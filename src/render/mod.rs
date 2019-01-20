//! Rendering.

pub mod fx;
mod presenter;
mod renderer;
mod shape;
pub mod stroke_tess;
mod swapchain_renderer;
mod tex_comp;
mod texture;

pub use self::presenter::*;
pub use self::renderer::*;
pub use self::shape::*;
pub use self::tex_comp::*;
pub use self::texture::*;

use crate::data::Camera;
use vulkano::format::Format;

/// The color format; RGBA half-floats.
pub const COLOR_FORMAT: Format = Format::R16G16B16A16Sfloat;

/// The depth format; 32-bit float.
pub const DEPTH_FORMAT: Format = Format::D32Sfloat;

/// Context data for rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Context {
    /// The viewport camera.
    pub camera: Camera,

    /// The rasterization resolution, usually 2 (high-dpi screens) or 1. However, this may also be a
    /// dynamically generated value and should be handled with caution (i.e. clamping to a sane
    /// range).
    pub resolution: f32,
}

impl Context {
    /// Tries to merge two contexts by taking the max. viewport size and resolution.
    pub fn merge(&mut self, other: Context) {
        if self == &other {
            return;
        }

        let left = self.camera.offset.x;
        let top = self.camera.offset.y;
        let right = left + self.camera.width;
        let bottom = top + self.camera.height;

        let other_left = other.camera.offset.x;
        let other_top = other.camera.offset.y;
        let other_right = left + other.camera.width;
        let other_bottom = top + other.camera.height;

        self.camera.offset.x = left.min(other_left);
        self.camera.offset.y = top.min(other_top);
        self.camera.width = right.max(other_right) - self.camera.offset.x;
        self.camera.height = bottom.max(other_bottom) - self.camera.offset.y;

        self.camera.clip_near = self.camera.clip_near.min(other.camera.clip_near);
        self.camera.clip_far = self.camera.clip_far.max(other.camera.clip_far);

        // the rest of the camera attributes can’t be merged very well
        // so just keep them, I guess

        self.resolution = self.resolution.max(other.resolution);
    }
}
