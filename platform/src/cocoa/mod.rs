use crate::event::*;
use crate::{App, AppCallback, Window, WindowCallback};
use cgmath::{Point2, Vector2, Vector3};
use cocoa::foundation::{NSPoint, NSRect, NSSize};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::any::Any;
use std::collections::VecDeque;
use std::mem;
use std::ops::DerefMut;
use std::pin::Pin;
use std::sync::Arc;
use vulkano::instance::loader::FunctionPointers;
use vulkano::instance::{ApplicationInfo, Instance, InstanceExtensions, Version};
use vulkano::statically_linked_vulkan_loader;
use vulkano::swapchain::Surface;

mod sys;

/// Private type for initializing Box<Any>s with *something* to start with because they can’t be
/// empty. Because this is a private type, no downcast call with this inside can be successful
/// outside of this crate.
struct PrivateTypeForInitialUserData;

pub(crate) struct CocoaApp {
    app: sys::NSApplication,
    delegate: sys::NCAppDelegate,
    instance: Arc<Instance>,
    event_queue: VecDeque<AppEvent>,
    callback: Box<AppCallback>,

    /// User data; won’t be touched by anything in this crate.
    pub data: Box<Any>,
}

pub(crate) type InnerApp = Pin<Box<CocoaApp>>;

extern "C" fn app_callback(delegate: sys::NCAppDelegate) {
    let app = unsafe { &mut *(delegate.callback_data().app_ptr as *mut CocoaApp) };
    app.dequeue_events();
    app.call_user_callback();
}

lazy_static! {
    static ref DID_INIT_APP: Mutex<bool> = Mutex::new(false);
}

pub(crate) fn init_app(
    name: &str,
    version: (u16, u16, u16),
    callback: Box<AppCallback>,
) -> Pin<Box<CocoaApp>> {
    {
        let mut did_init = DID_INIT_APP.lock();
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

    let app = Pin::new(Box::new(CocoaApp {
        app: ns_app,
        delegate: app_delegate,
        instance,
        callback,
        event_queue: VecDeque::new(),
        data: Box::new(PrivateTypeForInitialUserData),
    }));
    let app_ptr = &*app as *const CocoaApp as *mut ();
    app.delegate
        .set_callback_data(sys::NCAppDelegateCallbackData { app_ptr });
    unsafe { app.app.finish_launching_and_activate() };
    app
}

impl CocoaApp {
    pub(crate) fn instance(&self) -> &Arc<Instance> {
        &self.instance
    }

    pub(crate) fn run(&mut self) -> ! {
        unsafe { self.app.run() };
        loop {
            let mut is_first = true;

            let _pool = sys::NSAutoreleasePool::new();

            loop {
                let wait_duration = if is_first {
                    is_first = false;
                    // block waiting for the next event
                    sys::NSDate::distant_future()
                } else {
                    // then dequeue the rest
                    sys::NSDate::distant_past()
                };

                let event = unsafe {
                    self.app.next_event(
                        sys::NSEventMask::NSAnyEventMask,
                        wait_duration,
                        sys::NSRunLoopMode::Default,
                        true,
                    )
                };

                if let Some(event) = event {
                    match event.event_type() {
                        sys::NSEventType::ApplicationDefined => (),
                        _ => unsafe { self.app.send_event(event) },
                    }
                } else {
                    break;
                }
            }
        }
    }

    fn dequeue_events(&mut self) {
        while let Some(event) = self.delegate.dequeue_event() {
            self.event_queue.push_back(match event.event_type() {
                sys::NCAppEventType::Ready => AppEvent::Ready,
                sys::NCAppEventType::Terminating => AppEvent::Terminating,
            });
        }
    }

    fn call_user_callback(&mut self) {
        // the following is PROBABLY EXTREMELY UNSAFE operating under the assumption that
        // App(InnerApp, PhantomData) is exactly the same as InnerApp (memory-wise)
        // hence, &mut App == &mut InnerApp == &mut Pin<Box<CocoaApp>>
        let mut tmp_pin = Pin::new(unsafe { Box::from_raw(self) });

        {
            let tmp_app = unsafe { mem::transmute::<&mut InnerApp, &mut App>(&mut tmp_pin) };
            (self.callback)(tmp_app);
        }

        mem::forget(tmp_pin); // don’t want to drop the app
    }

    pub(crate) fn events(&mut self) -> impl Iterator<Item = AppEvent> + '_ {
        struct Drain<'a>(&'a mut CocoaApp);
        impl<'a> Iterator for Drain<'a> {
            type Item = AppEvent;
            fn next(&mut self) -> Option<AppEvent> {
                self.0.event_queue.pop_front()
            }
        }
        Drain(self)
    }

    pub(crate) fn create_window(
        &mut self,
        width: u16,
        height: u16,
        callback: Box<WindowCallback>,
    ) -> Pin<Box<CocoaWindow>> {
        let window = sys::NCWindow::new(
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
            Surface::from_macos_moltenvk(Arc::clone(&self.instance), layer, NarwhalSurface(()))
                .expect("Failed to create window surface")
        };
        let window = Pin::new(Box::new(CocoaWindow {
            inner: window,
            surface,
            callback,
            event_queue: VecDeque::new(),
            data: Mutex::new(Box::new(PrivateTypeForInitialUserData)),
        }));

        let window_ptr = &*window as *const CocoaWindow as *mut ();
        window
            .inner
            .set_callback_data(sys::NCWindowCallbackData { window_ptr });

        window
    }
}

