use crate::event::*;
use cgmath::{Point2, Vector2, Vector3};
use cocoa_ffi::foundation::{NSPoint, NSRect, NSSize};
use std::any::Any;
use std::mem;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use vulkano::instance::loader::FunctionPointers;
use vulkano::instance::{ApplicationInfo, Instance, InstanceExtensions, Version};
use vulkano::statically_linked_vulkan_loader;
use vulkano::swapchain::Surface;

// TODO: support OpenGL maybe

mod sys;

/// Private type for initializing Box<Any>s with *something* to start with because they can’t be
/// empty. Because this is a private type, no downcast call with this inside can be successful
/// outside of this crate.
struct PrivateTypeForInitialUserData;

type AppCallback = Fn(AppEvent, &mut App);
fn null_app_callback(_: AppEvent, _: &mut App) {}

/// The application.
///
/// TODO: make this Pin when Pin is stable
pub struct App {
    app: sys::NSApplication,
    delegate: sys::NCAppDelegate,
    callback: Box<AppCallback>,
    instance: Arc<Instance>,
    event_queue: Vec<AppEvent>,

    /// User data; won’t be touched by anything in this crate.
    pub data: Box<Any>,
}

extern "C" fn app_callback(delegate: sys::NCAppDelegate) {
    let app = unsafe { &mut *(delegate.callback_data().app_ptr as *mut App) };

    let events = app.delegate.drain_events();
    for i in 0..events.len() {
        app.event_queue.push(match events.get(i).event_type() {
            sys::NCAppEventType::Ready => AppEvent::Ready,
            sys::NCAppEventType::Terminating => AppEvent::Terminating,
        });
    }

    let callback = mem::replace(&mut app.callback, Box::new(null_app_callback));
    let event_queue = mem::replace(&mut app.event_queue, Vec::new());

    for event in event_queue {
        callback(event, app);
    }

    mem::replace(&mut app.callback, callback);
}

lazy_static! {
    static ref DID_INIT_APP: Mutex<bool> = Mutex::new(false);
}

impl App {
    pub(crate) fn init_impl<F: 'static + Fn(AppEvent, &mut App)>(
        name: &str,
        version: (u16, u16, u16),
        callback: F,
    ) -> Box<App> {
        {
            let mut did_init = DID_INIT_APP.lock().unwrap();
            if *did_init {
                panic!("Cannot initialize narwhal::platform::App twice");
            }
            *did_init = true;
        }

        debug!(target: "narwhal", "Initializing application “{}” version {:?}", name, version);

        let ns_app = sys::NSApplication::shared();
        let app_delegate = sys::NCAppDelegate::new(app_callback);

        unsafe { ns_app.set_delegate(app_delegate.0) };

        app_delegate.set_dark_appearance();
        app_delegate.set_default_main_menu(name);

        let instance = Instance::with_loader(
            FunctionPointers::new({
                use std::os::raw::c_char;
                use vk_sys as vk;
                use vulkano::instance::loader::Loader;

                Box::new(statically_linked_vulkan_loader!())
            }),
            Some(&ApplicationInfo {
                application_name: Some(name.into()),
                application_version: Some(Version {
                    major: version.0,
                    minor: version.1,
                    patch: version.2,
                }),
                engine_name: None,
                engine_version: None,
            }),
            &InstanceExtensions {
                khr_surface: true,
                mvk_macos_surface: true,
                ..InstanceExtensions::none()
            },
            None,
        )
        .expect("Failed to create Vulkan instance");

        let app = Box::new(App {
            app: ns_app,
            delegate: app_delegate,
            callback: Box::new(callback),
            instance,
            event_queue: Vec::new(),
            data: Box::new(PrivateTypeForInitialUserData),
        });
        let app_ptr = &*app as *const App as *mut ();
        app.delegate
            .set_callback_data(sys::NCAppDelegateCallbackData { app_ptr });
        app
    }

    pub(crate) fn instance_impl(&self) -> &Arc<Instance> {
        &self.instance
    }

    pub(crate) fn run_impl(&mut self) {
        self.app.run();
    }
}

impl App {
    pub(crate) fn create_window_impl<F: 'static + Fn(WindowEvent, &mut Window)>(
        &mut self,
        width: u16,
        height: u16,
        callback: F,
    ) -> Box<Window> {
        let window = sys::NCWindow::new_metal(
            NSRect::new(
                NSPoint::new(0., 0.),
                NSSize::new(width as f64, height as f64),
            ),
            window_callback,
        );
        window.center();

        let layer = unsafe { window.metal_layer() };

        // TODO: don’t panic
        let surface = unsafe {
            Surface::from_macos_moltenvk(Arc::clone(&self.instance), layer, NarwhalSurface)
                .expect("Failed to create window surface")
        };
        let window = Box::new(Window {
            inner: window,
            surface,
            callback_manager: sys::NCCallbackManager::new(),
            callback: Box::new(callback),
            event_queue: Vec::new(),
            data: Box::new(PrivateTypeForInitialUserData),
        });

        let window_ptr = &*window as *const Window as *mut ();
        window.callback_manager.set_owner(window_ptr);
        window
            .inner
            .set_callback_data(sys::NCWindowCallbackData { window_ptr });

        window
    }
}

