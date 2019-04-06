extern crate cgmath;
extern crate lcms_prime;
extern crate narwhal;
extern crate vulkano;

use cgmath::{InnerSpace, Matrix4, Vector2};
use lcms_prime::Profile;
use narwhal::data::cgmath_ext::Vector2Ext;
use narwhal::data::*;
use narwhal::node::*;
use narwhal::platform::event::*;
use narwhal::platform::*;
use narwhal::render::*;
use std::io;
use std::sync::{Arc, Mutex};
use vulkano::device::{Device, Queue};
use vulkano::instance::PhysicalDevice;
use vulkano::sync::GpuFuture;

struct AppData {
    windows: Vec<Window>,
    phys_dev: usize,
    device: Arc<Device>,
    queue: Arc<Queue>,
}

struct WinData {
    renderer: Renderer,
    presenter: Mutex<Presenter>,
    composite: Option<NodeRef>,
    stroke_len: f64,
    prev_point: Option<Vector2<f64>>,
}

fn main() {
    let mut app = App::init("Narwhal Drawing Test", (0, 1, 0), |app| {
        for event in app.events().collect::<Vec<_>>() {
            handle_app_event(app, event);
        }
    });

    let (pd, device, queue) = Presenter::choose_device(app.instance()).expect("No device");
    *app.data_mut() = Box::new(AppData {
        windows: Vec::new(),
        phys_dev: pd,
        device,
        queue,
    });

    app.run();
}

fn handle_app_event(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::Ready => {
            let mut win = app.create_window(400, 400, handle_window_events);

            let data: &mut AppData = app.data_mut().downcast_mut().expect("Invalid app data");

            let renderer = Renderer::new(
                Graph::new(),
                Arc::clone(&data.device),
                Arc::clone(&data.queue),
            )
            .unwrap();
            let mut presenter = Presenter::new(
                &PhysicalDevice::from_index(data.device.instance(), data.phys_dev).unwrap(),
                Arc::clone(&win.surface()),
                Arc::clone(&data.device),
                Arc::clone(&data.queue),
            )
            .unwrap();

            if let Some(profile) = win.icc_profile() {
                let profile =
                    Profile::deser(&mut io::Cursor::new(profile)).expect("Failed to deser profile");
                presenter
                    .set_profile(profile)
                    .expect("Failed to set profile");
            }

            *win.data_mut() = Box::new(WinData {
                renderer,
                presenter: Mutex::new(presenter),
                composite: None,
                stroke_len: 0.,
                prev_point: None,
            });

            data.windows.push(win);
        }
        AppEvent::Terminating => (),
    }
}

