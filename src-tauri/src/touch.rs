//! Touch support for the Edge on macOS.
//!
//! The Edge's touch panel shows up as its own USB touchscreen. macOS can't tie
//! a touchscreen to a display, so it maps every touch onto the *main* display
//! (a tap on the Edge clicks the same spot on your main screen).
//!
//! So Edge Deck takes the touchscreen over while it runs:
//! 1. Opens the touch device with "seize" (exclusive access). macOS stops
//!    turning touches into mouse clicks on the main display.
//! 2. Reads each touch's position itself (0..1 across the panel).
//! 3. Hands Down / Move / Up events, in Edge pixels, to a callback that
//!    delivers them straight into the Edge Deck window. The mouse never moves.
//!
//! Needs Input Monitoring permission (macOS asks once).

use std::collections::HashMap;

/// A rectangle in global screen points (CoreGraphics coordinates).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One touch event, in pixels relative to the Edge's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TouchEvent {
    Down(f64, f64),
    Move(f64, f64),
    Up(f64, f64),
}

/// The touch controller in the XENEON Edge.
pub const TOUCH_VENDOR_ID: i64 = 0x27c0;
pub const TOUCH_PRODUCT_ID: i64 = 0x0859;

/// Map a raw HID value onto 0..1 using the element's logical range.
pub fn normalize(v: i64, min: i64, max: i64) -> f64 {
    if max <= min {
        return 0.0;
    }
    ((v - min) as f64 / (max - min) as f64).clamp(0.0, 1.0)
}

/// What one HID value reports about a finger.
#[derive(Clone, Copy, Debug)]
pub enum Field {
    X(f64),
    Y(f64),
    Pressed(bool),
}

#[derive(Default, Clone, Copy, Debug)]
struct Contact {
    x: f64,
    y: f64,
    down: bool,
}

/// Turns raw per-finger HID values into one stream of Down/Move/Up for the
/// first finger. Values arrive one element at a time, so `flush` (called a
/// few ms after values stop arriving) decides what happened.
#[derive(Default, Debug)]
pub struct Tracker {
    contacts: HashMap<usize, Contact>,
    primary: Option<usize>,
    last: Option<(f64, f64)>,
}

impl Tracker {
    /// Record one value. `finger` identifies the finger's HID collection.
    pub fn update(&mut self, finger: usize, field: Field) {
        let c = self.contacts.entry(finger).or_default();
        match field {
            Field::X(x) => c.x = x,
            Field::Y(y) => c.y = y,
            Field::Pressed(d) => c.down = d,
        }
    }

    /// Emit at most one event, scaled to an Edge of `w` x `h` pixels.
    pub fn flush(&mut self, w: f64, h: f64) -> Option<TouchEvent> {
        let px = |c: &Contact| (c.x * w, c.y * h);
        match self.primary {
            None => {
                let (&id, c) = self.contacts.iter().filter(|(_, c)| c.down).min_by_key(|(id, _)| **id)?;
                let p = px(c);
                self.primary = Some(id);
                self.last = Some(p);
                Some(TouchEvent::Down(p.0, p.1))
            }
            Some(id) => {
                let c = *self.contacts.get(&id)?;
                if !c.down {
                    self.primary = None;
                    let (x, y) = self.last.take().unwrap_or(px(&c));
                    return Some(TouchEvent::Up(x, y));
                }
                let p = px(&c);
                let moved = self.last.map(|l| (l.0 - p.0).abs() + (l.1 - p.1).abs() >= 1.0).unwrap_or(true);
                if moved {
                    self.last = Some(p);
                    Some(TouchEvent::Move(p.0, p.1))
                } else {
                    None
                }
            }
        }
    }
}

/// The Edge's bounds in global screen points (macOS's own coordinates, which
/// already account for each display's scaling). None if it isn't connected.
pub fn edge_rect() -> Option<Rect> {
    #[cfg(target_os = "macos")]
    return mac::edge_rect();
    #[cfg(not(target_os = "macos"))]
    None
}

/// Whether touch takeover is working, for the UI and the log.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "state", content = "detail", rename_all = "camelCase")]
pub enum TouchStatus {
    Starting,
    On,
    NeedsPermission,
    Failed(String),
    Unsupported,
}

static STATUS: std::sync::Mutex<TouchStatus> = std::sync::Mutex::new(TouchStatus::Starting);

pub fn status() -> TouchStatus {
    if cfg!(target_os = "macos") { STATUS.lock().unwrap().clone() } else { TouchStatus::Unsupported }
}

