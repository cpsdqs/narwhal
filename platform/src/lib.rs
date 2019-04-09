//! Window creation and application lifecycle stuff for Narwhal.
//!
//! # Event Loop
//! Important to note is that the event loop is managed by the native API such that most app
//! lifecycle handling comes for free, and so all application logic must be dispatched from a
//! callback. See [App] and [Window] for more details.
//!
//! # Additional Notes
//! - The origin of the coordinate system for windows is at the bottom left of the primary screen,
//!   with positive X going right and positive Y going upwards

#[macro_use]
extern crate log;

use crate::event::{AppEvent, WindowEvent};
use cgmath::Vector2;
use std::any::Any;
use std::marker::PhantomData;
use std::ops::DerefMut;
use std::sync::Arc;
use vulkano::instance::Instance;
use vulkano::swapchain::Surface;

#[cfg(target_os = "macos")]
mod cocoa;
#[cfg(target_os = "macos")]
pub use self::cocoa::*;

#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "linux")]
pub use self::wayland::*;

pub mod event;

type PhantomNotSend = PhantomData<*const ()>;
pub(crate) type AppCallback = FnMut(&mut App);
pub(crate) type WindowCallback = FnMut(&mut Window);

// NOTE: do not change the memory layout of these following types (see cocoa app callback handling)

/// The application singleton.
///
/// The callback function will be called immediately after an event occurs.
#[repr(C)]
pub struct App(InnerApp, PhantomNotSend);

/// A window.
///
/// Created with [App::create_window].
///
/// The callback function will be called synchronously with every frame, or immediately when special
/// events occur (such as resizing). Note that this may entail the callback being called from
/// **different threads** (despite the fact that Window is !Send + !Sync).
/// When using a narwhal `Presenter`, it should be ensured that the `wait` calls are not
/// overlapping.
#[repr(C)]
pub struct Window(InnerWindow, PhantomNotSend);

impl App {
    /// Initializes the application instance—must be called only once.
    ///
    /// The name and the version will be used for Vulkan’s application info struct, among other
    /// things.
    ///
    /// # Panics
    /// This method will panic when called twice.
    pub fn init<F>(name: &str, version: (u16, u16, u16), callback: F) -> App
    where
        F: FnMut(&mut App) + 'static,
    {
        App(init_app(name, version, Box::new(callback)), PhantomData)
    }

    /// Iterates over enqueued events, in order.
    pub fn events(&mut self) -> impl Iterator<Item = AppEvent> + '_ {
        self.0.events()
    }

    /// Returns the associated [Instance].
    pub fn instance(&self) -> &Arc<Instance> {
        self.0.instance()
    }

    /// Starts the main event loop.
    pub fn run(&mut self) -> ! {
        self.0.run()
    }

    /// Creates a window with a callback.
    ///
    /// The width and the height are the *suggested* content size. It may happen that the window
    /// ends up having different dimensions.
    pub fn create_window<F>(&mut self, width: u16, height: u16, callback: F) -> Window
    where
        F: FnMut(&mut Window) + Send + Sync + 'static,
    {
        Window(
            self.0.create_window(width, height, Box::new(callback)),
            PhantomData,
        )
    }

    /// Returns a reference to the user data.
    pub fn data(&self) -> &Box<dyn Any> {
        &self.0.data
    }

    /// Returns a mutable reference to the user data.
    pub fn data_mut(&mut self) -> &mut Box<dyn Any> {
        &mut self.0.data
    }
}

impl Window {
    /// Iterates over enqueued events, in order.
    pub fn events(&mut self) -> impl Iterator<Item = WindowEvent> + '_ {
        self.0.events()
    }

    /// Requests for the callback to be called during the next frame.
    ///
    /// Note that this does not necessarily mean that the callback *won’t* be called if this
    /// method isn’t called.
    pub fn request_frame(&mut self) {
        self.0.request_frame();
    }

    /// Returns the associated [Surface].
    pub fn surface(&self) -> &Arc<Surface<NarwhalSurface>> {
        self.0.surface()
    }

    /// Returns a mutable reference to the user data.
    pub fn data(&mut self) -> impl DerefMut<Target = Box<dyn Any + Send>> {
        self.0.data()
    }

    /// Returns the window’s ICC profile data.
    pub fn icc_profile(&self) -> Option<Vec<u8>> {
        self.0.icc_profile()
    }

    /// Returns the window position from the bottom left.
    pub fn pos(&self) -> Vector2<u16> {
        self.0.pos()
    }

    /// Sets the window position.
    pub fn set_pos(&mut self, pos: Vector2<u16>) {
        self.0.set_pos(pos)
    }

    /// Returns the content size.
    pub fn size(&self) -> Vector2<u16> {
        self.0.size()
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
        self.0.set_size(size)
    }

    /// The ratio between physical pixels and layout pixels.
    ///
    /// Usually 2 (most high-dpi screens) or 1.
    pub fn backing_scale_factor(&self) -> f64 {
        self.0.backing_scale_factor()
    }

    /// Returns the window title.
    pub fn title(&self) -> String {
        self.0.title()
    }

    /// Sets the window title.
    pub fn set_title(&mut self, title: &str) {
        self.0.set_title(title)
    }
}