type WindowCallback = Fn(WindowEvent, &mut Window);
fn null_window_callback(_: WindowEvent, _: &mut Window) {}

/// Narwhal Surface metadata.
///
/// (Unused in the Cocoa backend)
pub struct NarwhalSurface;

/// A window.
///
/// Created with [App::create_window].
///
/// TODO: make this Pin when Pin is stable.
pub struct Window {
    inner: sys::NCWindow,
    surface: Arc<Surface<NarwhalSurface>>,
    callback_manager: sys::NCCallbackManager,
    callback: Box<WindowCallback>,
    event_queue: Vec<WindowEvent>,

    /// User data; won’t be touched by anything in this crate.
    pub data: Box<Any>,
}

extern "C" fn window_callback(window: sys::NCWindow) {
    let window = unsafe { &mut *(window.callback_data().window_ptr as *mut Window) };

    let events = window.inner.drain_events();
    for i in 0..events.len() {
        let event = events.get(i);
        if let Some(event) = match event.event_type() {
            sys::NCWindowEventType::NSEvent => {
                let event = event
                    .event()
                    .expect("NCWindowEventType::NSEvent has no NSEvent data");
                nsevent_to_window_event(event)
            }
            sys::NCWindowEventType::Resized => {
                let rect = window.inner.content_view_frame();
                Some(WindowEvent::Resized(
                    rect.size.width as usize,
                    rect.size.height as usize,
                ))
            }
            sys::NCWindowEventType::BackingUpdate => Some(WindowEvent::OutputChanged),
            sys::NCWindowEventType::WillClose => Some(WindowEvent::Closing),
            sys::NCWindowEventType::Ready => Some(WindowEvent::Ready),
        } {
            window.event_queue.push(event);
        }
    }

    let callback = mem::replace(&mut window.callback, Box::new(null_window_callback));
    let event_queue = mem::replace(&mut window.event_queue, Vec::new());

    for event in event_queue {
        callback(event, window);
    }

    mem::replace(&mut window.callback, callback);
}

fn window_scheduled_callback(window: *mut ()) {
    let window = unsafe { &mut *(window as *mut Window) };
    let callback = mem::replace(&mut window.callback, Box::new(null_window_callback));
    callback(WindowEvent::Scheduled, window);
    mem::replace(&mut window.callback, callback);
}

impl Window {
    pub(crate) fn icc_profile_impl(&self) -> Option<Vec<u8>> {
        // Some(unsafe { self.inner.color_space().icc_profile_data() }.to_vec())
        // FIXME: Apple Color LCD has wrong color transform
        // falling back to sRGB for now
        // (remember to reset layer.colorspace in NCWindow.m)
        None
    }

    pub(crate) fn pos_impl(&self) -> Vector2<u16> {
        let point = self.inner.frame().origin;
        Vector2 {
            x: point.x as u16,
            y: point.y as u16,
        }
    }

    pub(crate) fn set_pos_impl(&mut self, pos: Vector2<u16>) {
        let mut frame = self.inner.frame();
        frame.origin.x = pos.x as f64;
        frame.origin.y = pos.y as f64;
        self.inner.set_frame(frame);
    }

    pub(crate) fn size_impl(&self) -> Vector2<u16> {
        let size = self.inner.content_view_frame().size;
        Vector2 {
            x: size.width as u16,
            y: size.height as u16,
        }
    }

    pub(crate) fn set_size_impl(&mut self, size: Vector2<u16>) {
        self.inner.set_content_size(NSSize {
            width: size.x as f64,
            height: size.y as f64,
        });
    }

    pub(crate) fn physical_pixel_scale_impl(&self) -> f64 {
        self.inner.backing_scale_factor()
    }

    pub(crate) fn schedule_callback_impl(&mut self, delay: Duration) {
        self.callback_manager
            .schedule_callback(delay, Box::new(window_scheduled_callback));
    }

    pub(crate) fn surface_impl(&self) -> &Arc<Surface<NarwhalSurface>> {
        &self.surface
    }

    pub(crate) fn title_impl(&self) -> String {
        self.inner.title()
    }

    pub(crate) fn set_title_impl(&self, title: &str) {
        self.inner.set_title(title)
    }

    pub(crate) fn set_title_filename_impl(&self, filename: &str) -> bool {
        self.inner.set_title_with_represented_filename(filename);
        true
    }
}

