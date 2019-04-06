extern crate cgmath;
extern crate lcms_prime;
extern crate narwhal;
extern crate vulkano;

use cgmath::{Matrix4, Rad, SquareMatrix, Vector2};
use lcms_prime::Profile;
use narwhal::data::cgmath_ext::Vector2Ext;
use narwhal::data::*;
use narwhal::node::*;
use narwhal::platform::event::*;
use narwhal::platform::*;
use narwhal::render::fx::MaskMode;
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
    cam_z: f64,
}

fn main() {
    let mut app = App::init("Narwhal Render Test", (0, 1, 0), |app| {
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

fn handle_app_event(app: &mut App, app_event: AppEvent) {
    match app_event {
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
                cam_z: 200.,
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
        let data: &mut WinData = win.data_mut().downcast_mut().expect("Invalid window data");

        match event {
            WindowEvent::Ready => {
                data.renderer.set_resolution(win_resolution);
                data.renderer.add_node_type(defs::COMPOSITE).unwrap();
                data.renderer.add_node_type(defs::CAMERA).unwrap();
                data.renderer.add_node_type(defs::MASK).unwrap();
                data.renderer.add_node_type(defs::GAUSSIAN_BLUR).unwrap();

                let mut cam = Node::empty(defs::CAMERA_NAME.into());
                cam.set(defs::CameraProps::Size.into(), win_size.into_f64());
                cam.set(defs::CameraProps::Offset.into(), Vector2::new(0., 0.));
                cam.set(
                    defs::CameraProps::Transform.into(),
                    Matrix4::from_translation((0., 0., data.cam_z).into()),
                );
                cam.set(defs::CameraProps::Fov.into(), 1.57079632);
                cam.set(defs::CameraProps::ClipNear.into(), 0.01);
                cam.set(defs::CameraProps::ClipFar.into(), 100.);

                let graph = data.renderer.graph_mut();

                let cam = graph.add_node(cam);
                graph.set_output(cam);

                let composite = Node::empty(defs::COMPOSITE_NAME.into());
                let composite = graph.add_node(composite);

                let mut drawables = Vec::new();

                let test_paths = test_paths();
                let horse_paths = horse();
                let mut cache_id = 0;
                for path in test_paths {
                    let stroke: StrokeWeight = vec![
                        WeightCmd::LineTo((0., 0., 0.).into()),
                        WeightCmd::QuadTo((0.5, 1., 0.).into(), (1., 0., 0.).into()),
                    ]
                    .into();
                    drawables.push(Drawable {
                        id: (composite, cache_id),
                        shape: Shape {
                            fill: None,
                            stroke: Some((stroke, 7., (1., 1., 1., 1.).into())),
                            transform: Some(Matrix4::from_translation((0., 0., 10.).into())),
                            path: path.into(),
                        },
                    });
                    cache_id += 1;
                }
                for path in horse_paths {
                    let path: Path2D = path.into();
                    drawables.push(Drawable {
                        id: (composite, cache_id),
                        shape: Shape {
                            fill: Some((0.16, 0.08, 0.04, 1.).into()),
                            stroke: None,
                            transform: Some(Matrix4::identity()),
                            path,
                        },
                    });
                    cache_id += 1;
                }

                graph.node_mut(&composite).unwrap().set(0, drawables);

                let mask_comp = Node::empty(defs::COMPOSITE_NAME.into());
                let mask_comp = graph.add_node(mask_comp);
                let mask_drawables = vec![Drawable {
                    id: (mask_comp, 0),
                    shape: Shape {
                        fill: Some((1., 0., 1., 1.).into()),
                        stroke: None,
                        transform: Some(
                            Matrix4::from_translation((0., 0., 100.).into())
                                * Matrix4::from_scale(0.5),
                        ),
                        path: vec![
                            Path2DCmd::JumpTo((0., -115.).into()),
                            Path2DCmd::CubicTo(
                                (63.51, -115.).into(),
                                (115., -63.51).into(),
                                (115., 0.).into(),
                            ),
                            Path2DCmd::CubicTo(
                                (115., 63.51).into(),
                                (63.51, 115.).into(),
                                (0., 115.).into(),
                            ),
                            Path2DCmd::CubicTo(
                                (-63.51, 115.).into(),
                                (-115., 63.51).into(),
                                (-115., 0.).into(),
                            ),
                            Path2DCmd::CubicTo(
                                (-115., -63.51).into(),
                                (-63.51, -115.).into(),
                                (0., -115.).into(),
                            ),
                        ]
                        .into(),
                    },
                }];
                graph.node_mut(&mask_comp).unwrap().set(0, mask_drawables);

                let mut blur = Node::empty(defs::GAUSSIAN_BLUR_NAME.into());
                blur.set(defs::GaussianProps::Radius.into(), 80.);
                let blur = graph.add_node(blur);

                let mut mask = Node::empty(defs::MASK_NAME.into());
                mask.set_any(defs::MaskProps::Mode.into(), MaskMode::AlphaMatte);
                let mask = graph.add_node(mask);

                graph.link(mask, 1, cam, 0);
                graph.link(composite, 1, mask, 0);
                graph.link(blur, 1, mask, 2);
                graph.link(mask_comp, 1, blur, 0);
            }
            WindowEvent::Resized(..) => {
                data.renderer.set_resolution(win_resolution);
                let graph = data.renderer.graph_mut();
                let cam = graph.output();
                graph
                    .node_mut(&cam)
                    .unwrap()
                    .set(defs::CameraProps::Size.into(), win_size.into_f64());
            }
            WindowEvent::UIEvent(event) => match event.event_type {
                EventType::PointerMoved => {
                    let x = event.point.x / win_size.x as f64;
                    let y = event.point.y / win_size.y as f64;

                    let graph = data.renderer.graph_mut();
                    let cam = graph.output();
                    graph.node_mut(&cam).unwrap().set(
                        defs::CameraProps::Transform.into(),
                        Matrix4::from_angle_x(Rad(y - 0.5))
                            * Matrix4::from_angle_y(-Rad(x - 0.5))
                            * Matrix4::from_translation((0., 0., data.cam_z).into()),
                    );
                }
                EventType::Scroll => {
                    let dz = event.vector.unwrap().y;

                    let graph = data.renderer.graph_mut();
                    let cam = graph.output();
                    let transform = match graph
                        .node(&cam)
                        .unwrap()
                        .get(defs::CameraProps::Transform.into())
                    {
                        Some(Value::Mat4(t)) => *t,
                        _ => panic!("oh no"),
                    };
                    graph.node_mut(&cam).unwrap().set(
                        defs::CameraProps::Transform.into(),
                        transform * Matrix4::from_translation((0., 0., dz).into()),
                    );

                    data.cam_z += dz;
                }
                _ => (),
            },
            _ => (),
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

fn test_paths() -> Vec<Vec<Path2DCmd>> {
    vec![
        vec![
            Path2DCmd::LineTo((0., 0.).into()),
            Path2DCmd::LineTo((-50., 0.).into()),
            Path2DCmd::LineTo((-50., -50.).into()),
            Path2DCmd::LineTo((0., -50.).into()),
            Path2DCmd::LineTo((-25., -25.).into()),
            Path2DCmd::LineTo((0., -40.).into()),
        ],
        vec![
            Path2DCmd::JumpTo((-20., 40.).into()),
            Path2DCmd::CubicTo((0., 80.).into(), (150., 90.).into(), (220., 40.).into()),
        ],
    ]
}

/// Horse text
fn horse() -> Vec<Vec<Path2DCmd>> {
    vec![
        vec![
            Path2DCmd::LineTo((5., 5.).into()),
            Path2DCmd::LineTo((13., 5.).into()),
            Path2DCmd::LineTo((13., 26.8583519).into()),
            Path2DCmd::CubicTo(
                (12.9887082, 32.0030792).into(),
                (15.4921118, 34.5754428).into(),
                (20.5102107, 34.5754428).into(),
            ),
            Path2DCmd::CubicTo(
                (22.7171613, 34.5754428).into(),
                (27.2432188, 31.983904).into(),
                (27.2432188, 26.8583519).into(),
            ),
            Path2DCmd::CubicTo(
                (27.2432188, 25.4626113).into(),
                (27.2432188, 18.1764941).into(),
                (27.2432188, 5.).into(),
            ),
            Path2DCmd::LineTo((34.4711627, 5.).into()),
            Path2DCmd::CubicTo(
                (34.4711627, 19.8808871).into(),
                (34.4711627, 27.8526063).into(),
                (34.4711627, 28.9151574).into(),
            ),
            Path2DCmd::CubicTo(
                (34.4711627, 35.8728403).into(),
                (29.7927894, 41.8905793).into(),
                (22.7171613, 41.8905793).into(),
            ),
            Path2DCmd::CubicTo(
                (18.3929854, 41.8905793).into(),
                (15.1539317, 40.5463897).into(),
                (13., 37.8580104).into(),
            ),
            Path2DCmd::LineTo((13., 56.).into()),
            Path2DCmd::LineTo((5., 56.).into()),
            Path2DCmd::LineTo((5., 5.).into()),
        ],
        vec![
            Path2DCmd::LineTo((57., 4.40274975).into()),
            Path2DCmd::CubicTo(
                (65.4102722, 4.40274975).into(),
                (72.3216718, 9.31617048).into(),
                (72.3216718, 23.2013749).into(),
            ),
            Path2DCmd::CubicTo(
                (72.3216718, 37.0865793).into(),
                (65.9314841, 42.).into(),
                (57., 42.).into(),
            ),
            Path2DCmd::CubicTo(
                (52.6217844, 42.).into(),
                (42.197762, 40.7652574).into(),
                (42.197762, 23.2013749).into(),
            ),
            Path2DCmd::CubicTo(
                (42.197762, 5.63749234).into(),
                (52.7124452, 4.40274975).into(),
                (57., 4.40274975).into(),
            ),
            Path2DCmd::JumpTo((57.2597169, 11.7443117).into()),
            Path2DCmd::CubicTo(
                (52.9721621, 11.7443117).into(),
                (49.5425758, 13.4598514).into(),
                (49.5425758, 23.2013749).into(),
            ),
            Path2DCmd::CubicTo(
                (49.5425758, 32.9428984).into(),
                (53.8223664, 34.5759006).into(),
                (57.2597169, 34.5759006).into(),
            ),
            Path2DCmd::CubicTo(
                (60.6970674, 34.5759006).into(),
                (65.0163438, 32.9428984).into(),
                (65.0163438, 23.2013749).into(),
            ),
            Path2DCmd::CubicTo(
                (65.0163438, 13.4598514).into(),
                (61.5472717, 11.7443117).into(),
                (57.2597169, 11.7443117).into(),
            ),
        ],
        vec![
            Path2DCmd::LineTo((80., 5.).into()),
            Path2DCmd::LineTo((87.4168412, 5.).into()),
            Path2DCmd::LineTo((87.4168412, 27.075002).into()),
            Path2DCmd::CubicTo(
                (87.4207044, 30.4398072).into(),
                (89.0178602, 32.8156105).into(),
                (92.2083086, 34.2024119).into(),
            ),
            Path2DCmd::CubicTo(
                (93.6764193, 34.8405598).into(),
                (96.1842329, 34.3676599).into(),
                (99.7317494, 32.7837122).into(),
            ),
            Path2DCmd::LineTo((104.416617, 39.2105984).into()),
            Path2DCmd::CubicTo(
                (101.327748, 41.0701995).into(),
                (98.418345, 42.).into(),
                (95.6884087, 42.).into(),
            ),
            Path2DCmd::CubicTo(
                (92.9584725, 42.).into(),
                (90.2012833, 40.5386129).into(),
                (87.4168412, 37.6158387).into(),
            ),
            Path2DCmd::LineTo((87.4168412, 42.).into()),
            Path2DCmd::LineTo((80., 42.).into()),
            Path2DCmd::LineTo((80., 5.).into()),
        ],
        vec![
            Path2DCmd::LineTo((103.977164, 10.5).into()),
            Path2DCmd::CubicTo(
                (109.567107, 6.50918292).into(),
                (115.031935, 4.51377439).into(),
                (120.371647, 4.51377439).into(),
            ),
            Path2DCmd::CubicTo(
                (123.433684, 4.51377439).into(),
                (132.762288, 6.04783558).into(),
                (134.411712, 12.0124629).into(),
            ),
            Path2DCmd::CubicTo(
                (135.033382, 14.2605368).into(),
                (136.461836, 18.7370457).into(),
                (132.288341, 23.5282786).into(),
            ),
            Path2DCmd::CubicTo(
                (130.785624, 25.2534193).into(),
                (127.074898, 27.0807662).into(),
                (120.766003, 27.0807662).into(),
            ),
            Path2DCmd::CubicTo(
                (117.032989, 27.0807662).into(),
                (113.591483, 27.0807662).into(),
                (113.591483, 31.2171252).into(),
            ),
            Path2DCmd::CubicTo(
                (113.591483, 32.7221957).into(),
                (115.331866, 34.7663531).into(),
                (120.371647, 34.7663531).into(),
            ),
            Path2DCmd::CubicTo(
                (123.145086, 34.7663531).into(),
                (126.336928, 33.7513398).into(),
                (129.947173, 31.7213133).into(),
            ),
            Path2DCmd::LineTo((134.049046, 37.2697578).into()),
            Path2DCmd::CubicTo(
                (130.104175, 39.7747553).into(),
                (127.114529, 41.2164035).into(),
                (125.080107, 41.5947023).into(),
            ),
            Path2DCmd::CubicTo(
                (119.574665, 42.6184342).into(),
                (114.017707, 40.9452985).into(),
                (111.947444, 40.0714174).into(),
            ),
            Path2DCmd::CubicTo(
                (107.609852, 38.2404711).into(),
                (105.506986, 32.7534474).into(),
                (106.712579, 27.8045394).into(),
            ),
            Path2DCmd::CubicTo(
                (108.163065, 21.8503529).into(),
                (113.688884, 20.7191192).into(),
                (116.781628, 20.3025128).into(),
            ),
            Path2DCmd::CubicTo(
                (120.371647, 19.8189211).into(),
                (125.625635, 19.2712131).into(),
                (126.339064, 18.756406).into(),
            ),
            Path2DCmd::CubicTo(
                (128.903303, 16.9060606).into(),
                (127.340685, 12.9998628).into(),
                (125.080107, 12.0124629).into(),
            ),
            Path2DCmd::CubicTo(
                (119.885204, 9.74337598).into(),
                (114.599876, 11.072555).into(),
                (109.224123, 16.).into(),
            ),
            Path2DCmd::LineTo((103.977164, 10.5).into()),
        ],
        vec![
            Path2DCmd::LineTo((156.359526, 4.48092341).into()),
            Path2DCmd::CubicTo(
                (158.171244, 4.48092341).into(),
                (161.707145, 4.78209084).into(),
                (164.572738, 6.19500656).into(),
            ),
            Path2DCmd::CubicTo(
                (165.83674, 6.8182381).into(),
                (167.564341, 8.19745031).into(),
                (169.755539, 10.3326432).into(),
            ),
            Path2DCmd::LineTo((164.572738, 14.8345357).into()),
            Path2DCmd::CubicTo(
                (162.127812, 12.982951).into(),
                (160.293956, 11.9220366).into(),
                (159.07117, 11.6517926).into(),
            ),
            Path2DCmd::CubicTo(
                (155.720493, 10.9112704).into(),
                (152.468398, 12.2200104).into(),
                (151.088218, 13.1148107).into(),
            ),
            Path2DCmd::CubicTo(
                (148.95252, 14.4994297).into(),
                (147.898658, 16.9527208).into(),
                (147.926632, 20.4746838).into(),
            ),
            Path2DCmd::LineTo((170.50274, 20.4746838).into()),
            Path2DCmd::CubicTo(
                (170.897409, 26.2045676).into(),
                (170.648342, 30.1138243).into(),
                (169.755539, 32.2024541).into(),
            ),
            Path2DCmd::CubicTo(
                (166.908846, 38.8620281).into(),
                (160.480012, 42.073683).into(),
                (155.77809, 42.073683).into(),
            ),
            Path2DCmd::CubicTo(
                (149.754726, 42.073683).into(),
                (145.079422, 38.8627257).into(),
                (142.648977, 34.2328369).into(),
            ),
            Path2DCmd::CubicTo(
                (140.894751, 30.8911145).into(),
                (140.55618, 26.832858).into(),
                (140.55618, 22.6016844).into(),
            ),
            Path2DCmd::CubicTo(
                (140.55618, 18.3731879).into(),
                (141.353182, 12.1244279).into(),
                (145.458649, 8.46763892).into(),
            ),
            Path2DCmd::CubicTo(
                (147.929914, 6.26645229).into(),
                (151.843882, 4.48092341).into(),
                (156.359526, 4.48092341).into(),
            ),
            Path2DCmd::JumpTo((148., 26.5302177).into()),
            Path2DCmd::CubicTo(
                (148.139167, 29.5735586).into(),
                (149.01107, 31.7612097).into(),
                (150.615708, 33.0931709).into(),
            ),
            Path2DCmd::CubicTo(
                (153.022665, 35.0911127).into(),
                (157.993565, 35.8436026).into(),
                (160.68793, 33.0931709).into(),
            ),
            Path2DCmd::CubicTo(
                (162.484173, 31.2595497).into(),
                (163.254863, 29.0718987).into(),
                (163., 26.5302177).into(),
            ),
            Path2DCmd::LineTo((148., 26.5302177).into()),
        ],
    ]
}
