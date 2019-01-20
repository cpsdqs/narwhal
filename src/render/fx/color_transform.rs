use crate::render::Texture;
use failure::Error;
use half::f16;
use lcms_prime::pixel_format::RGBA;
use lcms_prime::Transform;
use std::sync::Arc;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::descriptor::descriptor_set::FixedSizeDescriptorSetsPool;
use vulkano::device::{Device, Queue};
use vulkano::format::Format;
use vulkano::image::{Dimensions, ImageUsage, StorageImage};
use vulkano::pipeline::{ComputePipeline, ComputePipelineAbstract};
use vulkano::sampler::{BorderColor, Filter, MipmapMode, Sampler, SamplerAddressMode};

const LOCAL_SIZE_X: f32 = 16.;
const LOCAL_SIZE_Y: f32 = 16.;

mod shader {
    vulkano_shaders::shader!(ty: "compute", path: "src/shaders/color_transform.comp");
}

use self::shader::ty::Data;

/// A color transform.
pub struct ColorTransform {
    pipeline: Arc<dyn ComputePipelineAbstract + Send + Sync>,
    ds_pool: FixedSizeDescriptorSetsPool<Arc<dyn ComputePipelineAbstract + Send + Sync>>,
    lut: Arc<StorageImage<Format>>,
    lut_buf: Arc<CpuAccessibleBuffer<[f16]>>,
    data_buf: Arc<CpuAccessibleBuffer<Data>>,
    input_sampler: Arc<Sampler>,
    lut_sampler: Arc<Sampler>,
    transform: Option<Transform<RGBA<f32>, RGBA<f32>>>,
    lut_resolution: u16,
    lut_bounds: (f32, f32),
    lut_needs_update: bool,
}

#[derive(Debug, Fail)]
enum DispatchError {
    #[fail(display = "invalid output dimensions (should be 2d)")]
    InvalidOutputDimensions,

    #[fail(display = "no color transform set")]
    NoTransform,
}

impl ColorTransform {
    /// Creates a new color transform.
    ///
    /// - `lut_resolution` is the resolution of the LUT *per unit*, and something like 1024 should
    ///   be fine
    /// - `lut_bounds` are the lower and upper bounds of the LUT. `(0, 1)` is fine if there are no
    ///   out-of-gamut colors
    pub fn new(
        device: Arc<Device>,
        queue: &Arc<Queue>,
        lut_resolution: u16,
        lut_bounds: (f32, f32),
    ) -> Result<ColorTransform, Error> {
        let shader = shader::Shader::load(Arc::clone(&device))?;

        let data_buf = CpuAccessibleBuffer::from_data(
            Arc::clone(&device),
            BufferUsage {
                uniform_buffer: true,
                storage_buffer: true,
                ..BufferUsage::none()
            },
            Data {
                lower_bound: lut_bounds.0,
                lut_range: lut_bounds.1 - lut_bounds.0,
            },
        )?;

        let lut_pixel_count = ((lut_bounds.1 - lut_bounds.0) * lut_resolution as f32) as usize;
        let lut_buf = CpuAccessibleBuffer::from_iter(
            Arc::clone(&device),
            BufferUsage {
                storage_buffer: true,
                transfer_source: true,
                ..BufferUsage::none()
            },
            (0..lut_pixel_count * 4)
                .into_iter()
                .map(|_| f16::from_f32(0.)),
        )?;

        let lut = StorageImage::with_usage(
            Arc::clone(&device),
            Dimensions::Dim2d {
                width: lut_pixel_count as u32,
                height: 1,
            },
            Format::R16G16B16A16Sfloat,
            ImageUsage {
                sampled: true,
                transfer_destination: true,
                ..ImageUsage::none()
            },
            Some(queue.family()),
        )?;

        let pipeline: Arc<dyn ComputePipelineAbstract + Send + Sync> = Arc::new(
            ComputePipeline::new(Arc::clone(&device), &shader.main_entry_point(), &())?,
        );

        let ds_pool = FixedSizeDescriptorSetsPool::new(Arc::clone(&pipeline), 0);

        let input_sampler = Sampler::new(
            Arc::clone(&device),
            Filter::Linear,
            Filter::Linear,
            MipmapMode::Linear,
            SamplerAddressMode::ClampToBorder(BorderColor::FloatTransparentBlack),
            SamplerAddressMode::ClampToBorder(BorderColor::FloatTransparentBlack),
            SamplerAddressMode::ClampToBorder(BorderColor::FloatTransparentBlack),
            0.,
            1.,
            0.,
            0.,
        )?;
        let lut_sampler = Sampler::new(
            Arc::clone(&device),
            Filter::Linear,
            Filter::Linear,
            MipmapMode::Linear,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            0.,
            1.,
            0.,
            0.,
        )?;

        Ok(ColorTransform {
            pipeline,
            ds_pool,
            lut,
            lut_buf,
            data_buf,
            input_sampler,
            lut_sampler,
            transform: None,
            lut_resolution,
            lut_bounds,
            lut_needs_update: true,
        })
    }

