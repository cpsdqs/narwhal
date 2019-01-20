use self::input_handler;
use crate::event::{AppEvent, WindowEvent};
use cgmath::Vector2;
use smithay_client_toolkit::{Environment, Shell};
use std::any::Any;
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use std::{mem, thread};
use vulkano::instance::{ApplicationInfo, Instance, InstanceExtensions, Version};
use vulkano::swapchain::Surface;
use wayland_client::protocol::wl_compositor::RequestsTrait as CompositorReq;
use wayland_client::protocol::wl_surface::{
    Event as SurfaceEvent, RequestsTrait as SurfaceReq, WlSurface,
};
use wayland_client::{Display, EventQueue, GlobalEvent, Proxy};
use wayland_protocols::xdg_shell::client::xdg_surface::{
    Event as XdgSurfaceEvent, RequestsTrait as XdgSurfaceReq, XdgSurface,
};
use wayland_protocols::xdg_shell::client::xdg_toplevel::{
    Event as XdgToplevelEvent, RequestsTrait as XdgToplevelReq, XdgToplevel,
};
use wayland_protocols::xdg_shell::client::xdg_wm_base::RequestsTrait as XdgWmBaseReq;

mod input_handler;

lazy_static! {
    static ref DID_INIT_APP: Mutex<bool> = Mutex::new(false);
}

/// Private type for initializing Box<Any>s with *something* to start with because they can’t be
/// empty. Because this is a private type, no downcast call with this inside can be successful
/// outside of this crate.
struct PrivateTypeForInitialUserData;

type AppCallback = Fn(AppEvent, &mut App);
fn null_app_callback(_: AppEvent, _: &mut App) {}

type SurfaceID = u32;

#[derive(Debug)]
struct WindowUpdate {
    id: SurfaceID,
    update: Update,
}

#[derive(Debug)]
enum Update {
    Event(WindowEvent),
    Resize(i32, i32),
}

/// The application.
pub struct App {
    display: Display,
    environment: Environment,
    wl_queue: EventQueue,
    app_name: String,
    callback: Box<AppCallback>,
    instance: Arc<Instance>,
    event_queue: Vec<AppEvent>,
    windows: HashMap<SurfaceID, (Weak<Mutex<WindowInner>>, *mut Window)>,
    update_recv: mpsc::Receiver<WindowUpdate>,
    update_send: mpsc::Sender<WindowUpdate>,
    callback_recv: mpsc::Receiver<(SurfaceID, Instant)>,
    callback_send: mpsc::Sender<(SurfaceID, Instant)>,
    callbacks: Vec<(SurfaceID, Instant)>,

    /// User data; won’t be touched by anything in this crate.
    pub data: Box<Any>,
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

        let (display, mut event_queue) =
            Display::connect_to_env().expect("Failed to connect to Wayland server");

        let (update_send, update_recv) = mpsc::channel();

        let mut input_handler = input_handler::InputHandler::new(update_send.clone());

        let environment = Environment::from_display_with_cb(
            &display,
            &mut event_queue,
            move |event, registry| match event {
                GlobalEvent::New {
                    id,
                    interface,
                    version,
                } => {
                    println!("new global: {} v{}", interface, version);
                    match &*interface {
                        "wl_seat" => input_handler.add_seat(id, version, &registry),
                        "zwp_tablet_manager_v2" => {
                            input_handler.add_tablet_manager(id, version, &registry)
                        }
                        _ => (),
                    }
                }
                GlobalEvent::Removed { id, interface } => {
                    match &*interface {
                        "wl_seat" => input_handler.remove_seat(id),
                        "zwp_tablet_manager_v2" => input_handler.remove_tablet_manager(id),
                        _ => (),
                    }
                    println!("global removed: {}", interface);
                }
            },
        )
        .unwrap();