fn nsevent_to_window_event(event: sys::NSEvent) -> Option<WindowEvent> {
    let modifier_flags = event.modifier_flags();
    let modifiers = Modifiers {
        cmd: modifier_flags & sys::NSEventModifierFlagCommand != 0,
        ctrl: modifier_flags & sys::NSEventModifierFlagControl != 0,
        shift: modifier_flags & sys::NSEventModifierFlagShift != 0,
        opt: modifier_flags & sys::NSEventModifierFlagOption != 0,
    };

    let event_type = event.event_type();

    match event_type {
        sys::NSEventType::KeyDown | sys::NSEventType::KeyUp => {
            let event_type = match event_type {
                sys::NSEventType::KeyDown => KeyEventType::KeyDown,
                sys::NSEventType::KeyUp => KeyEventType::KeyUp,
                _ => unreachable!(),
            };
            let string = event.characters();
            let raw_string = event.characters_ignoring_modifiers();
            let repeating = event.is_a_repeat();
            let code = event.key_code();

            Some(WindowEvent::UIKeyEvent(KeyEvent {
                event_type,
                modifiers,
                string,
                raw_string: Some(raw_string),
                repeating,
                code: Some(code),
            }))
        }
        sys::NSEventType::SystemDefined
        | sys::NSEventType::AppKitDefined
        | sys::NSEventType::ApplicationDefined
        | sys::NSEventType::Periodic
        | sys::NSEventType::Gesture
        | sys::NSEventType::Magnify
        | sys::NSEventType::Swipe
        | sys::NSEventType::Rotate
        | sys::NSEventType::BeginGesture
        | sys::NSEventType::EndGesture
        | sys::NSEventType::SmartMagnify
        | sys::NSEventType::DirectTouch
        | sys::NSEventType::CursorUpdate => None,
        _ => {
            let mut may_check_subtype = false;
            let mut may_check_device_type = false;
            let mut may_read_pressure = false;

            let event_type = match event_type {
                sys::NSEventType::KeyDown
                | sys::NSEventType::KeyUp
                | sys::NSEventType::SystemDefined
                | sys::NSEventType::AppKitDefined
                | sys::NSEventType::ApplicationDefined
                | sys::NSEventType::Periodic
                | sys::NSEventType::Gesture
                | sys::NSEventType::Magnify
                | sys::NSEventType::Swipe
                | sys::NSEventType::Rotate
                | sys::NSEventType::BeginGesture
                | sys::NSEventType::EndGesture
                | sys::NSEventType::SmartMagnify
                | sys::NSEventType::DirectTouch
                | sys::NSEventType::CursorUpdate => unreachable!(),
                sys::NSEventType::LeftMouseDown
                | sys::NSEventType::RightMouseDown
                | sys::NSEventType::OtherMouseDown => {
                    may_check_subtype = true;
                    may_read_pressure = true;
                    EventType::PointerDown
                }
                sys::NSEventType::LeftMouseDragged
                | sys::NSEventType::RightMouseDragged
                | sys::NSEventType::OtherMouseDragged => {
                    may_check_subtype = true;
                    may_read_pressure = true;
                    EventType::PointerDragged
                }
                sys::NSEventType::LeftMouseUp
                | sys::NSEventType::RightMouseUp
                | sys::NSEventType::OtherMouseUp => {
                    may_check_subtype = true;
                    may_read_pressure = true;
                    EventType::PointerUp
                }
                sys::NSEventType::MouseMoved => {
                    may_check_subtype = true;
                    EventType::PointerMoved
                }
                sys::NSEventType::MouseEntered => {
                    may_check_subtype = true;
                    may_read_pressure = true;
                    EventType::PointerEntered
                }
                sys::NSEventType::MouseExited => {
                    EventType::PointerExited
                }
                sys::NSEventType::FlagsChanged => EventType::ModifiersChanged,
                sys::NSEventType::ScrollWheel => EventType::Scroll,
                sys::NSEventType::TabletPoint => {
                    may_read_pressure = true;
                    EventType::PointerMoved
                }
                sys::NSEventType::TabletProximity => {
                    may_check_device_type = true;
                    EventType::PointerMoved
                }
                sys::NSEventType::QuickLook => EventType::QuickLook,
                sys::NSEventType::Pressure => {
                    may_read_pressure = true;
                    EventType::PressureChanged
                }
            };

            if may_check_subtype {
                let subtype = event.subtype();
                match subtype {
                    sys::NSEventSubtypeTabletProximity => may_check_device_type = true,
                    _ => (),
                }
            }

            let button = match event.button_number() {
                0 => None,
                1 => Some(Button::Primary),
                2 => Some(Button::Secondary),
                3 => Some(Button::Middle),
                x => Some(Button::Other(x as usize)),
            };

            let device = if may_check_device_type {
                match event.pointing_device_type() {
                    sys::NSPointingDeviceType::Unknown => None,
                    sys::NSPointingDeviceType::Cursor => Some(PointingDevice::Cursor),
                    sys::NSPointingDeviceType::Pen => Some(PointingDevice::Pen),
                    sys::NSPointingDeviceType::Eraser => Some(PointingDevice::Eraser),
                }
            } else {
                None
            };

            let pressure = if may_read_pressure {
                Some(event.pressure() as f64)
            } else {
                None
            };

            // TODO: more tablet data

            let vector = Vector3::new(event.delta_x(), event.delta_y(), event.delta_z());
            let scale = event.magnification();
            let ns_point = event.location_in_window();
            let point = Point2::new(ns_point.x, ns_point.y);

            Some(WindowEvent::UIEvent(Event {
                event_type,
                modifiers,
                point,
                button,
                device,
                pressure,
                vector: Some(vector),
                scale: Some(scale),
            }))
        }
    }
}
