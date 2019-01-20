//! Gaussian blur compute shader.

use crate::eval::EvalError;
use crate::render::Texture;
use failure::Error;
use std::sync::Arc;
use vulkano::command_buffer::pool::standard::StandardCommandPoolBuilder;
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::descriptor::descriptor_set::FixedSizeDescriptorSetsPool;
use vulkano::device::Device;
use vulkano::format::Format;
use vulkano::image::{Dimensions, StorageImage};
use vulkano::pipeline::{ComputePipeline, ComputePipelineAbstract};
use vulkano::sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode};

const LOCAL_SIZE_X: f32 = 16.;
const LOCAL_SIZE_Y: f32 = 16.;

mod shader {
    vulkano_shaders::shader!(ty: "compute", path: "src/shaders/gaussian_blur.comp");
}

use self::shader::ty::Data;

/// Multi-pass gaussian blur.
pub struct GaussianBlur {
    pipeline: Arc<dyn ComputePipelineAbstract + Send + Sync>,
    sampler: Arc<Sampler>,
    ds_pool: FixedSizeDescriptorSetsPool<Arc<dyn ComputePipelineAbstract + Send + Sync>>,
}

impl GaussianBlur {
    /// Compiles shaders and creates a pipeline.
    pub fn new(device: Arc<Device>) -> Result<GaussianBlur, Error> {
        let shader = shader::Shader::load(Arc::clone(&device))?;

        let pipeline: Arc<dyn ComputePipelineAbstract + Send + Sync> = Arc::new(
            ComputePipeline::new(Arc::clone(&device), &shader.main_entry_point(), &())?,
        );

        let ds_pool = FixedSizeDescriptorSetsPool::new(Arc::clone(&pipeline), 0);

        let sampler = Sampler::new(
            Arc::clone(&device),
            Filter::Linear,
            Filter::Linear,
            MipmapMode::Nearest,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            0.,
            1.,
            0.,
            0.,
        )?;

        Ok(GaussianBlur {
            pipeline,
            sampler,
            ds_pool,
        })
    }

    fn dispatch_pass(
        &mut self,
        mut cmd_buffer: AutoCommandBufferBuilder<StandardCommandPoolBuilder>,
        input: &Texture,
        output: &Arc<StorageImage<Format>>,
        filter_size: f32,
        vertical: bool,
    ) -> Result<AutoCommandBufferBuilder<StandardCommandPoolBuilder>, Error> {
        let (width, height) = match output.dimensions() {
            Dimensions::Dim2d { width, height } => (width, height),
            _ => return Err(EvalError::Input("Unsupported texture dimensions".into()).into()),
        };

        let set = self
            .ds_pool
            .next()
            .add_sampled_image(input.clone(), Arc::clone(&self.sampler))?
            .add_image(Arc::clone(&output))?
            .build()?;

        cmd_buffer = cmd_buffer.dispatch(
            [
                (width as f32 / LOCAL_SIZE_X).ceil() as u32,
                (height as f32 / LOCAL_SIZE_Y).ceil() as u32,
                1,
            ],
            Arc::clone(&self.pipeline),
            set,
            Data {
                size: filter_size,
                vertical: if vertical { 1 } else { 0 },
            },
        )?;

        Ok(cmd_buffer)
    }

    /// Dispatches the gaussian blur shader in the command buffer.
    ///
    /// Because this is a multi-pass gaussian blur (i.e. horizontal and vertical) a third attachment
    /// is required for temporary storage.
    ///
    /// To set the blur radius, use `radius_px`. Note that for the blur to be of decent quality,
    /// `passes` should be increased as well. The shader is approximately a 9-tap gaussian blur,
    /// so `radius / 4.5` would yield optimal (but potentially costly) results.
    /// Also note that `radius_px` should usually be multiplied by the context resolution since
    /// this shader operates on actual pixels.
    pub fn dispatch(
        &mut self,
        mut cmd_buffer: AutoCommandBufferBuilder<StandardCommandPoolBuilder>,
        input: &Texture,
        intermediate: &Arc<StorageImage<Format>>,
        output: &Arc<StorageImage<Format>>,
        radius_px: f32,
        passes: u8,
    ) -> Result<AutoCommandBufferBuilder<StandardCommandPoolBuilder>, Error> {
        // times 2 because this is the radius (not the diameter)
        // 9 because the shader is a 9-tap filter
        let filter_size = radius_px * 2. / (9. * passes as f32);

        let intermediate_tex = Texture::Storage(Arc::clone(&intermediate));
        let output_tex = Texture::Storage(Arc::clone(&output));

        // TODO: keep descriptor sets around instead of recreating each pass
        if passes > 0 {
            cmd_buffer = self.dispatch_pass(cmd_buffer, input, intermediate, filter_size, false)?;
            cmd_buffer =
                self.dispatch_pass(cmd_buffer, &intermediate_tex, output, filter_size, true)?;
        }
        for _ in 1..passes {
            cmd_buffer =
                self.dispatch_pass(cmd_buffer, &output_tex, intermediate, filter_size, false)?;
            cmd_buffer =
                self.dispatch_pass(cmd_buffer, &intermediate_tex, output, filter_size, true)?;
        }
        Ok(cmd_buffer)
    }
}