        let instance = Instance::new(
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
                khr_wayland_surface: true,
                ..InstanceExtensions::none()
            },
            None,
        )
        .expect("Failed to create Vulkan instance");

        let (callback_send, callback_recv) = mpsc::channel();

        Box::new(App {
            display,
            environment,
            wl_queue: event_queue,
            app_name: name.into(),
            callback: Box::new(callback),
            instance,
            event_queue: Vec::new(),
            windows: HashMap::new(),
            update_recv,
            update_send,
            callback_recv,
            callback_send,
            callbacks: Vec::new(),
            data: Box::new(PrivateTypeForInitialUserData),
        })
    }

    pub(crate) fn instance_impl(&self) -> &Arc<Instance> {
        &self.instance
    }

    fn dispatch_callback(&mut self) {
        let callback = mem::replace(&mut self.callback, Box::new(null_app_callback));
        let event_queue = mem::replace(&mut self.event_queue, unsafe { mem::uninitialized() });

        for event in event_queue {
            callback(event, self);
        }

        mem::replace(&mut self.callback, callback);
        // must not drop uninitialized
        mem::forget(mem::replace(&mut self.event_queue, Vec::new()));
    }

    pub(crate) fn run_impl(&mut self) {
        self.display.flush().expect("Failed to flush events");

        self.event_queue.push(AppEvent::Ready);
        self.dispatch_callback();

        loop {
            loop {
                let (window_id, time) = match self.callback_recv.try_recv() {
                    Ok(v) => v,
                    Err(_) => break,
                };

                match self.callbacks.binary_search_by_key(&time, |k| k.1) {
                    Ok(i) | Err(i) => {
                        if i >= self.callbacks.len() {
                            self.callbacks.push((window_id, time));
                        } else {
                            self.callbacks.insert(i, (window_id, time))
                        }
                    }
                };
            }

            if let Some((_, next_callback)) = self.callbacks.get(0) {
                self.wl_queue
                    .dispatch_pending()
                    .expect("Failed to dispatch event queue");

                let now = Instant::now();
                let mut wait_duration = if *next_callback < now {
                    Duration::new(0, 0)
                } else {
                    *next_callback - Instant::now()
                };
                // TODO: dispatch_timeout somehow
                // HACK: cap wait duration at one second
                if wait_duration.as_secs() >= 1 {
                    wait_duration = Duration::new(1, 0);
                }
                thread::sleep(wait_duration);
            } else {
                // nothing scheduled
                self.wl_queue
                    .dispatch()
                    .expect("Failed to dispatch event queue");
            }

            let mut callbacks_to_remove = Vec::new();
            let now = Instant::now();

            for ((window_id, time), index) in self.callbacks.iter().zip(0..) {
                if time <= &now {
                    self.update_send
                        .send(WindowUpdate {
                            id: *window_id,
                            update: Update::Event(WindowEvent::Scheduled),
                        })
                        .unwrap();
                    callbacks_to_remove.push(index);
                } else {
                    break;
                }
            }

            let mut offset = 0;
            for i in callbacks_to_remove {
                self.callbacks.remove(i - offset);
                offset += 1;
            }

            loop {
                let WindowUpdate { id, update } = match self.update_recv.try_recv() {
                    Ok(v) => v,
                    Err(_) => break,
                };

                if let Some((window_inner, window_ptr)) = self
                    .windows
                    .get(&id)
                    .map_or(None, |(weak, ptr)| Weak::upgrade(&weak).map(|x| (x, ptr)))
                {
                    {
                        let mut window_inner = window_inner.lock().unwrap();
                        match update {
                            Update::Event(mut event) => {
                                // must invert Y
                                // TODO: move this elsewhere
                                match event {
                                    WindowEvent::UIEvent(ref mut event) => {
                                        event.point.y = window_inner.size.1 as f64 - event.point.y;
                                    }
                                    _ => (),
                                }
                                window_inner.event_queue.push(event);
                            }
                            Update::Resize(w, h) => {
                                window_inner.xdg_surface.set_window_geometry(0, 0, w, h);
                                window_inner.size = (w as u16, h as u16);
                                // TODO: get resolution
                                *window_inner.vk_surface.window().new_size.lock().unwrap() =
                                    Some(((w as u16, h as u16).into(), 2.));
                            }
                        };
                    }

                    WindowInner::dispatch_callback(&window_inner, *window_ptr);
                }
            }
        }
    }

    pub(crate) fn create_window_impl<F: 'static + Fn(WindowEvent, &mut Window)>(
        &mut self,
        width: u16,
        height: u16,
        callback: F,
    ) -> Box<Window> {
        let surface = self
            .environment
            .compositor
            .create_surface(|surface| {
                surface.implement(
                    |event, surface| match event {
                        SurfaceEvent::Enter { output } => {
                            println!("TODO: surface entered");
                        }
                        SurfaceEvent::Leave { output } => {
                            println!("TODO: surface left");
                        }
                    },
                    (),
                )
            })
            .unwrap();

        let update_sender = self.update_send.clone();
        let window_id = surface.id();

        let shell = match self.environment.shell {
            Shell::Xdg(ref shell) => shell,
            _ => panic!("Unsupported shell"),
        };

        let xdg_surf = shell
            .get_xdg_surface(&surface, |surface| {
                surface.implement(
                    |event, surface| match event {
                        XdgSurfaceEvent::Configure { serial } => surface.ack_configure(serial),
                    },
                    (),
                )
            })
            .unwrap();

        let toplevel = xdg_surf
            .get_toplevel(move |toplevel| {
                toplevel.implement(
                    move |event, _| match event {
                        XdgToplevelEvent::Configure {
                            width,
                            height,
                            states: _,
                        } => {
                            update_sender
                                .send(WindowUpdate {
                                    id: window_id,
                                    update: Update::Resize(width, height),
                                })
                                .unwrap();
                        }
                        XdgToplevelEvent::Close => {
                            update_sender
                                .send(WindowUpdate {
                                    id: window_id,
                                    update: Update::Event(WindowEvent::Closing),
                                })
                                .unwrap();
                        }
                    },
                    (),
                )
            })
            .unwrap();

        toplevel.set_app_id(self.app_name.clone());
        xdg_surf.set_window_geometry(0, 0, width as i32, height as i32);

        let vk_surface = unsafe {
            Surface::from_wayland(
                Arc::clone(&self.instance),
                self.display.c_ptr(),
                surface.c_ptr(),
                NarwhalSurface {
                    // TODO: get resolution
                    new_size: Mutex::new(Some(((width, height).into(), 2.))),
                },
            )
        }
        .expect("Failed to create Vulkan surface");

        // TODO: get DPI
        surface.set_buffer_scale(2);

        // fixes window being weirdly stuck in the corner
        surface.commit();

        let window_inner = Arc::new(Mutex::new(WindowInner {
            toplevel,
            vk_surface: vk_surface.clone(),
            xdg_surface: xdg_surf,
            wl_surface: surface,
            event_queue: Vec::new(),
            size: (width, height),
        }));

        let window_inner_ref = Arc::downgrade(&window_inner);

        let window = Box::new(Window {
            id: window_id,
            title: "".into(),
            inner: window_inner,
            surface: vk_surface,
            callback: Box::new(callback),
            callback_send: self.callback_send.clone(),
            data: Box::new(PrivateTypeForInitialUserData),
        });

        let window_ptr = &*window as *const Window as *mut Window;
        self.windows
            .insert(window_id, (window_inner_ref, window_ptr));
        self.update_send
            .send(WindowUpdate {
                id: window_id,
                update: Update::Event(WindowEvent::Ready),
            })
            .unwrap();

        window
    }
}