#[allow(dead_code)]
fn set_status(s: TouchStatus) {
    *STATUS.lock().unwrap() = s;
}

/// Take over the touchscreen and send its events to `on_event`. Runs on its
/// own thread. No-op off macOS.
pub fn start(on_event: impl Fn(TouchEvent) + Send + 'static) {
    #[cfg(target_os = "macos")]
    mac::start(on_event);
    #[cfg(not(target_os = "macos"))]
    let _ = on_event;
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{normalize, Field, Rect, TouchEvent, Tracker, TOUCH_PRODUCT_ID, TOUCH_VENDOR_ID};
    use core_foundation::base::TCFType;
    use core_foundation::number::CFNumber;
    use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop, CFRunLoopRunInMode};
    use core_foundation::string::CFString;
    use core_graphics::display::CGDisplay;
    use std::ffi::c_void;
    use std::time::Duration;

    type Ref = *mut c_void;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOServiceMatching(name: *const std::ffi::c_char) -> Ref;
        fn IOServiceGetMatchingServices(main_port: u32, matching: Ref, iter: *mut u32) -> i32;
        fn IOIteratorNext(iter: u32) -> u32;
        fn IOObjectRelease(obj: u32) -> i32;
        fn IOHIDDeviceCreate(allocator: Ref, service: u32) -> Ref;
        fn IOHIDDeviceOpen(device: Ref, options: u32) -> i32;
        fn IOHIDDeviceGetProperty(device: Ref, key: Ref) -> Ref;
        fn IOHIDDeviceRegisterInputValueCallback(
            device: Ref,
            callback: extern "C" fn(*mut c_void, i32, Ref, Ref),
            context: *mut c_void,
        );
        fn IOHIDDeviceScheduleWithRunLoop(device: Ref, run_loop: Ref, mode: Ref);
        fn IOHIDDeviceUnscheduleFromRunLoop(device: Ref, run_loop: Ref, mode: Ref);
        fn IOHIDDeviceClose(device: Ref, options: u32) -> i32;
        fn IOHIDValueGetElement(value: Ref) -> Ref;
        fn IOHIDValueGetIntegerValue(value: Ref) -> isize;
        fn IOHIDElementGetUsagePage(element: Ref) -> u32;
        fn IOHIDElementGetUsage(element: Ref) -> u32;
        fn IOHIDElementGetLogicalMin(element: Ref) -> isize;
        fn IOHIDElementGetLogicalMax(element: Ref) -> isize;
        fn IOHIDElementGetParent(element: Ref) -> Ref;
        fn IOHIDRequestAccess(request_type: u32) -> bool;
        fn IOHIDCheckAccess(request_type: u32) -> u32;
    }

    const SEIZE_DEVICE: u32 = 0x01;
    const REQUEST_LISTEN_EVENT: u32 = 1;
    /// kIOReturnNotPermitted: Input Monitoring isn't in effect for this app.
    const NOT_PERMITTED: i32 = 0xe00002e2u32 as i32;

    // HID usages.
    const PAGE_GENERIC_DESKTOP: u32 = 0x01;
    const USAGE_X: u32 = 0x30;
    const USAGE_Y: u32 = 0x31;
    const PAGE_BUTTON: u32 = 0x09;
    const PAGE_DIGITIZER: u32 = 0x0D;
    const USAGE_TIP_SWITCH: u32 = 0x42;

    pub fn edge_rect() -> Option<Rect> {
        let ids = CGDisplay::active_displays().ok()?;
        ids.into_iter()
            .map(|id| CGDisplay::new(id).bounds())
            .filter(|b| b.size.height > 0.0 && b.size.width / b.size.height >= crate::EDGE_MIN_ASPECT)
            .max_by(|a, b| a.size.width.total_cmp(&b.size.width))
            .map(|b| Rect { x: b.origin.x, y: b.origin.y, w: b.size.width, h: b.size.height })
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDictionarySetValue(dict: Ref, key: Ref, value: Ref);
        fn CFRelease(obj: Ref);
    }

    /// Every HID interface of the Edge's touch controller.
    unsafe fn touch_devices() -> Vec<Ref> {
        let matching = IOServiceMatching(c"IOHIDDevice".as_ptr());
        if matching.is_null() {
            return vec![];
        }
        let (vk, vv) = (CFString::new("VendorID"), CFNumber::from(TOUCH_VENDOR_ID));
        let (pk, pv) = (CFString::new("ProductID"), CFNumber::from(TOUCH_PRODUCT_ID));
        CFDictionarySetValue(matching, vk.as_concrete_TypeRef() as Ref, vv.as_concrete_TypeRef() as Ref);
        CFDictionarySetValue(matching, pk.as_concrete_TypeRef() as Ref, pv.as_concrete_TypeRef() as Ref);
        let mut iter = 0u32;
        // Takes ownership of `matching`.
        if IOServiceGetMatchingServices(0, matching, &mut iter) != 0 {
            return vec![];
        }
        let mut out = vec![];
        loop {
            let service = IOIteratorNext(iter);
            if service == 0 {
                break;
            }
            let dev = IOHIDDeviceCreate(std::ptr::null_mut(), service);
            if !dev.is_null() {
                out.push(dev);
            }
            IOObjectRelease(service);
        }
        IOObjectRelease(iter);
        out
    }

    /// An integer property of a HID device (e.g. "PrimaryUsagePage").
    unsafe fn prop_i64(dev: Ref, key: &str) -> i64 {
        let k = CFString::new(key);
        let v = IOHIDDeviceGetProperty(dev, k.as_concrete_TypeRef() as Ref);
        if v.is_null() {
            return -1;
        }
        CFNumber::wrap_under_get_rule(v as _).to_i64().unwrap_or(-1)
    }

    /// HID value callback: turn one element's value into a `Field`.
    extern "C" fn on_value(ctx: *mut c_void, _result: i32, _sender: Ref, value: Ref) {
        let tracker = unsafe { &mut *(ctx as *mut Tracker) };
        unsafe {
            let el = IOHIDValueGetElement(value);
            let (page, usage) = (IOHIDElementGetUsagePage(el), IOHIDElementGetUsage(el));
            let v = IOHIDValueGetIntegerValue(value) as i64;
            let field = match (page, usage) {
                (PAGE_GENERIC_DESKTOP, USAGE_X) | (PAGE_GENERIC_DESKTOP, USAGE_Y) => {
                    let n = normalize(v, IOHIDElementGetLogicalMin(el) as i64, IOHIDElementGetLogicalMax(el) as i64);
                    if usage == USAGE_X { Field::X(n) } else { Field::Y(n) }
                }
                (PAGE_DIGITIZER, USAGE_TIP_SWITCH) | (PAGE_BUTTON, 1) => Field::Pressed(v != 0),
                _ => return,
            };
            // The parent collection identifies which finger this value is for.
            tracker.update(IOHIDElementGetParent(el) as usize, field);
        }
    }

    pub fn start(on_event: impl Fn(TouchEvent) + Send + 'static) {
        std::thread::spawn(move || unsafe {
            // Shows the Input Monitoring prompt the first time. The answer is
            // only logged: the real test is whether seizing the device works
            // (the access check can say "no" when the grant is in place).
            let requested = IOHIDRequestAccess(REQUEST_LISTEN_EVENT);
            let checked = IOHIDCheckAccess(REQUEST_LISTEN_EVENT);
            crate::log(&format!("touch: access request={requested} check={checked} (0 = granted)"));

            let tracker: *mut Tracker = Box::into_raw(Box::new(Tracker::default()));
            let run_loop = CFRunLoop::get_current().as_concrete_TypeRef() as Ref;
            let mut logged_first = false;
            let mut said_missing = false;

            // Outer loop: find the Edge, take it over, use it until it's
            // unplugged, then go back to looking. So touch works whether the
            // Edge was connected before or after the app started.
            loop {
                // The touch controller shows up as several HID interfaces (the
                // touch digitizer, a mouse-emulation interface, a vendor one).
                // Each must be seized on its own, or macOS keeps getting
                // touches from whichever one is left and moves the main pointer.
                let devices = touch_devices();
                if devices.is_empty() {
                    super::set_status(super::TouchStatus::Failed("touchscreen not found".into()));
                    if !said_missing {
                        crate::log("touch: Edge touchscreen not found (is its USB cable connected?); still looking");
                        said_missing = true;
                    }
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                }
                said_missing = false;
                crate::log(&format!("touch: found {} touch interfaces", devices.len()));
                for &dev in &devices {
                    IOHIDDeviceRegisterInputValueCallback(dev, on_value, tracker as *mut c_void);
                    IOHIDDeviceScheduleWithRunLoop(dev, run_loop, kCFRunLoopDefaultMode as Ref);
                }

                // Seize every interface. Not permitted means Input Monitoring
                // isn't in effect yet: keep retrying so touch turns on as soon
                // as it is, without restarting the app.
                let mut seized = vec![false; devices.len()];
                let mut last: Vec<Option<i32>> = vec![None; devices.len()];
                while seized.iter().any(|s| !s) {
                    for (i, &dev) in devices.iter().enumerate() {
                        if seized[i] {
                            continue;
                        }
                        let rc = IOHIDDeviceOpen(dev, SEIZE_DEVICE);
                        if last[i] != Some(rc) {
                            let (page, usage) = (prop_i64(dev, "PrimaryUsagePage"), prop_i64(dev, "PrimaryUsage"));
                            crate::log(&format!("touch: seize interface {page:#x}/{usage:#x} -> IOReturn {rc:#x}"));
                            last[i] = Some(rc);
                        }
                        seized[i] = rc == 0;
                    }
                    if seized.iter().all(|s| *s) {
                        break;
                    }
                    let not_permitted = last.iter().any(|r| *r == Some(NOT_PERMITTED));
                    super::set_status(if not_permitted {
                        super::TouchStatus::NeedsPermission
                    } else {
                        super::TouchStatus::Failed(format!("{:#x}", last.iter().flatten().find(|r| **r != 0).unwrap_or(&0)))
                    });
                    std::thread::sleep(Duration::from_secs(3));
                    if touch_devices().len() != devices.len() {
                        break; // unplugged while waiting; start over
                    }
                }
                if seized.iter().all(|s| *s) {
                    super::set_status(super::TouchStatus::On);
                    crate::log("touch: took over every Edge touch interface");

                    // Pump HID callbacks, then turn what arrived into touch
                    // events. Every ~3 seconds, check the Edge is still there.
                    let mut ticks = 0u32;
                    loop {
                        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.008, 0);
                        if let Some(edge) = edge_rect() {
                            while let Some(ev) = (*tracker).flush(edge.w, edge.h) {
                                if !logged_first {
                                    crate::log(&format!("touch: first touch received {ev:?}"));
                                    logged_first = true;
                                }
                                on_event(ev);
                                if matches!(ev, TouchEvent::Move(..)) {
                                    break;
                                }
                            }
                        }
                        ticks += 1;
                        if ticks % 375 == 0 && touch_devices().is_empty() {
                            crate::log("touch: Edge touchscreen unplugged; waiting for it to come back");
                            break;
                        }
                    }
                }
                // Let go of these interfaces before looking again.
                for (i, &dev) in devices.iter().enumerate() {
                    if seized[i] {
                        IOHIDDeviceClose(dev, 0);
                    }
                    IOHIDDeviceUnscheduleFromRunLoop(dev, run_loop, kCFRunLoopDefaultMode as Ref);
                    CFRelease(dev);
                }
                *tracker = Tracker::default();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_maps_logical_range() {
        assert_eq!(normalize(0, 0, 4095), 0.0);
        assert_eq!(normalize(4095, 0, 4095), 1.0);
        assert!((normalize(2048, 0, 4096) - 0.5).abs() < 1e-9);
        assert_eq!(normalize(5, 10, 10), 0.0);
    }

    #[test]
    fn tap_emits_down_then_up_in_edge_pixels() {
        let mut t = Tracker::default();
        t.update(1, Field::X(0.5));
        t.update(1, Field::Y(0.25));
        t.update(1, Field::Pressed(true));
        assert_eq!(t.flush(2560.0, 720.0), Some(TouchEvent::Down(1280.0, 180.0)));
        assert_eq!(t.flush(2560.0, 720.0), None);
        t.update(1, Field::Pressed(false));
        assert_eq!(t.flush(2560.0, 720.0), Some(TouchEvent::Up(1280.0, 180.0)));
        assert_eq!(t.flush(2560.0, 720.0), None);
    }

    #[test]
    fn drag_emits_moves_and_ignores_second_finger() {
        let mut t = Tracker::default();
        t.update(1, Field::Pressed(true));
        t.update(1, Field::X(0.1));
        t.update(1, Field::Y(0.5));
        assert!(matches!(t.flush(1000.0, 100.0), Some(TouchEvent::Down(..))));
        t.update(2, Field::Pressed(true));
        t.update(1, Field::Y(0.8));
        assert_eq!(t.flush(1000.0, 100.0), Some(TouchEvent::Move(100.0, 80.0)));
        t.update(1, Field::Pressed(false));
        assert_eq!(t.flush(1000.0, 100.0), Some(TouchEvent::Up(100.0, 80.0)));
    }
}
