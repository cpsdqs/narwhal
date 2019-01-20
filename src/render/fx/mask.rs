//! Mask compute shader.

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
use vulkano::sampler::{BorderColor, Filter, MipmapMode, Sampler, SamplerAddressMode};

const LOCAL_SIZE_X: f32 = 16.;
const LOCAL_SIZE_Y: f32 = 16.;

mod shader {
    vulkano_shaders::shader!(ty: "compute", path: "src/shaders/mask.comp");
}

use self::shader::ty::Data;

/// Mask modes.
///
/// Luma is obtained by taking the average of the three color channels multiplied by alpha.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MaskMode {
    /// Sets the texture alpha to the alpha of the matte.
    AlphaMatte = 0,

    /// Sets the texture alpha to the luma of the matte.
    LumaMatte = 1,

    /// Sets the texture alpha to the inverse of the alpha of the matte.
    AlphaCutter = 2,

    /// Sets the texture alpha to the inverse of the luma of the matte.
    LumaCutter = 3,
}

/// Mask/matte shader.
pub struct Mask {
    pipeline: Arc<dyn ComputePipelineAbstract + Send + Sync>,
    sampler: Arc<Sampler>,
    ds_pool: FixedSizeDescriptorSetsPool<Arc<dyn ComputePipelineAbstract + Send + Sync>>,
}

impl Mask {
    /// Compiles shaders and creates a pipeline.
    pub fn new(device: Arc<Device>) -> Result<Mask, Error> {
        let shader = shader::Shader::load(Arc::clone(&device))?;

        let pipeline: Arc<dyn ComputePipelineAbstract + Send + Sync> = Arc::new(
            ComputePipeline::new(Arc::clone(&device), &shader.main_entry_point(), &())?,
        );

        let ds_pool = FixedSizeDescriptorSetsPool::new(Arc::clone(&pipeline), 0);

        let sampler = Sampler::new(
            Arc::clone(&device),
            Filter::Nearest,
            Filter::Nearest,
            MipmapMode::Nearest,
            SamplerAddressMode::ClampToBorder(BorderColor::FloatTransparentBlack),
            SamplerAddressMode::ClampToBorder(BorderColor::FloatTransparentBlack),
            SamplerAddressMode::ClampToBorder(BorderColor::FloatTransparentBlack),
            0.,
            1.,
            0.,
            0.,
        )?;

        Ok(Mask {
            pipeline,
            sampler,
            ds_pool,
        })
    }

    /// Dispatches the mask shader in the command buffer.
    pub fn dispatch(
        &mut self,
        mut cmd_buffer: AutoCommandBufferBuilder<StandardCommandPoolBuilder>,
        input: &Texture,
        mask: &Texture,
        output: &Arc<StorageImage<Format>>,
        mode: MaskMode,
    ) -> Result<AutoCommandBufferBuilder<StandardCommandPoolBuilder>, Error> {
        let (width, height) = match output.dimensions() {
            Dimensions::Dim2d { width, height } => (width, height),
            _ => return Err(EvalError::Input("Unsupported texture dimensions".into()).into()),
        };

        let set = self
            .ds_pool
            .next()
            .add_sampled_image(input.clone(), Arc::clone(&self.sampler))?
            .add_sampled_image(mask.clone(), Arc::clone(&self.sampler))?
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
            Data { mode: mode as i32 },
        )?;

        Ok(cmd_buffer)
    }
}
