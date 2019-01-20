use super::{SurfaceID, Update, WindowUpdate};
use crate::event::{Event, EventType, Modifiers, PointingDevice, WindowEvent};
use std::collections::HashMap;
use std::sync::mpsc;
use wayland_client::protocol::wl_keyboard::{
    Event as KeyboardEvent, RequestsTrait as KeyboardReq, WlKeyboard,
};
use wayland_client::protocol::wl_pointer::{
    Event as PointerEvent, RequestsTrait as PointerReq, WlPointer,
};
use wayland_client::protocol::wl_registry::{RequestsTrait as RegistryReq, WlRegistry};
use wayland_client::protocol::wl_seat::{
    Capability, Event as SeatEvent, RequestsTrait as SeatReq, WlSeat,
};
use wayland_client::protocol::wl_touch::{Event as TouchEvent, RequestsTrait as TouchReq, WlTouch};
use wayland_client::{NewProxy, Proxy};
use wayland_protocols::unstable::tablet::v2::client::zwp_tablet_manager_v2::{
    RequestsTrait as ZwpTabletManagerV2Req, ZwpTabletManagerV2,
};
use wayland_protocols::unstable::tablet::v2::client::zwp_tablet_seat_v2::Event as TabletEvent;
use wayland_protocols::unstable::tablet::v2::client::zwp_tablet_tool_v2::{
    Event as TabletToolEvent, Type as TabletToolType, ZwpTabletToolV2,
};

pub struct InputHandler {
    update_sender: mpsc::Sender<WindowUpdate>,
    seats: HashMap<u32, Proxy<WlSeat>>,
    zwp_tablet_manager: Option<Proxy<ZwpTabletManagerV2>>,
    tablet_manager_needs_init: bool,
}

impl InputHandler {
    pub(super) fn new(update_sender: mpsc::Sender<WindowUpdate>) -> InputHandler {
        InputHandler {
            update_sender,
            seats: HashMap::new(),
            zwp_tablet_manager: None,
            tablet_manager_needs_init: false,
        }
    }

    pub fn add_seat(&mut self, id: u32, version: u32, registry: &Proxy<WlRegistry>) {
        let mut seat_handler = SeatHandler::new(self.update_sender.clone());

        self.seats.insert(
            id,
            registry
                .bind(version.min(5), id, |seat: NewProxy<WlSeat>| {
                    seat.implement(
                        move |event, seat| match event {
                            SeatEvent::Name { name } => seat_handler.set_name(seat, name),
                            SeatEvent::Capabilities { capabilities } => {
                                seat_handler.set_caps(seat, capabilities);
                            }
                        },
                        (),
                    )
                })
                .unwrap(),
        );

        self.try_init_tablet_manager();
    }

    pub fn remove_seat(&mut self, id: u32) {
        if let Some(seat) = self.seats.get(&id) {
            seat.release();
        }
    }

    pub fn add_tablet_manager(&mut self, id: u32, version: u32, registry: &Proxy<WlRegistry>) {
        let update_sender = self.update_sender.clone();
        let manager = registry
            .bind(
                version.min(1),
                id,
                |manager: NewProxy<ZwpTabletManagerV2>| {
                    manager.implement(|_event, _manager| {}, ())
                },
            )
            .unwrap();

        self.zwp_tablet_manager = Some(manager);

        // FIXME: what if there are multiple tablet managers??

        self.tablet_manager_needs_init = true;

        self.try_init_tablet_manager();
    }

    fn try_init_tablet_manager(&mut self) {
        if !self.tablet_manager_needs_init {
            return;
        }
        let (seat, manager) = match (self.seats.iter().next(), &self.zwp_tablet_manager) {
            (Some((_, seat)), Some(manager)) => (seat, manager),
            _ => return,
        };
        self.tablet_manager_needs_init = false;

        let update_sender = self.update_sender.clone();

        manager
            .get_tablet_seat(seat, |seat| {
                seat.implement(
                    move |event, _seat| {
                        let update_sender = update_sender.clone();
                        match event {
                            TabletEvent::TabletAdded { .. } => {}
                            TabletEvent::ToolAdded { id: tool } => {
                                TabletToolHandler::new(update_sender, tool);
                            }
                            TabletEvent::PadAdded { id } => {
                                // TODO
                            }
                        }
                    },
                    (),
                )
            })
            .unwrap();
    }

