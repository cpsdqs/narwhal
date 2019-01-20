use crate::eval::*;
use crate::render::fx::GaussianBlur;
use crate::render::TextureRef;
use failure::Error;
use std::f32;
use std::sync::{Arc, Mutex};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::device::{Device, Queue};

pub static GAUSSIAN_BLUR: NodeTypeDef = NodeTypeDef::Graphics(GaussianType::new);
pub const GAUSSIAN_BLUR_NAME: &str = "narwhal.gaussian-blur";

struct GaussianType {
    inner: Arc<Mutex<GaussianBlur>>,
}

impl GaussianType {
    fn new(device: &Arc<Device>, _: &Arc<Queue>) -> Result<Box<dyn SharedGraphicsType>, Error> {
        Ok(Box::new(GaussianType {
            inner: Arc::new(Mutex::new(GaussianBlur::new(Arc::clone(device))?)),
        }))
    }
}

impl SharedGraphicsType for GaussianType {
    fn name(&self) -> String {
        GAUSSIAN_BLUR_NAME.into()
    }

    fn create(&mut self) -> Box<dyn GraphicsNode> {
        Box::new(GaussianNode {
            inner: Arc::clone(&self.inner),
            textures: None,
        })
    }
}

struct GaussianNode {
    inner: Arc<Mutex<GaussianBlur>>,
    textures: Option<(TextureRef, TextureRef)>,
}

#[repr(usize)]
pub enum GaussianProps {
    In = 0,
    Out = 1,
    Radius = 2,
}

impl Into<usize> for GaussianProps {
    fn into(self) -> usize {
        self as usize
    }
}

const MIN_RADIUS: f32 = 0.1;

impl GraphicsNode for GaussianNode {
    fn eval(
        &mut self,
        input: Input,
        mut context: NodeContext,
        output: &mut Output,
        mut cmd_buffer: AutoCommandBufferBuilder,
    ) -> EvalResult<AutoCommandBufferBuilder> {
        let radius = *input.one::<_, f64>(GaussianProps::Radius)? as f32 * context.resolution();
        let (input_size, input_resolution) = {
            let input = input.one::<_, TextureRef>(GaussianProps::In)?;
            (input.size(), input.resolution())
        };

        if radius < MIN_RADIUS {
            output.set(
                GaussianProps::Out,
                input.one::<_, TextureRef>(GaussianProps::In)?.clone(),
            );
            return Ok(cmd_buffer);
        }

        if self.textures.as_ref().map_or(true, |(tex, _)| {
            tex.size() != input_size || tex.resolution() != input_resolution
        }) {
            let intermediate =
                context.new_storage_texture(input_size.x, input_size.y, input_resolution)?;
            let output_tex =
                context.new_storage_texture(input_size.x, input_size.y, input_resolution)?;
            self.textures = Some((intermediate, output_tex));
        }

        let (intermediate, output_tex) = self.textures.as_ref().unwrap();

        let input_tex: &TextureRef = input.one(GaussianProps::In)?;

        // FIXME: what about the depth channel?

        // fixed pass count for now (TODO: quality config?)
        // TODO: adjust this curve more nicely (this one was just eyeballed)
        let pass_count = (4. - f32::consts::E.powf(1.5 - radius / 9.))
            .round()
            .max(1.) as u8;

        cmd_buffer = self.inner.lock().unwrap().dispatch(
            cmd_buffer,
            input_tex.color(),
            intermediate.color().as_storage()?,
            output_tex.color().as_storage()?,
            radius,
            pass_count,
        )?;

        output.set(GaussianProps::Out, output_tex.clone());
        Ok(cmd_buffer)
    }
}
