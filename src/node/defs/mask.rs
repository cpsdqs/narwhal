use crate::eval::*;
use crate::render::fx::{Mask, MaskMode};
use crate::render::TextureRef;
use failure::Error;
use std::sync::{Arc, Mutex};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::device::{Device, Queue};

pub static MASK: NodeTypeDef = NodeTypeDef::Graphics(MaskType::new);
pub const MASK_NAME: &str = "narwhal.mask";

struct MaskType {
    inner: Arc<Mutex<Mask>>,
}

impl MaskType {
    fn new(device: &Arc<Device>, _: &Arc<Queue>) -> Result<Box<dyn SharedGraphicsType>, Error> {
        Ok(Box::new(MaskType {
            inner: Arc::new(Mutex::new(Mask::new(Arc::clone(device))?)),
        }))
    }
}

impl SharedGraphicsType for MaskType {
    fn name(&self) -> String {
        MASK_NAME.into()
    }

    fn create(&mut self) -> Box<dyn GraphicsNode> {
        Box::new(MaskNode {
            inner: Arc::clone(&self.inner),
            output_tex: None,
        })
    }
}

struct MaskNode {
    inner: Arc<Mutex<Mask>>,
    output_tex: Option<TextureRef>,
}

#[repr(usize)]
pub enum MaskProps {
    In = 0,
    Out = 1,
    Mask = 2,
    Mode = 3,
}

impl Into<usize> for MaskProps {
    fn into(self) -> usize {
        self as usize
    }
}

impl GraphicsNode for MaskNode {
    fn eval(
        &mut self,
        input: Input,
        mut context: NodeContext,
        output: &mut Output,
        mut cmd_buffer: AutoCommandBufferBuilder,
    ) -> EvalResult<AutoCommandBufferBuilder> {
        if input.get(MaskProps::Mask).is_err() {
            // no mask input
            output.set(
                MaskProps::Out,
                input.one::<_, TextureRef>(MaskProps::In)?.clone(),
            );
            return Ok(cmd_buffer);
        }

        let (input_size, input_resolution) = {
            let input = input.one::<_, TextureRef>(MaskProps::In)?;
            (input.size(), input.resolution())
        };

        if self.output_tex.as_ref().map_or(true, |tex| {
            tex.size() != input_size || tex.resolution() != input_resolution
        }) {
            let output_tex =
                context.new_storage_texture(input_size.x, input_size.y, input_resolution)?;
            self.output_tex = Some(output_tex);
        }

        let output_tex = self.output_tex.as_ref().unwrap();
        let input_tex: &TextureRef = input.one(MaskProps::In)?;
        let mask: &TextureRef = input.one(MaskProps::Mask)?;
        let mode = *input.one_any::<_, MaskMode>(MaskProps::Mode)?;

        // FIXME: what about the depth channel?

        cmd_buffer = self.inner.lock().unwrap().dispatch(
            cmd_buffer,
            input_tex.color(),
            mask.color(),
            output_tex.color().as_storage()?,
            mode,
        )?;

        output.set(MaskProps::Out, output_tex.clone());
        Ok(cmd_buffer)
    }
}