type WindowCallback = Fn(WindowEvent, &mut Window);
fn null_window_callback(_: WindowEvent, _: &mut Window) {}

struct WindowInner {
    toplevel: Proxy<XdgToplevel>,
    vk_surface: Arc<Surface<NarwhalSurface>>,
    xdg_surface: Proxy<XdgSurface>,
    wl_surface: Proxy<WlSurface>,
    event_queue: Vec<WindowEvent>,
    size: (u16, u16),
}

impl WindowInner {
    fn dispatch_callback(inner: &Mutex<WindowInner>, owner_ref: *mut Window) {
        let (win, event_queue) = {
            let mut inner = inner.lock().unwrap();
            let win = unsafe { &mut *owner_ref };
            let event_queue = mem::replace(&mut inner.event_queue, Vec::new());
            (win, event_queue)
        };
        let callback = mem::replace(&mut win.callback, Box::new(null_window_callback));

        // inner must not be borrowed here so that callback can use the window API

        for event in event_queue {
            callback(event, win);
        }

        mem::replace(&mut win.callback, callback);
    }
}

/// Narwhal Surface metadata.
pub struct NarwhalSurface {
    /// Wayland-specific: new window size and resolution for the presenter.
    pub new_size: Mutex<Option<(Vector2<u16>, f32)>>,
}

/// A window.
///
/// Created with [App::create_window].
///
/// TODO: make this Pin when Pin is stable
pub struct Window {
    id: SurfaceID,
    surface: Arc<Surface<NarwhalSurface>>,
    title: String,
    callback: Box<WindowCallback>,
    callback_send: mpsc::Sender<(SurfaceID, Instant)>,
    inner: Arc<Mutex<WindowInner>>,

    /// User data; won’t be touched by anything in this crate.
    pub data: Box<Any>,
}

impl Window {
    pub(crate) fn icc_profile_impl(&self) -> Option<Vec<u8>> {
        println!("TODO: ICC profile stuff");
        None
    }

    pub(crate) fn pos_impl(&self) -> Vector2<u16> {
        // ??
        (0, 0).into()
    }

    pub(crate) fn set_pos_impl(&mut self, _: Vector2<u16>) {
        // ??
    }

    pub(crate) fn size_impl(&self) -> Vector2<u16> {
        self.inner.lock().unwrap().size.into()
    }

    pub(crate) fn set_size_impl(&mut self, size: Vector2<u16>) {
        let mut inner = self.inner.lock().unwrap();
        inner
            .xdg_surface
            .set_window_geometry(0, 0, size.x as i32, size.y as i32);
        inner.size = size.into();
        // TODO: get resolution
        *self.surface.window().new_size.lock().unwrap() = Some((size, 2.));
    }

    pub(crate) fn physical_pixel_scale_impl(&self) -> f64 {
        println!("TODO: get DPI");
        2.
    }

    pub(crate) fn schedule_callback_impl(&mut self, delay: Duration) {
        self.callback_send
            .send((self.id, Instant::now() + delay))
            .unwrap();
    }

    pub(crate) fn surface_impl(&self) -> &Arc<Surface<NarwhalSurface>> {
        &self.surface
    }

    pub(crate) fn title_impl(&self) -> String {
        self.title.clone()
    }

    pub(crate) fn set_title_impl(&mut self, title: &str) {
        self.inner.lock().unwrap().toplevel.set_title(title.into());
        self.title = title.into();
    }

    pub(crate) fn set_title_filename_impl(&mut self, filename: &str) -> bool {
        false
    }
}
