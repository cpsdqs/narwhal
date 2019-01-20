//! Window creation and application lifecycle stuff for Narwhal.
//!
//! The API is the same across all platforms but some functions may be no-ops in some
//! implementations.
//!
//! # Event Loop
//! Important to note is that the event loop is managed by the native API such that most app
//! lifecycle handling comes for free, and so all application logic must be dispatched from a
//! callback.
//!
//! Each object that emits events will call its callback function immediately after the event is
//! added to the event queue.
//!
//! [Window]s also support scheduling a callback to be called after a delay.
//!
//! # Additional Notes
//! - The origin of the coordinate system for windows is at the bottom left of the primary screen,
//!   with positive X going right and positive Y going upwards

extern crate cgmath;
#[macro_use]
extern crate log;
extern crate vulkano;
#[macro_use]
extern crate lazy_static;

#[macro_use]
#[cfg(target_os = "macos")]
extern crate objc;
#[cfg(target_os = "macos")]
extern crate cocoa as cocoa_ffi;
#[cfg(target_os = "macos")]
extern crate vk_sys;

#[cfg(target_os = "linux")]
extern crate smithay_client_toolkit;
#[cfg(target_os = "linux")]
extern crate wayland_client;
#[cfg(target_os = "linux")]
extern crate wayland_protocols;

#[cfg(target_os = "macos")]
mod cocoa;
#[cfg(target_os = "macos")]
pub use self::cocoa::*;

#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "linux")]
pub use self::wayland::*;

pub mod event;

// -- platform-independent dispatch impls --

use crate::event::{AppEvent, WindowEvent};
use cgmath::Vector2;
use std::sync::Arc;
use std::time::Duration;
use vulkano::instance::Instance;
use vulkano::swapchain::Surface;

impl App {
    /// Initializes the application instance, must be called only once. The name string and the
    /// version is for the application name in Vulkan’s application info.
    ///
    /// The returned value *must not be moved out of its box*.
    ///
    /// # Panics
    /// This method will panic when called twice.
    pub fn init<F: 'static + Fn(AppEvent, &mut App)>(
        name: &str,
        version: (u16, u16, u16),
        callback: F,
    ) -> Box<App> {
        Self::init_impl(name, version, callback)
    }

    /// Returns the associated [Instance].
    pub fn instance(&self) -> &Arc<Instance> {
        self.instance_impl()
    }

    /// Runs the main event loop.
    ///
    /// This method may or may not return.
    pub fn run(&mut self) {
        self.run_impl();
    }
}

impl App {
    /// Creates a window.
    ///
    /// The returned value *must not be moved out of its box*.
    pub fn create_window<F: 'static + Fn(WindowEvent, &mut Window)>(
        &mut self,
        width: u16,
        height: u16,
        callback: F,
    ) -> Box<Window> {
        self.create_window_impl(width, height, callback)
    }
}

impl Window {
    /// Returns the window’s ICC profile data.
    pub fn icc_profile(&self) -> Option<Vec<u8>> {
        self.icc_profile_impl()
    }

    /// Returns the window position from the bottom left.
    pub fn pos(&self) -> Vector2<u16> {
        self.pos_impl()
    }

    /// Sets the window position.
    pub fn set_pos(&mut self, pos: Vector2<u16>) {
        self.set_pos_impl(pos)
    }

    /// Returns the content size.
    pub fn size(&self) -> Vector2<u16> {
        self.size_impl()
    }

    /// Returns the content size in f32.
    pub fn size_f32(&self) -> Vector2<f32> {
        let size = self.size();
        Vector2 {
            x: size.x as f32,
            y: size.y as f32,
        }
    }

    /// Sets the window size.
    pub fn set_size(&mut self, size: Vector2<u16>) {
        self.set_size_impl(size)
    }

    /// The ratio between physical pixels and layout pixels.
    ///
    /// Usually 2 (high-dpi screens) or 1.
    pub fn physical_pixel_scale(&self) -> f64 {
        self.physical_pixel_scale_impl()
    }

    /// Schedules a call to the callback function after a delay.
    pub fn schedule_callback(&mut self, delay: Duration) {
        self.schedule_callback_impl(delay)
    }

    /// Returns the associated [Surface].
    pub fn surface(&self) -> &Arc<Surface<NarwhalSurface>> {
        self.surface_impl()
    }

    /// Returns the window title.
    pub fn title(&self) -> String {
        self.title_impl()
    }

    /// Sets the window title.
    pub fn set_title(&mut self, title: &str) {
        self.set_title_impl(title)
    }

    /// Sets the title with a represented filename.
    /// Returns false if this is not supported on this platform.
    pub fn set_title_filename(&mut self, filename: &str) -> bool {
        self.set_title_filename_impl(filename)
    }
}