fn handle_window_events(win: &mut Window) {
    for event in win.events().collect::<Vec<_>>() {
        let win_size = win.size_f32();
        let win_resolution = win.backing_scale_factor() as f32;
        let mut schedule_cb = false;
        {
            let data: &mut WinData = win.data_mut().downcast_mut().expect("Invalid window data");

            match event {
                WindowEvent::Ready => {
                    data.renderer.set_resolution(win_resolution);

                    data.renderer.add_node_type(defs::COMPOSITE).unwrap();
                    data.renderer.add_node_type(defs::CAMERA).unwrap();

                    let mut cam = Node::empty(defs::CAMERA_NAME.into());
                    cam.set(defs::CameraProps::Size.into(), win_size.into_f64());
                    cam.set(defs::CameraProps::Offset.into(), Vector2::new(0., 0.));
                    cam.set(
                        defs::CameraProps::Transform.into(),
                        Matrix4::from_translation((0., 0., 200.).into()),
                    );
                    cam.set(defs::CameraProps::Fov.into(), 1.57079632);
                    cam.set(defs::CameraProps::ClipNear.into(), 0.01);
                    cam.set(defs::CameraProps::ClipFar.into(), 100.);

                    let graph = data.renderer.graph_mut();

                    let cam = graph.add_node(cam);
                    graph.set_output(cam);

                    let mut composite = Node::empty(defs::COMPOSITE_NAME.into());
                    composite.set(defs::CompositeProps::In.into(), Vec::<Drawable>::new());
                    let composite = graph.add_node(composite);

                    graph.link(composite, 1, cam, 0);
                    data.composite = Some(composite);

                    schedule_cb = true;
                }
                WindowEvent::Resized(..) | WindowEvent::OutputChanged => {
                    data.renderer.set_resolution(win_resolution);

                    let graph = data.renderer.graph_mut();
                    let cam_id = graph.output();
                    let cam = graph.node_mut(&cam_id).unwrap();
                    cam.set(defs::CameraProps::Size.into(), win_size.into_f64());
                    cam.set(
                        defs::CameraProps::Transform.into(),
                        Matrix4::from_translation((0., 0., win_size.y as f64 / 2.).into()),
                    );
                    schedule_cb = true;
                }
                WindowEvent::UIEvent(event) => match event.event_type {
                    EventType::PointerDown => {
                        let x = event.point.x - win_size.x as f64 / 2.;
                        let y = event.point.y - win_size.y as f64 / 2.;
                        let pressure = event.pressure.unwrap_or(1.);

                        data.stroke_len = 0.;
                        data.prev_point = Some(Vector2::new(x, y));

                        let graph = data.renderer.graph_mut();
                        let composite_ref = data.composite.unwrap();

                        let mut path = Path2D::new();
                        path.commands_mut().push(Path2DCmd::LineTo((x, y).into()));
                        let mut weight = StrokeWeight::new();
                        weight
                            .commands_mut()
                            .push(WeightCmd::LineTo((0., pressure, 0.).into()));

                        match graph
                            .node_mut(&composite_ref)
                            .unwrap()
                            .get_mut(defs::CompositeProps::In.into())
                            .unwrap()
                        {
                            Value::Drawables(drawables) => drawables.push(Drawable {
                                id: (composite_ref, drawables.len() as u64),
                                shape: Shape {
                                    path,
                                    fill: None,
                                    stroke: Some((weight, 10., Color::WHITE)),
                                    transform: None,
                                },
                            }),
                            _ => panic!("oh no"),
                        }
                    }
                    EventType::PointerDragged => {
                        let x = event.point.x - win_size.x as f64 / 2.;
                        let y = event.point.y - win_size.y as f64 / 2.;
                        let pressure = event.pressure.unwrap_or(1.);

                        let prev_len = data.stroke_len;
                        let delta_len = (Vector2::new(x, y) - data.prev_point.unwrap()).magnitude();
                        data.prev_point = Some(Vector2::new(x, y));
                        data.stroke_len += delta_len;
                        let new_len = prev_len + delta_len;

                        let graph = data.renderer.graph_mut();
                        let composite_ref = data.composite.unwrap();
                        match graph
                            .node_mut(&composite_ref)
                            .unwrap()
                            .get_mut(defs::CompositeProps::In.into())
                            .unwrap()
                        {
                            Value::Drawables(drawables) => {
                                let stroke = drawables.last_mut().unwrap();
                                stroke
                                    .shape
                                    .path
                                    .commands_mut()
                                    .push(Path2DCmd::LineTo((x, y).into()));
                                let (weight, ..) = stroke.shape.stroke.as_mut().unwrap();
                                let cmds = weight.commands_mut();
                                for cmd in cmds.iter_mut() {
                                    cmd.remap_points(&mut |p| {
                                        p.x *= prev_len / new_len;
                                        if p.x.is_nan() {
                                            p.x = 0.;
                                        }
                                    });
                                }
                                cmds.push(WeightCmd::LineTo((1., pressure, 0.).into()));
                            }
                            _ => panic!("oh no"),
                        }
                    }
                    _ => (),
                },
                _ => (),
            }
        }

        if schedule_cb {
            win.request_frame();
        }
    }

    let data: &mut WinData = win.data_mut().downcast_mut().expect("Invalid window data");
    let cmd_buffer = data.renderer.new_cmd_buffer().unwrap();
    let (cmd_buffer, out_tex) = data.renderer.render(cmd_buffer).unwrap();

    let res = data
        .presenter
        .lock()
        .unwrap()
        .present(cmd_buffer, out_tex.color())
        .map(|f| f.then_signal_fence_and_flush().map(|f| f.wait(None)));

    if let Err(err) = res {
        println!("presenter error: {}", err);
    }
}
