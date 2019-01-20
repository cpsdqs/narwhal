use crate::render::TextureRef;
use cgmath::Matrix4;
use failure::Error;
use std::sync::Arc;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer, CpuBufferPool};
use vulkano::command_buffer::pool::standard::StandardCommandPoolBuilder;
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::descriptor::descriptor_set::FixedSizeDescriptorSetsPool;
use vulkano::descriptor::PipelineLayoutAbstract;
use vulkano::device::Device;
use vulkano::framebuffer::{RenderPassAbstract, Subpass};
use vulkano::pipeline::vertex::SingleBufferDefinition;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::sampler::{BorderColor, Filter, MipmapMode, Sampler, SamplerAddressMode};

mod tex_vert {
    vulkano_shaders::shader!(ty: "vertex", path: "src/shaders/composite_tex.vert");
}

mod tex_frag {
    vulkano_shaders::shader!(ty: "fragment", path: "src/shaders/composite_tex.frag");
}

use self::tex_vert::ty::CompTexUniforms;

type CompTexPipeline = Arc<
    GraphicsPipeline<
        SingleBufferDefinition<CompTexVertex>,
        Box<dyn PipelineLayoutAbstract + Send + Sync>,
        Arc<dyn RenderPassAbstract + Send + Sync>,
    >,
>;

#[repr(C)]
struct Globals {
    camera: Matrix4<f32>,
}

#[repr(C)]
struct CompTexVertex {
    a_position: [f32; 4],
}

impl_vertex!(CompTexVertex, a_position);

/// Draws textures into the current framebuffer.
pub struct TexCompositor {
    device: Arc<Device>,
    global_pool: CpuBufferPool<Globals>,
    comp_tex_pipeline: CompTexPipeline,
    tex_ds_pool: FixedSizeDescriptorSetsPool<CompTexPipeline>,
    tex_sampler: Arc<Sampler>,
}

impl TexCompositor {
    /// Creates a texture compositor.
    pub fn new(
        device: Arc<Device>,
        render_pass: &Arc<RenderPassAbstract + Send + Sync>,
        subpass: u32,
    ) -> Result<TexCompositor, Error> {
        let comp_tex_vs = tex_vert::Shader::load(Arc::clone(&device))?;
        let comp_tex_fs = tex_frag::Shader::load(Arc::clone(&device))?;

        let comp_tex_pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<CompTexVertex>()
                .vertex_shader(comp_tex_vs.main_entry_point(), ())
                .viewports_scissors_dynamic(1)
                .fragment_shader(comp_tex_fs.main_entry_point(), ())
                .blend_alpha_blending()
                .depth_write(true)
                .triangle_strip()
                .render_pass(Subpass::from(Arc::clone(render_pass), subpass).unwrap())
                .build(Arc::clone(&device))?,
        );

        let tex_ds_pool = FixedSizeDescriptorSetsPool::new(Arc::clone(&comp_tex_pipeline), 0);

        let tex_sampler = Sampler::new(
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

        Ok(TexCompositor {
            comp_tex_pipeline,
            tex_ds_pool,
            global_pool: CpuBufferPool::uniform_buffer(Arc::clone(&device)),
            device,
            tex_sampler,
        })
    }

    /// Renders a texture.
    pub fn draw(
        &mut self,
        mut cmd_buffer: AutoCommandBufferBuilder<StandardCommandPoolBuilder>,
        texture: &TextureRef,
        dyn_state: &DynamicState,
        camera: Matrix4<f32>,
    ) -> Result<AutoCommandBufferBuilder<StandardCommandPoolBuilder>, Error> {
        // FIXME: should cache most of this stuff
        let globals = self.global_pool.next(Globals { camera })?;
        let size = texture.size();

        // TODO: something about the depth buffer? maybe?

        let verts = CpuAccessibleBuffer::from_iter(
            Arc::clone(&self.device),
            BufferUsage::vertex_buffer(),
            [
                [0., 0., 0., 0.],
                [size.x as f32, 0., 1., 0.],
                [0., size.y as f32, 0., 1.],
                [size.x as f32, size.y as f32, 1., 1.],
            ]
            .iter()
            .map(|v| CompTexVertex { a_position: *v }),
        )
        .map_err(|e| Error::from(e))?;
        let uniform_buffer = CpuAccessibleBuffer::from_data(
            Arc::clone(&self.device),
            BufferUsage::uniform_buffer(),
            CompTexUniforms {
                transform: (*texture.transform()).into(),
            },
        )
        .map_err(|e| Error::from(e))?;
        let set = self
            .tex_ds_pool
            .next()
            .add_buffer(globals)
            .map_err(|e| Error::from(e))?
            .add_buffer(uniform_buffer)
            .map_err(|e| Error::from(e))?
            .add_sampled_image(texture.color().clone(), Arc::clone(&self.tex_sampler))
            .map_err(|e| Error::from(e))?
            .build()
            .map_err(|e| Error::from(e))?;

        cmd_buffer = cmd_buffer
            .draw(
                Arc::clone(&self.comp_tex_pipeline),
                dyn_state,
                verts,
                set,
                (),
            )
            .map_err(|e| Error::from(e))?;

        Ok(cmd_buffer)
    }
}
