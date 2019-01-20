use cgmath::{self, Euler, Matrix4, Rad, SquareMatrix, Vector2, Zero};
use std::f32;

/// A perspective camera.
///
/// Assuming sensor height 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Viewport offset.
    pub offset: Vector2<f32>,

    /// Viewport width.
    pub width: f32,

    /// Viewport height.
    pub height: f32,

    /// Transform matrix.
    pub transform: Matrix4<f32>,

    /// Field of view in radians.
    pub fov: f32,

    /// Near clip plane.
    pub clip_near: f32,

    /// Far clip plane.
    pub clip_far: f32,
}

impl Camera {
    /// Creates a new camera with 90° FOV at (0, 0, 0.5) looking into +Z with 0.01
    /// and 100 as clip planes, and a 1px×1px viewport.
    pub fn new() -> Camera {
        let rotation: Matrix4<f32> = Euler {
            x: Rad(0.),
            y: Rad(0.),
            z: Rad(0.),
        }
        .into();
        let position = Matrix4::from_translation((0., 0., 0.5).into());

        Camera {
            offset: Vector2::zero(),
            width: 1.,
            height: 1.,
            transform: rotation * position,
            fov: f32::consts::PI / 2.,
            clip_near: 0.01,
            clip_far: 100.,
        }
    }

    /// The distance at which pixels map 1:1 to the screen.
    pub fn focal_length(&self) -> f32 {
        let d = 1.;
        d / (2. * (self.fov / 2.).tan())
    }

    /// Sets the field of view from the focal length.
    pub fn set_focal_length(&mut self, f: f32) {
        let d = 1.;
        self.fov = 2. * (d / (2. * f)).atan()
    }

    /// Returns a view-perspective matrix.
    pub fn matrix(&self) -> Matrix4<f32> {
        let aspect = self.width / self.height;
        let persp = cgmath::perspective(Rad(self.fov), aspect, self.clip_near, self.clip_far);
        let scale = Matrix4::from_scale(1. / self.height);
        let offset = Matrix4::from_translation((self.offset.x, self.offset.y, 0.).into());
        let transform = self
            .transform
            .invert()
            .unwrap_or(Matrix4::from_translation((0., 0., -0.5).into()));
        persp * (scale * transform * offset)
    }
}
