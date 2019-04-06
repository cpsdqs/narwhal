//! Events.
//!
//! The first event on any receiver will always be `Ready`.
//!
//! No guarantees can be made about the order of events that may appear in pairs, such as
//! [AppEvent::Terminating] and [WindowEvent::Closing].

use cgmath::{Point2, Vector3};
use std::fmt;

/// Application-level events.
#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    /// The application has finished launching.
    Ready,

    /// The application is about to terminate.
    Terminating,
}

/// Window events.
#[derive(Debug, Clone, PartialEq)]
pub enum WindowEvent {
    /// The window is ready for use.
    Ready,

    /// A UI event.
    UIEvent(Event),

    /// A UI key event.
    UIKeyEvent(KeyEvent),

    /// The window was resized.
    Resized(usize, usize),

    /// The window’s color space or physical pixel scale changed.
    OutputChanged,

    /// The window is about to close.
    Closing,
}

/// Contains possible key modifiers.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    /// Whether or not the shift key is pressed.
    pub shift: bool,
    /// Whether or not the control key is pressed.
    pub ctrl: bool,
    /// Whether or not the option key is pressed.
    pub opt: bool,
    /// Whether or not the command key is pressed.
    pub cmd: bool,
}

impl fmt::Debug for Modifiers {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut mods = String::new();

        if self.ctrl {
            mods.push('⌃');
        }
        if self.opt {
            mods.push('⌥');
        }
        if self.shift {
            mods.push('⇧');
        }
        if self.cmd {
            mods.push('⌘');
        }

        write!(f, "Modifiers({})", mods)
    }
}

/// Event types.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum EventType {
    /// A pointer key was pressed.
    PointerDown,

    /// The pointer was dragged.
    PointerDragged,

    /// A pointer key was released.
    PointerUp,

    /// The pointer was moved.
    PointerMoved,

    /// The pointer entered this UI element.
    PointerEntered,

    /// The pointer exited this UI element.
    PointerExited,

    /// The pointer event was canceled and any effects should be reversed.
    ///
    /// This may occur, for example, when a scroll view decides that a tap is actually a swipe but
    /// has already sent what was thought to be a tap to a child element.
    PointerCancel,

    /// Scrolling.
    Scroll,

    /// Scaling.
    Scale,

    /// QuickLook.
    QuickLook,

    /// The set of pressed modifier keys changed.
    ModifiersChanged,

    /// The pointer pressure changed.
    PressureChanged,
}

/// The pointer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    /// The primary button, usually the left mouse button.
    Primary,

    /// The secondary button, usually the right mouse button.
    Secondary,

    /// The middle mouse button.
    Middle,

    /// Some other button that should probably be added to this enum.
    Other(usize),
}

impl Default for Button {
    fn default() -> Button {
        Button::Primary
    }
}

/// The pointing device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointingDevice {
    /// The default pointing device, usually a mouse or trackpad.
    Cursor,

    /// A pen.
    Pen,

    /// The erasing tip of a pen.
    Eraser,

    /// A finger, where dragging should be interpreted as scrolling.
    Touch,
}

impl Default for PointingDevice {
    fn default() -> PointingDevice {
        PointingDevice::Cursor
    }
}

// TODO: scrolling momentum phases

/// Events with a location.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    /// The event type.
    pub event_type: EventType,

    /// The event location.
    pub point: Point2<f64>,

    /// The button.
    pub button: Option<Button>,

    /// The pointing device.
    pub device: Option<PointingDevice>,

    /// Keyboard modifiers, such as Shift or Control.
    pub modifiers: Modifiers,

    /// Tablet pressure.
    pub pressure: Option<f64>,

    /// Event vector.
    pub vector: Option<Vector3<f64>>,

    /// Scale.
    pub scale: Option<f64>,
}

impl Event {
    /// Returns whether or not this is a pointer event.
    pub fn is_pointer_event(&self) -> bool {
        match self.event_type {
            EventType::PointerDown
            | EventType::PointerDragged
            | EventType::PointerUp
            | EventType::PointerMoved
            | EventType::PointerEntered
            | EventType::PointerExited => true,
            _ => false,
        }
    }

    /// Clones this event and applies the transform function to all point data.
    pub fn clone_with_point_transform<F>(&self, transform: F) -> Event
    where
        F: Fn(Point2<f64>) -> Point2<f64>,
    {
        Event {
            point: transform(self.point),
            ..self.clone()
        }
    }

    /// Clones this event with a different event type.
    /// Useful for e.g. creating PointerEntered from PointerMoved.
    pub fn clone_with_type(&self, event_type: EventType) -> Event {
        Event {
            event_type,
            ..self.clone()
        }
    }
}

/// Keyboard event types.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum KeyEventType {
    /// A key was pressed.
    KeyDown,

    /// A key was released.
    KeyUp,
}

/// Keyboard events.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    pub event_type: KeyEventType,
    pub modifiers: Modifiers,
    pub string: String,
    pub raw_string: Option<String>,
    pub repeating: bool,
    pub code: Option<KeyCode>,
}

/// Keyboard-layout-independent key codes.
///
/// - `Key`N refers to the number keys above the keyboard
/// - `Num`* refers to keys on the numpad
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Unknown,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,
    Equal,
    Minus,
    LeftBracket,
    RightBracket,
    Quote,
    Semicolon,
    Backslash,
    Comma,
    Slash,
    Period,
    Grave,
    NumDecimal,
    NumMultiply,
    NumPlus,
    NumClear,
    NumDivide,
    NumEnter,
    NumMinus,
    NumEquals,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Return,
    Tab,
    Space,
    Delete,
    Escape,
    Command,
    Shift,
    CapsLock,
    Option,
    Control,
    RightCommand,
    RightShift,
    RightOption,
    RightControl,
    Function,
    VolumeUp,
    VolumeDown,
    Mute,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    Help,
    Home,
    End,
    PageUp,
    PageDown,
    ForwardDelete,
    LeftArrow,
    RightArrow,
    UpArrow,
    DownArrow,
    ISOSection,
    Yen,
    Underscore,
    NumComma,
    Eisu,
    Kana,
}
