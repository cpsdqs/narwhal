use crate::data::{Color, Path2D, StrokeWeight};
use cgmath::Matrix4;

/// A 2D shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub path: Path2D,
    pub stroke: Option<(StrokeWeight, f32, Color)>,
    pub fill: Option<Color>,
    pub transform: Option<Matrix4<f32>>,
}
