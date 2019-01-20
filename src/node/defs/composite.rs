use crate::data::{Color, Value};
use crate::eval::*;
use crate::node::NodeRef;
use crate::render::{ShapeRasterizer, TexCompositor, TextureRef, COLOR_FORMAT, DEPTH_FORMAT};
use failure::Error;
use std::sync::{Arc, Mutex};
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::device::{Device, Queue};
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract};
use vulkano::pipeline::viewport::{Scissor, Viewport};

pub static COMPOSITE: NodeTypeDef = NodeTypeDef::Graphics(CompositeType::new);
pub const COMPOSITE_NAME: &str = "narwhal.composite";

// TODO: GC rasterizer

#[derive(Clone)]
struct Shared {
    tex_comp: Arc<Mutex<TexCompositor>>,
    rasterizer: Arc<Mutex<ShapeRasterizer<(NodeRef, u64)>>>,
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
}

struct CompositeType {
    shared: Shared,
}

impl CompositeType {
    fn new(device: &Arc<Device>, _: &Arc<Queue>) -> Result<Box<dyn SharedGraphicsType>, Error> {
        let render_pass: Arc<dyn RenderPassAbstract + Send + Sync> =
            Arc::new(single_pass_renderpass! {
                Arc::clone(&device),
                attachments: {
                    color: {
                        load: Clear,
                        store: Store,
                        format: COLOR_FORMAT,
                        samples: 1,
                    },
                    depth: {
                        load: Clear,
                        store: Store,
                        format: DEPTH_FORMAT,
                        samples: 1,
                    }
                },
                pass: {
                    color: [color],
                    depth_stencil: {depth}
                }
            }?);

        let tex_comp = Arc::new(Mutex::new(TexCompositor::new(
            Arc::clone(&device),
            &render_pass,
            0,
        )?));

        let rasterizer = Arc::new(Mutex::new(ShapeRasterizer::new(
            Arc::clone(&device),
            &render_pass,
            0,
        )?));

        Ok(Box::new(CompositeType {
            shared: Shared {
                tex_comp,
                rasterizer,
                render_pass,
            },
        }))
    }
}

impl SharedGraphicsType for CompositeType {
    fn name(&self) -> String {
        COMPOSITE_NAME.into()
    }

    fn create(&mut self) -> Box<dyn GraphicsNode> {
        Box::new(CompositeNode {
            shared: self.shared.clone(),
            output: None,
            framebuffer: None,
        })
    }
}

struct CompositeNode {
    shared: Shared,
    output: Option<TextureRef>,
    framebuffer: Option<Arc<dyn FramebufferAbstract + Send + Sync>>,
}

#[repr(usize)]
pub enum CompositeProps {
    In = 0,
    Out = 1,
}

impl Into<usize> for CompositeProps {
    fn into(self) -> usize {
        self as usize
    }
}

impl GraphicsNode for CompositeNode {
    fn eval(
        &mut self,
        input: Input,
        mut context: NodeContext,
        output: &mut Output,
        mut cmd_buffer: AutoCommandBufferBuilder,
    ) -> EvalResult<AutoCommandBufferBuilder> {
        let size = (context.camera().width, context.camera().height).into();
        let resolution = context.resolution();

        if self.output.as_ref().map_or(true, |tex| {
            tex.size() != size || tex.resolution() != resolution
        }) {
            let output = context.new_attachment(size.x, size.y, resolution)?;
            self.framebuffer = Some(Arc::new(
                Framebuffer::start(Arc::clone(&self.shared.render_pass))
                    .add(output.color().clone())?
                    .add(output.depth().unwrap().clone())?
                    .build()?,
            ));
            self.output = Some(output);
        }

        let framebuffer = self.framebuffer.as_ref().unwrap();

        output.set(CompositeProps::Out, self.output.as_ref().unwrap().clone());

        if let Ok(in_values) = input.get(CompositeProps::In) {
            cmd_buffer = cmd_buffer.begin_render_pass(
                Arc::clone(framebuffer),
                false,
                vec![Color::CLEAR.into(), 0.0.into()],
            )?;

            let camera = context.camera().matrix();
            let px_width = size.x * resolution;
            let px_height = size.y * resolution;

            let scissor = Scissor {
                origin: [0, 0],
                dimensions: [px_width as u32, px_height as u32],
            };
            let viewport = Viewport {
                origin: [0., 0.],
                dimensions: [px_width, px_height],
                depth_range: 0.0..1.0,
            };

            let dyn_state = DynamicState {
                line_width: None,
                scissors: Some(vec![scissor]),
                viewports: Some(vec![viewport]),
            };

            let mut tex_comp = self.shared.tex_comp.lock().unwrap();
            let mut rasterizer = self.shared.rasterizer.lock().unwrap();

            for value in in_values {
                match &**value {
                    Value::Texture(texture) => {
                        cmd_buffer = tex_comp.draw(cmd_buffer, &texture, &dyn_state, camera)?;
                    }
                    Value::Drawables(drawables) => {
                        for drawable in drawables {
                            cmd_buffer = rasterizer.draw(
                                cmd_buffer,
                                drawable.id,
                                &drawable.shape,
                                &dyn_state,
                                camera,
                            )?;
                        }
                    }
                    _ => return Err(EvalError::InputType(CompositeProps::In.into())),
                }
            }

            cmd_buffer = cmd_buffer.end_render_pass()?;
        }

        Ok(cmd_buffer)
    }
}