    pub fn remove_tablet_manager(&mut self, _id: u32) {
        if let Some(manager) = &self.zwp_tablet_manager {
            manager.destroy();
        }
    }
}

struct SeatHandler {
    update_sender: mpsc::Sender<WindowUpdate>,
    pointer: Option<Proxy<WlPointer>>,
    keyboard: Option<Proxy<WlKeyboard>>,
    touch: Option<Proxy<WlTouch>>,
}

impl SeatHandler {
    fn new(update_sender: mpsc::Sender<WindowUpdate>) -> SeatHandler {
        SeatHandler {
            update_sender,
            pointer: None,
            keyboard: None,
            touch: None,
        }
    }

    fn set_name(&mut self, seat: Proxy<WlSeat>, name: String) {
        println!("name: {:?}", name);
    }

    #[allow(unused)] // TODO: <-- remove
    fn set_caps(&mut self, seat: Proxy<WlSeat>, caps: Capability) {
        println!("caps: {:?}", caps);
        if caps.contains(Capability::Pointer) && self.pointer.is_none() {
            let update_sender = self.update_sender.clone();
            seat.get_pointer(|pointer| {
                pointer.implement(
                    |event, pointer| match event {
                        PointerEvent::Enter {
                            serial,
                            surface,
                            surface_x,
                            surface_y,
                        } => {}
                        PointerEvent::Leave { serial, surface } => {}
                        PointerEvent::Motion {
                            time,
                            surface_x,
                            surface_y,
                        } => {}
                        PointerEvent::Button {
                            serial,
                            time,
                            button,
                            state,
                        } => {}
                        PointerEvent::Axis { time, axis, value } => {}
                        PointerEvent::Frame => {}
                        PointerEvent::AxisSource { axis_source } => {}
                        PointerEvent::AxisStop { time, axis } => {}
                        PointerEvent::AxisDiscrete { axis, discrete } => {}
                    },
                    (),
                )
            })
            .unwrap();
        } else if !caps.contains(Capability::Pointer) && self.pointer.is_some() {
            self.pointer.take().unwrap().release();
        }

        if caps.contains(Capability::Keyboard) && self.keyboard.is_none() {
            let update_sender = self.update_sender.clone();
            seat.get_keyboard(|keyboard| {
                keyboard.implement(
                    |event, keyboard| match event {
                        KeyboardEvent::Keymap { format, fd, size } => {}
                        KeyboardEvent::Enter {
                            serial,
                            surface,
                            keys,
                        } => {}
                        KeyboardEvent::Leave { serial, surface } => {}
                        KeyboardEvent::Key {
                            serial,
                            time,
                            key,
                            state,
                        } => {}
                        KeyboardEvent::Modifiers {
                            serial,
                            mods_depressed,
                            mods_latched,
                            mods_locked,
                            group,
                        } => {}
                        KeyboardEvent::RepeatInfo { rate, delay } => {}
                    },
                    (),
                )
            })
            .unwrap();
        } else if !caps.contains(Capability::Keyboard) && self.keyboard.is_some() {
            self.keyboard.take().unwrap().release();
        }

        if caps.contains(Capability::Touch) && self.touch.is_none() {
            let update_sender = self.update_sender.clone();
            seat.get_touch(|touch| {
                touch.implement(
                    |event, touch| match event {
                        TouchEvent::Down {
                            serial,
                            time,
                            surface,
                            id,
                            x,
                            y,
                        } => {}
                        TouchEvent::Up { serial, time, id } => {}
                        TouchEvent::Motion { time, id, x, y } => {}
                        TouchEvent::Frame => {}
                        TouchEvent::Cancel => {}
                    },
                    (),
                )
            })
            .unwrap();
        } else if !caps.contains(Capability::Touch) && self.touch.is_some() {
            self.touch.take().unwrap().release();
        }
    }
}

