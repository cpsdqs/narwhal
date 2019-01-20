use crate::data::Color;
use crate::platform::NarwhalSurface;
use crate::render::Texture;
use failure::Error;
use std::sync::Arc;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::descriptor::descriptor_set::FixedSizeDescriptorSetsPool;
use vulkano::descriptor::PipelineLayoutAbstract;
use vulkano::device::Device;
use vulkano::format::Format;
use vulkano::framebuffer::{Framebuffer, RenderPassAbstract, Subpass};
use vulkano::image::SwapchainImage;
use vulkano::pipeline::vertex::SingleBufferDefinition;
use vulkano::pipeline::viewport::Viewport;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::sampler::{BorderColor, Filter, MipmapMode, Sampler, SamplerAddressMode};

mod render_vs {
    vulkano_shaders::shader!(ty: "vertex", src: "
#version 450
layout(location = 0) in vec2 a_position;
layout(location = 0) out vec2 v_position;
void main() {
    v_position = a_position / vec2(2, -2) + vec2(0.5);
    gl_Position = vec4(a_position, 0, 1);
}
    ");
}

mod render_fs {
    vulkano_shaders::shader!(ty: "fragment", src: "
#version 450
layout(location = 0) in vec2 v_position;
layout(binding = 0) uniform sampler2D u_image;
layout(location = 0) out vec4 out_color;
void main() {
    out_color = texture(u_image, v_position);
    out_color.rgb *= out_color.a;
}
    ");
}

#[repr(C)]
struct Vertex {
    a_position: [f32; 2],
}

impl_vertex!(Vertex, a_position);

type CTGraphicsPipeline = Arc<
    GraphicsPipeline<
        SingleBufferDefinition<Vertex>,
        Box<dyn PipelineLayoutAbstract + Send + Sync>,
        Arc<dyn RenderPassAbstract + Send + Sync>,
    >,
>;

/// Renders a texture onto a swapchain image, because apparently swapchain images can’t be used as
/// transfer targets.
pub(crate) struct SwapchainRenderer {
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    graphics_pipeline: CTGraphicsPipeline,
    graphics_ds_pool: FixedSizeDescriptorSetsPool<CTGraphicsPipeline>,
    vertex_buf: Arc<CpuAccessibleBuffer<[Vertex]>>,
    input_sampler: Arc<Sampler>,
}

impl SwapchainRenderer {
    pub fn new(device: Arc<Device>, output_format: Format) -> Result<SwapchainRenderer, Error> {
        let render_vs = render_vs::Shader::load(Arc::clone(&device))?;
        let render_fs = render_fs::Shader::load(Arc::clone(&device))?;

        let render_pass: Arc<dyn RenderPassAbstract + Send + Sync> =
            Arc::new(single_pass_renderpass! {
                Arc::clone(&device),
                attachments: {
                    color: {
                        load: Clear,
                        store: Store,
                        format: output_format,
                        samples: 1,
                    }
                },
                pass: {
                    color: [color],
                    depth_stencil: {}
                }
            }?);

        let vertex_buf = CpuAccessibleBuffer::from_iter(
            Arc::clone(&device),
            BufferUsage::vertex_buffer(),
            [[-1., -1.], [1., -1.], [-1., 1.], [1., 1.]]
                .into_iter()
                .map(|x| Vertex { a_position: *x }),
        )?;

        let graphics_pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<Vertex>()
                .vertex_shader(render_vs.main_entry_point(), ())
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(render_fs.main_entry_point(), ())
                .render_pass(Subpass::from(Arc::clone(&render_pass), 0).unwrap())
                .triangle_strip()
                .build(Arc::clone(&device))?,
        );

        let graphics_ds_pool = FixedSizeDescriptorSetsPool::new(Arc::clone(&graphics_pipeline), 0);

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

        Ok(SwapchainRenderer {
            render_pass,
            graphics_pipeline,
            graphics_ds_pool,
            vertex_buf,
            input_sampler,
        })
    }

    pub fn render(
        &mut self,
        mut cmd_buffer: AutoCommandBufferBuilder,
        input: &Texture,
        output: &Arc<SwapchainImage<NarwhalSurface>>,
    ) -> Result<AutoCommandBufferBuilder, Error> {
        let size = output.dimensions();

        let framebuffer = Arc::new(
            Framebuffer::start(Arc::clone(&self.render_pass))
                .add(Arc::clone(&output))?
                .build()?,
        );

        let render_set = self
            .graphics_ds_pool
            .next()
            .add_sampled_image(input.clone(), Arc::clone(&self.input_sampler))?
            .build()?;

        cmd_buffer = cmd_buffer
            .begin_render_pass(framebuffer, false, vec![Color::CLEAR.into()])?
            .draw(
                Arc::clone(&self.graphics_pipeline),
                &DynamicState {
                    viewports: Some(vec![Viewport {
                        origin: [0., 0.],
                        dimensions: [size[0] as f32, size[1] as f32],
                        depth_range: 0.0..1.0,
                    }]),
                    ..DynamicState::none()
                },
                Arc::clone(&self.vertex_buf),
                render_set,
                (),
            )?
            .end_render_pass()?;

        Ok(cmd_buffer)
    }
}
