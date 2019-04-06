//! Cocoa FFI bindings.

#![allow(dead_code)]

use crate::event::KeyCode;
use cocoa::appkit::NSApplicationActivateIgnoringOtherApps;
pub use cocoa::appkit::NSEventMask;
use cocoa::foundation::NSDefaultRunLoopMode;
use cocoa_ffi::appkit::CGFloat;
use cocoa_ffi::appkit::NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular;
use cocoa_ffi::base::{id, nil};
pub use cocoa_ffi::foundation::{NSInteger, NSPoint, NSRect, NSSize, NSUInteger};
use objc::runtime::*;
use std::ffi::{CStr, CString};
use std::os::raw::c_float;
use std::{slice, str};

/// Converts a NSString to a Rust String.
fn nsstring_to_string(ns_string: id) -> String {
    let c_str = unsafe { CStr::from_ptr(msg_send![ns_string, UTF8String]) };
    let buf = c_str.to_bytes();
    String::from(str::from_utf8(buf).unwrap())
}

/// Converts a Rust &str to NSString.
fn string_to_nsstring(string: &str) -> id {
    let c_string = CString::new(string).unwrap();
    let ns_string: id = unsafe { msg_send![class!(NSString), alloc] };
    unsafe { msg_send![ns_string, initWithUTF8String:c_string.into_raw()] }
}

#[repr(u32)]
pub enum NCAppEventType {
    Ready = 0,
    Terminating = 1,
}

#[repr(u32)]
#[allow(dead_code)] // technically not dead code; the variants are just never constructed in *Rust*
pub enum NCWindowDevice {
    OpenGL = 0,
    Metal = 1,
}

#[repr(u32)]
pub enum NCWindowEventType {
    NSEvent = 0,
    Resized = 1,
    BackingUpdate = 2,
    WillClose = 3,
    Ready = 4,
}

#[repr(usize)] // NSUInteger
#[derive(PartialEq)]
pub enum NSEventType {
    LeftMouseDown = 1,
    LeftMouseUp = 2,
    RightMouseDown = 3,
    RightMouseUp = 4,
    MouseMoved = 5,
    LeftMouseDragged = 6,
    RightMouseDragged = 7,
    MouseEntered = 8,
    MouseExited = 9,
    KeyDown = 10,
    KeyUp = 11,
    FlagsChanged = 12,
    AppKitDefined = 13,
    SystemDefined = 14,
    ApplicationDefined = 15,
    Periodic = 16,
    CursorUpdate = 17,
    ScrollWheel = 22,
    TabletPoint = 23,
    TabletProximity = 24,
    OtherMouseDown = 25,
    OtherMouseUp = 26,
    OtherMouseDragged = 27,
    Gesture = 29,
    Magnify = 30,
    Swipe = 31,
    Rotate = 18,
    BeginGesture = 19,
    EndGesture = 20,
    SmartMagnify = 32,
    QuickLook = 33,
    Pressure = 34,
    DirectTouch = 37,
}

#[allow(non_upper_case_globals)]
pub const NSEventSubtypeApplicationActivated: u16 = 1;
#[allow(non_upper_case_globals)]
pub const NSEventSubtypeApplicationDeactivated: u16 = 2;
#[allow(non_upper_case_globals)]
pub const NSEventSubtypeMouseEvent: u16 = 0;
#[allow(non_upper_case_globals)]
pub const NSEventSubtypePowerOff: u16 = 1;
#[allow(non_upper_case_globals)]
pub const NSEventSubtypeScreenChanged: u16 = 8;
#[allow(non_upper_case_globals)]
pub const NSEventSubtypeTabletPoint: u16 = 1;
#[allow(non_upper_case_globals)]
pub const NSEventSubtypeTabletProximity: u16 = 2;
#[allow(non_upper_case_globals)]
pub const NSEventSubtypeTouch: u16 = 3;
#[allow(non_upper_case_globals)]
pub const NSEventSubtypeWindowExposed: u16 = 0;
#[allow(non_upper_case_globals)]
pub const NSEventSubtypeWindowMoved: u16 = 4;

#[repr(usize)] // NSUInteger
pub enum NSPointingDeviceType {
    Unknown = 0,
    Pen = 1,
    Cursor = 2,
    Eraser = 3,
}