/// Narwhal Surface metadata.
///
/// (Unused in the Cocoa backend)
pub struct NarwhalSurface(());

pub(crate) struct CocoaWindow {
    inner: sys::NCWindow,
    surface: Arc<Surface<NarwhalSurface>>,
    event_queue: VecDeque<WindowEvent>,
    callback: Box<WindowCallback>,

    /// User data; won’t be touched by anything in this crate.
    pub data: Mutex<Box<Any + Send>>,
}

pub(crate) type InnerWindow = Pin<Box<CocoaWindow>>;

extern "C" fn window_callback(
    window: sys::NCWindow,
    is_main_thread: sys::BOOL,
    should_render: sys::BOOL,
) {
    let window = unsafe { &mut *(window.callback_data().window_ptr as *mut CocoaWindow) };

    if is_main_thread == sys::YES {
        while let Some(event) = window.inner.dequeue_event() {
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
                window.event_queue.push_back(event);
            }
        }
    }

    if should_render == sys::YES {
        window.call_user_callback();
    }
}

impl CocoaWindow {
    pub(crate) fn events(&mut self) -> impl Iterator<Item = WindowEvent> + '_ {
        struct Drain<'a>(&'a mut CocoaWindow);
        impl<'a> Iterator for Drain<'a> {
            type Item = WindowEvent;
            fn next(&mut self) -> Option<WindowEvent> {
                self.0.event_queue.pop_front()
            }
        }
        Drain(self)
    }

    pub(crate) fn data(&mut self) -> impl DerefMut<Target = Box<dyn Any + Send>> {
        self.data.lock()
    }

    pub(crate) fn request_frame(&mut self) {
        self.inner.request_frame();
    }

    fn call_user_callback(&mut self) {
        // see call_user_callback in CocoaApp for more details
        let mut tmp_pin = Pin::new(unsafe { Box::from_raw(self) });

        {
            let tmp_win = unsafe { mem::transmute::<&mut InnerWindow, &mut Window>(&mut tmp_pin) };
            (self.callback)(tmp_win);
        }

        mem::forget(tmp_pin);
    }

    pub(crate) fn icc_profile(&self) -> Option<Vec<u8>> {
        Some(unsafe { self.inner.layer_color_space().icc_profile_data() }.to_vec())
    }

    pub(crate) fn pos(&self) -> Vector2<u16> {
        let point = self.inner.frame().origin;
        Vector2 {
            x: point.x as u16,
            y: point.y as u16,
        }
    }

    pub(crate) fn set_pos(&mut self, pos: Vector2<u16>) {
        let mut frame = self.inner.frame();
        frame.origin.x = pos.x as f64;
        frame.origin.y = pos.y as f64;
        self.inner.set_frame(frame);
    }

    pub(crate) fn size(&self) -> Vector2<u16> {
        let size = self.inner.content_view_frame().size;
        Vector2 {
            x: size.width as u16,
            y: size.height as u16,
        }
    }

    pub(crate) fn set_size(&mut self, size: Vector2<u16>) {
        self.inner.set_content_size(NSSize {
            width: size.x as f64,
            height: size.y as f64,
        });
    }

    pub(crate) fn backing_scale_factor(&self) -> f64 {
        self.inner.backing_scale_factor()
    }

    pub(crate) fn surface(&self) -> &Arc<Surface<NarwhalSurface>> {
        &self.surface
    }

    pub(crate) fn title(&self) -> String {
        self.inner.title()
    }

    pub(crate) fn set_title(&self, title: &str) {
        self.inner.set_title(title)
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
                sys::NSEventType::MouseEntered => EventType::PointerEntered,
                sys::NSEventType::MouseExited => EventType::PointerExited,
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