impl Drop for SeatHandler {
    fn drop(&mut self) {
        if let Some(pointer) = &self.pointer {
            pointer.release();
        }
        if let Some(keyboard) = &self.keyboard {
            keyboard.release();
        }
        if let Some(touch) = &self.touch {
            touch.release();
        }
    }
}

struct TabletToolHandler {
    surface_id: SurfaceID,
    is_down: bool,
    event_type: EventType,
    dev_type: PointingDevice,
    pressure: f64,
    x: f64,
    y: f64,
}

impl TabletToolHandler {
    fn new(update_sender: mpsc::Sender<WindowUpdate>, tablet_tool: NewProxy<ZwpTabletToolV2>) {
        let mut state = TabletToolHandler {
            surface_id: 0,
            is_down: false,
            event_type: EventType::PointerCancel,
            dev_type: PointingDevice::Pen,
            pressure: 0.,
            x: 0.,
            y: 0.,
        };

        #[allow(unused)] // TODO: <--- remove
        tablet_tool.implement(
            move |event, _tool| {
                match event {
                    TabletToolEvent::Type { tool_type } => match tool_type {
                        TabletToolType::Pen
                        | TabletToolType::Pencil
                        | TabletToolType::Brush
                        | TabletToolType::Airbrush => {
                            state.dev_type = PointingDevice::Pen;
                        }
                        TabletToolType::Eraser => {
                            state.dev_type = PointingDevice::Eraser;
                        }
                        TabletToolType::Finger | TabletToolType::Mouse | TabletToolType::Lens => {
                            // FIXME: I don’t know what to make of these
                            state.dev_type = PointingDevice::Cursor;
                        }
                    },
                    TabletToolEvent::HardwareSerial {
                        hardware_serial_hi,
                        hardware_serial_lo,
                    } => {}
                    TabletToolEvent::HardwareIdWacom {
                        hardware_id_hi,
                        hardware_id_lo,
                    } => {}
                    TabletToolEvent::Capability { capability } => {}
                    TabletToolEvent::Done => {}
                    TabletToolEvent::Removed => {
                        // TODO: destroy somehow?
                    }
                    TabletToolEvent::ProximityIn {
                        serial,
                        tablet,
                        surface,
                    } => {
                        state.surface_id = surface.id();
                        state.event_type = EventType::PointerEntered;
                    }
                    TabletToolEvent::ProximityOut => {
                        state.event_type = EventType::PointerExited;
                    }
                    TabletToolEvent::Down { serial } => {
                        state.event_type = EventType::PointerDown;
                        state.is_down = true;
                    }
                    TabletToolEvent::Up => {
                        state.event_type = EventType::PointerUp;
                        state.is_down = false;
                    }
                    TabletToolEvent::Motion { x, y } => {
                        state.event_type = if state.is_down {
                            EventType::PointerDragged
                        } else {
                            EventType::PointerMoved
                        };
                        state.x = x;
                        state.y = y;
                    }
                    TabletToolEvent::Pressure { pressure } => {
                        state.pressure = pressure as f64 / 65535.;
                    }
                    TabletToolEvent::Distance { distance } => {}
                    TabletToolEvent::Tilt { tilt_x, tilt_y } => {}
                    TabletToolEvent::Rotation { degrees } => {}
                    TabletToolEvent::Slider { position } => {}
                    TabletToolEvent::Wheel { degrees, clicks } => {}
                    TabletToolEvent::Button {
                        serial,
                        button,
                        state,
                    } => {}
                    TabletToolEvent::Frame { time } => {
                        let event = Event {
                            event_type: state.event_type,
                            point: (state.x, state.y).into(),
                            pressure: Some(state.pressure),
                            button: None,
                            device: Some(state.dev_type),
                            scale: None,
                            vector: None,
                            modifiers: Modifiers {
                                // TODO: these
                                cmd: false,
                                ctrl: false,
                                opt: false,
                                shift: false,
                            },
                        };

                        update_sender.send(WindowUpdate {
                            id: state.surface_id,
                            update: Update::Event(WindowEvent::UIEvent(event)),
                        });
                    }
                }
            },
            (),
        );
    }
}