// const NSEventModifierFlagCapsLock: NSUInteger = 1 << 16;
#[allow(non_upper_case_globals)]
pub const NSEventModifierFlagShift: NSUInteger = 1 << 17;
#[allow(non_upper_case_globals)]
pub const NSEventModifierFlagControl: NSUInteger = 1 << 18;
#[allow(non_upper_case_globals)]
pub const NSEventModifierFlagOption: NSUInteger = 1 << 19;
#[allow(non_upper_case_globals)]
pub const NSEventModifierFlagCommand: NSUInteger = 1 << 20;
// const NSEventModifierFlagNumericPad: NSUInteger = 1 << 21;
// const NSEventModifierFlagHelp: NSUInteger = 1 << 22;
// const NSEventModifierFlagFunction: NSUInteger = 1 << 23;
// const NSEventModifierFlagDeviceIndependentFlagsMask: NSUInteger = 0xffff0000;

#[allow(non_upper_case_globals)]
pub const NSEventButtonMaskTip: NSUInteger = 1;
#[allow(non_upper_case_globals)]
pub const NSEventButtonMaskPenLowerSide: NSUInteger = 2;
#[allow(non_upper_case_globals)]
pub const NSEventButtonMaskPenUpperSide: NSUInteger = 4;

pub enum NSRunLoopMode {
    Default,
}

#[link(name = "narwhal_platform")]
extern "C" {
    #[link_name = "OBJC_CLASS_$_NCAppEvent"]
    static OBJC_NCAppEvent: Class;
    #[link_name = "OBJC_CLASS_$_NCAppDelegate"]
    static OBJC_NCAppDelegate: Class;
    #[link_name = "OBJC_CLASS_$_NCWindowEvent"]
    static OBJC_NCWindowEvent: Class;
    #[link_name = "OBJC_CLASS_$_NCWindow"]
    static OBJC_NCWindow: Class;

    fn NCWakeApplication();
}

#[repr(C)]
pub struct NSApplication(pub id);
#[repr(C)]
pub struct NCAppDelegate(pub id);
#[repr(C)]
pub struct NCAppEvent(pub id);
#[repr(C)]
pub struct NCWindow(pub id);
#[repr(C)]
pub struct NCWindowEvent(pub id);
#[repr(C)]
pub struct NSEvent(pub id);
#[repr(C)]
pub struct NSColorSpace(pub id);
#[repr(C)]
pub struct NSDate(pub id);
#[repr(C)]
pub struct NSAutoreleasePool(pub id);
#[repr(C)]
pub struct NCAppEventArray(pub id);
#[repr(C)]
pub struct NCWindowEventArray(pub id);

pub type CAMetalLayer = id;

impl NSApplication {
    pub fn shared() -> NSApplication {
        NSApplication(unsafe { msg_send![Class::get("NSApplication").unwrap(), sharedApplication] })
    }

    pub unsafe fn set_delegate(&self, delegate: id) {
        msg_send![self.0, setDelegate: delegate];
    }

    pub unsafe fn wake(&mut self) {
        NCWakeApplication();
    }

    pub unsafe fn finish_launching_and_activate(&self) {
        let current_policy: NSInteger = msg_send![self.0, activationPolicy];
        let mut needs_activation = false;
        if current_policy != NSApplicationActivationPolicyRegular as NSInteger {
            msg_send![
                self.0,
                setActivationPolicy: NSApplicationActivationPolicyRegular
            ];
            needs_activation = true;
        }

        msg_send![self.0, finishLaunching];

        if needs_activation {
            // [NSApplication activateIgnoringOtherApps:YES] would do the same thing in theory
            // but using this method makes the menu bar work without refocusing
            let current: id = msg_send![
                Class::get("NSRunningApplication").unwrap(),
                currentApplication
            ];
            msg_send![
                current,
                activateWithOptions: NSApplicationActivateIgnoringOtherApps
            ];
        }
    }

    pub unsafe fn run(&self) {
        msg_send![self.0, run];
    }

    pub unsafe fn send_event(&self, event: NSEvent) {
        msg_send![self.0, sendEvent: event];
    }

