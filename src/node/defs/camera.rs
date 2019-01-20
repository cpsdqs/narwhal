use crate::data::cgmath_ext::{Matrix4Ext, Vector2Ext};
use crate::data::{Camera, Value};
use crate::eval::*;
use cgmath::{Matrix4, Vector2};
use std::sync::Arc;

pub static CAMERA: NodeTypeDef = NodeTypeDef::Data(CameraType::new);
pub const CAMERA_NAME: &str = "narwhal.camera";

struct CameraType;

impl CameraType {
    fn new() -> Box<dyn SharedDataType> {
        Box::new(CameraType)
    }
}

impl SharedDataType for CameraType {
    fn name(&self) -> String {
        CAMERA_NAME.into()
    }

    fn create(&mut self) -> Box<dyn DataNode> {
        Box::new(CameraNode)
    }
}

struct CameraNode;

#[repr(usize)]
pub enum CameraProps {
    In = 0,
    Size = 1,
    Offset = 2,
    Transform = 3,
    Fov = 4,
    ClipNear = 5,
    ClipFar = 6,
}

impl Into<usize> for CameraProps {
    fn into(self) -> usize {
        self as usize
    }
}

impl DataNode for CameraNode {
    fn eval(&mut self, input: Input, output: &mut Output) -> EvalResult<()> {
        let mut camera = Camera::new();

        let size = input.one::<_, Vector2<f64>>(CameraProps::Size)?.into_f32();
        camera.width = size.x;
        camera.height = size.y;

        camera.offset = input
            .one::<_, Vector2<f64>>(CameraProps::Offset)?
            .into_f32();
        camera.transform = input
            .one::<_, Matrix4<f64>>(CameraProps::Transform)?
            .into_f32();
        camera.fov = *input.one::<_, f64>(CameraProps::Fov)? as f32;
        camera.clip_near = *input.one::<_, f64>(CameraProps::ClipNear)? as f32;
        camera.clip_far = *input.one::<_, f64>(CameraProps::ClipFar)? as f32;

        output.set(0_usize, Value::Any(Arc::new(camera)));

        Ok(())
    }
}