    /// Sets the color transform and updates the LUT.
    ///
    /// FIXME: sometimes color transforms are incorrect
    pub fn set_transform(
        &mut self,
        transform: Transform<RGBA<f32>, RGBA<f32>>,
    ) -> Result<(), Error> {
        self.transform = Some(transform);
        self.encode_pipeline()
    }

    fn encode_pipeline(&mut self) -> Result<(), Error> {
        if self.transform.is_none() {
            return Ok(());
        }

        let pixel_count = self.lut.dimensions().width();
        let mut pixels = Vec::with_capacity(pixel_count as usize * 4);
        for i in 0..pixel_count {
            let value = (i as f32 / self.lut_resolution as f32) + self.lut_bounds.0;
            pixels.push(value);
            pixels.push(value);
            pixels.push(value);
            pixels.push(1.);
        }

        let mut lut_pixels = Vec::with_capacity(pixel_count as usize * 4);
        lut_pixels.resize(pixel_count as usize * 4, 0.);
        self.transform
            .as_ref()
            .unwrap()
            .convert(&pixels, &mut lut_pixels);

        let mut lut_data = self.lut_buf.write()?;

        for i in 0..lut_pixels.len() {
            lut_data[i] = f16::from_f32(lut_pixels[i]);
        }

        self.lut_needs_update = true;

        Ok(())
    }

    /// Dispatches the color transform compute shader.
    ///
    /// # Errors
    /// - when `set_transform` hasn’t been called to set a transform
    /// - when the output texture is not 2D
    /// - when Vulkan decides to raise an error
    pub fn dispatch(
        &mut self,
        mut cmd_buffer: AutoCommandBufferBuilder,
        input: &Texture,
        output: &Arc<StorageImage<Format>>,
    ) -> Result<AutoCommandBufferBuilder, Error> {
        if self.transform.is_none() {
            return Err(DispatchError::NoTransform.into());
        }

        let (width, height) = match output.dimensions() {
            Dimensions::Dim2d { width, height } => (width, height),
            _ => return Err(DispatchError::InvalidOutputDimensions.into()),
        };

        if self.lut_needs_update {
            cmd_buffer = cmd_buffer
                .copy_buffer_to_image(Arc::clone(&self.lut_buf), Arc::clone(&self.lut))
                .map_err(|e| Error::from(e))?;

            self.lut_needs_update = false;
        }

        let set = self
            .ds_pool
            .next()
            .add_buffer(Arc::clone(&self.data_buf))?
            .add_sampled_image(input.clone(), Arc::clone(&self.input_sampler))?
            .add_image(Arc::clone(&output))?
            .add_sampled_image(Arc::clone(&self.lut), Arc::clone(&self.lut_sampler))?
            .build()?;

        cmd_buffer = cmd_buffer.dispatch(
            [
                (width as f32 / LOCAL_SIZE_X).ceil() as u32,
                (height as f32 / LOCAL_SIZE_Y).ceil() as u32,
                1,
            ],
            Arc::clone(&self.pipeline),
            set,
            (),
        )?;

        Ok(cmd_buffer)
    }
}