    pub unsafe fn next_event(
        &self,
        matching_mask: NSEventMask,
        until_date: NSDate,
        in_mode: NSRunLoopMode,
        dequeue: bool,
    ) -> Option<NSEvent> {
        let dequeue = if dequeue { YES } else { NO };

        let mode = match in_mode {
            NSRunLoopMode::Default => NSDefaultRunLoopMode,
        };

        let i: id = msg_send![self.0, nextEventMatchingMask:matching_mask.bits()
                                                  untilDate:until_date
                                                     inMode:mode
                                                    dequeue:dequeue];
        if i == nil {
            None
        } else {
            Some(NSEvent(i))
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NCAppDelegateCallbackData {
    pub app_ptr: *mut (),
}

impl NCAppDelegate {
    pub fn new(callback: extern "C" fn(NCAppDelegate)) -> NCAppDelegate {
        unsafe {
            let i: id = msg_send![&OBJC_NCAppDelegate, alloc];
            NCAppDelegate(msg_send![i, initWithCallback: callback])
        }
    }

    pub fn is_metal_available() -> bool {
        let b: BOOL = unsafe { msg_send![&OBJC_NCAppDelegate, isMetalAvailable] };
        b == YES
    }

    pub fn set_dark_appearance(&self) {
        unsafe { msg_send![self.0, setDarkAppearance] };
    }

    pub fn set_default_main_menu(&self, app_name: &str) {
        let nsstring = string_to_nsstring(app_name);
        unsafe { msg_send![self.0, setDefaultMainMenu: nsstring] };
    }

    pub fn callback_data(&self) -> NCAppDelegateCallbackData {
        unsafe { msg_send![self.0, callbackData] }
    }

    pub fn set_callback_data(&self, data: NCAppDelegateCallbackData) {
        unsafe { msg_send![self.0, setCallbackData: data] }
    }

    pub fn dequeue_event(&self) -> Option<NCAppEvent> {
        let i: id = unsafe { msg_send![self.0, dequeueEvent] };
        if i == nil {
            return None;
        }
        Some(NCAppEvent(i))
    }
}

impl NCAppEvent {
    pub fn event_type(&self) -> NCAppEventType {
        unsafe { msg_send![self.0, eventType] }
    }
}

#[repr(C)]
pub struct NCWindowCallbackData {
    pub window_ptr: *mut (),
}

impl NCWindow {
    pub fn new(content_rect: NSRect, callback: extern "C" fn(NCWindow)) -> NCWindow {
        unsafe {
            let i: id = msg_send![&OBJC_NCWindow, alloc];
            let obj = msg_send![i, initWithContentRect:content_rect
                                             callback:callback];
            if obj == nil {
                panic!("Failed to create a Metal window");
            }
            NCWindow(obj)
        }
    }

    pub fn callback_data(&self) -> NCWindowCallbackData {
        unsafe { msg_send![self.0, callbackData] }
    }

    pub fn set_callback_data(&self, data: NCWindowCallbackData) {
        unsafe { msg_send![self.0, setCallbackData: data] }
    }

    pub fn request_frame(&self) {
        unsafe { msg_send![self.0, requestFrame] }
    }

    pub fn backing_scale_factor(&self) -> CGFloat {
        unsafe { msg_send![self.0, backingScaleFactor] }
    }

    pub fn center(&self) {
        unsafe { msg_send![self.0, center] }
    }

    pub fn frame(&self) -> NSRect {
        unsafe { msg_send![self.0, frame] }
    }

    pub fn set_frame(&self, frame: NSRect) {
        unsafe { msg_send![self.0, setFrame: frame display:YES] };
    }

    fn content_view(&self) -> id {
        unsafe { msg_send![self.0, contentView] }
    }

    pub fn content_view_frame(&self) -> NSRect {
        unsafe { msg_send![self.content_view(), frame] }
    }

    pub fn set_content_size(&self, size: NSSize) {
        unsafe { msg_send![self.0, setContentSize: size] }
    }

    pub fn color_space(&self) -> NSColorSpace {
        NSColorSpace(unsafe { msg_send![self.0, colorSpace] })
    }

    pub fn min_size(&self) -> NSSize {
        unsafe { msg_send![self.0, minSize] }
    }

    pub fn set_min_size(&self, size: NSSize) {
        unsafe { msg_send![self.0, setMinSize: size] }
    }

    pub fn max_size(&self) -> NSSize {
        unsafe { msg_send![self.0, maxSize] }
    }

    pub fn set_max_size(&self, size: NSSize) {
        unsafe { msg_send![self.0, setMaxSize: size] }
    }

    pub fn in_live_resize(&self) -> bool {
        let in_live_resize: BOOL = unsafe { msg_send![self.0, inLiveResize] };
        in_live_resize == YES
    }

    pub fn order_out(&self) {
        unsafe { msg_send![self.0, orderOut] };
    }

    pub fn order_front(&self) {
        unsafe { msg_send![self.0, orderFront] };
    }

    pub fn make_key_and_order_front(&self) {
        unsafe { msg_send![self.0, makeKeyAndOrderFront] };
    }

    pub fn device_type(&self) -> NCWindowDevice {
        unsafe { msg_send![self.0, deviceType] }
    }

    pub unsafe fn metal_layer(&self) -> id {
        msg_send![self.0, metalLayer]
    }

    pub unsafe fn set_device(&self, dev: id) {
        msg_send![self.0, setDevice: dev]
    }

    pub unsafe fn opengl_context(&self) -> id {
        msg_send![self.0, openGLContext]
    }

    pub fn dequeue_event(&self) -> Option<NCWindowEvent> {
        let i: id = unsafe { msg_send![self.0, dequeueEvent] };
        if i == nil {
            return None;
        }
        Some(NCWindowEvent(i))
    }

    pub fn title(&self) -> String {
        unsafe { nsstring_to_string(msg_send![self.0, title]) }
    }

    pub fn set_title(&self, title: &str) {
        let title = string_to_nsstring(title);
        unsafe { msg_send![self.0, setTitle: title] };
    }

    pub fn set_title_with_represented_filename(&self, filename: &str) {
        let filename = string_to_nsstring(filename);
        unsafe { msg_send![self.0, setTitleWithRepresentedFilename: filename] };
    }
}

impl NCWindowEvent {
    pub fn event_type(&self) -> NCWindowEventType {
        unsafe { msg_send![self.0, eventType] }
    }

    pub fn event(&self) -> Option<NSEvent> {
        unsafe {
            let event: id = msg_send![self.0, event];
            if event != nil {
                Some(NSEvent(event))
            } else {
                None
            }
        }
    }
}

impl NSEvent {
    pub fn event_type(&self) -> NSEventType {
        unsafe { msg_send![self.0, type] }
    }

    pub fn subtype(&self) -> u16 {
        unsafe { msg_send![self.0, subtype] }
    }

    // TODO: handle subtypes

    pub fn modifier_flags(&self) -> NSUInteger {
        unsafe { msg_send![self.0, modifierFlags] }
    }

    pub fn location_in_window(&self) -> NSPoint {
        unsafe { msg_send![self.0, locationInWindow] }
    }

    pub fn window(&self) -> NCWindow {
        NCWindow(unsafe { msg_send![self.0, window] })
    }

    pub fn is_a_repeat(&self) -> bool {
        let v: BOOL = unsafe { msg_send![self.0, isARepeat] };
        v == YES
    }

    pub fn characters(&self) -> String {
        nsstring_to_string(unsafe { msg_send![self.0, characters] })
    }

    pub fn characters_ignoring_modifiers(&self) -> String {
        nsstring_to_string(unsafe { msg_send![self.0, charactersIgnoringModifiers] })
    }

    pub fn key_code(&self) -> KeyCode {
        key_code_from_virtual(unsafe { msg_send![self.0, keyCode] })
    }

    pub fn button_number(&self) -> NSInteger {
        unsafe { msg_send![self.0, buttonNumber] }
    }

    pub fn delta_x(&self) -> CGFloat {
        unsafe { msg_send![self.0, deltaX] }
    }

    pub fn delta_y(&self) -> CGFloat {
        unsafe { msg_send![self.0, deltaY] }
    }

    pub fn delta_z(&self) -> CGFloat {
        unsafe { msg_send![self.0, deltaZ] }
    }

    pub fn pressure(&self) -> c_float {
        unsafe { msg_send![self.0, pressure] }
    }

    pub fn entering_proximity(&self) -> bool {
        let v: BOOL = unsafe { msg_send![self.0, isEnteringProximity] };
        v == YES
    }

    pub fn pointing_device_type(&self) -> NSPointingDeviceType {
        unsafe { msg_send![self.0, pointingDeviceType] }
    }

    pub fn pointing_device_id(&self) -> NSUInteger {
        unsafe { msg_send![self.0, pointingDeviceID] }
    }

    pub fn rotation(&self) -> c_float {
        unsafe { msg_send![self.0, rotation] }
    }

    pub fn button_mask(&self) -> NSUInteger {
        unsafe { msg_send![self.0, buttonMask] }
    }

    pub fn tilt(&self) -> NSPoint {
        unsafe { msg_send![self.0, tilt] }
    }

    pub fn magnification(&self) -> CGFloat {
        if self.event_type() == NSEventType::Magnify {
            unsafe { msg_send![self.0, magnification] }
        } else {
            0.
        }
    }

    pub fn data1(&self) -> NSInteger {
        unsafe { msg_send![self.0, data1] }
    }
}

impl NSColorSpace {
    pub fn cg_color_space(&self) -> *mut () {
        unsafe { msg_send![self.0, CGColorSpace] }
    }

    pub unsafe fn icc_profile_data(&self) -> &[u8] {
        let data: id = msg_send![self.0, ICCProfileData];
        let len: NSUInteger = msg_send![data, length];
        let ptr: *const u8 = msg_send![data, bytes];
        slice::from_raw_parts(ptr, len as usize)
    }
}

lazy_static! {
    static ref OBJC_NSDATE: &'static Class = Class::get("NSDate").unwrap();
}

impl NSDate {
    pub fn distant_future() -> NSDate {
        let i: id = unsafe { msg_send![*OBJC_NSDATE, distantFuture] };
        NSDate(i)
    }
    pub fn distant_past() -> NSDate {
        let i: id = unsafe { msg_send![*OBJC_NSDATE, distantPast] };
        NSDate(i)
    }
}

impl NSAutoreleasePool {
    pub fn new() -> NSAutoreleasePool {
        lazy_static! {
            static ref OBJC_NSAUTORELEASEPOOL: &'static Class =
                Class::get("NSAutoreleasePool").unwrap();
        }
        unsafe {
            let i: id = msg_send![*OBJC_NSAUTORELEASEPOOL, alloc];
            NSAutoreleasePool(msg_send![i, init])
        }
    }
}

impl Drop for NSAutoreleasePool {
    fn drop(&mut self) {
        unsafe { msg_send![self.0, release] };
    }
}

/// Returns a key code for the given carbon virtual key code.
fn key_code_from_virtual(code: u16) -> KeyCode {
    use crate::event::KeyCode::*;

    match code {
        0x00 => A,
        0x01 => S,
        0x02 => D,
        0x03 => F,
        0x04 => H,
        0x05 => G,
        0x06 => Z,
        0x07 => X,
        0x08 => C,
        0x09 => V,
        0x0a => ISOSection,
        0x0b => B,
        0x0c => Q,
        0x0d => W,
        0x0e => E,
        0x0f => R,
        0x10 => Y,
        0x11 => T,
        0x12 => Key1,
        0x13 => Key2,
        0x14 => Key3,
        0x15 => Key4,
        0x16 => Key6,
        0x17 => Key5,
        0x18 => Equal,
        0x19 => Key9,
        0x1a => Key7,
        0x1b => Minus,
        0x1c => Key8,
        0x1d => Key0,
        0x1e => RightBracket,
        0x1f => O,
        0x20 => U,
        0x21 => LeftBracket,
        0x22 => I,
        0x23 => P,
        0x24 => Return,
        0x25 => L,
        0x26 => J,
        0x27 => Quote,
        0x28 => K,
        0x29 => Semicolon,
        0x2a => Backslash,
        0x2b => Comma,
        0x2c => Slash,
        0x2d => N,
        0x2e => M,
        0x2f => Period,
        0x30 => Tab,
        0x31 => Space,
        0x32 => Grave,
        0x33 => Delete,
        0x35 => Escape,
        0x36 => RightCommand,
        0x37 => Command,
        0x38 => Shift,
        0x39 => CapsLock,
        0x3a => Option,
        0x3b => Control,
        0x3c => RightShift,
        0x3d => RightOption,
        0x3e => RightControl,
        0x3f => Function,
        0x40 => F17,
        0x41 => NumDecimal,
        0x43 => NumMultiply,
        0x45 => NumPlus,
        0x47 => NumClear,
        0x48 => VolumeUp,
        0x49 => VolumeDown,
        0x4a => Mute,
        0x4b => NumDivide,
        0x4c => NumEnter,
        0x4e => NumMinus,
        0x4f => F18,
        0x50 => F19,
        0x51 => NumEquals,
        0x52 => Num0,
        0x53 => Num1,
        0x54 => Num2,
        0x55 => Num3,
        0x56 => Num4,
        0x57 => Num5,
        0x58 => Num6,
        0x59 => Num7,
        0x5a => F20,
        0x5b => Num8,
        0x5c => Num9,
        0x5d => Yen,
        0x5e => Underscore,
        0x5f => NumComma,
        0x60 => F5,
        0x61 => F6,
        0x62 => F7,
        0x63 => F3,
        0x64 => F8,
        0x65 => F9,
        0x66 => Eisu,
        0x67 => F11,
        0x68 => Kana,
        0x69 => F13,
        0x6a => F16,
        0x6b => F14,
        0x6d => F10,
        0x6f => F12,
        0x71 => F15,
        0x72 => Help,
        0x73 => Home,
        0x74 => PageUp,
        0x75 => ForwardDelete,
        0x76 => F4,
        0x77 => End,
        0x78 => F2,
        0x79 => PageDown,
        0x7a => F1,
        0x7b => LeftArrow,
        0x7c => RightArrow,
        0x7d => DownArrow,
        0x7e => UpArrow,
        _ => Unknown,
    }
}
