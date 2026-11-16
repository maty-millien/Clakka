use std::cell::Cell;
use std::ffi::{c_char, c_void, CStr};
use std::ptr;

use crate::audio::{Audio, SoundType};

const PAGE_GENERIC_DESKTOP: u32 = 0x01;
const PAGE_KEYBOARD: u32 = 0x07;
const PAGE_CONSUMER: u32 = 0x0C;
const PAGE_APPLE_TOP_CASE: u32 = 0xFF;
const PAGE_APPLE_KEYBOARD: u32 = 0xFF01;
const USAGE_KEYBOARD: i32 = 0x06;
const USAGE_CONSUMER_CONTROL: i32 = 0x01;

const KEY_FN: i32 = 63;
const CTRL_KEYS: u128 = 1 << 59 | 1 << 62;
const ALT_KEYS: u128 = 1 << 58 | 1 << 61;
const CMD_KEYS: u128 = 1 << 55 | 1 << 54;

const REQUEST_LISTEN_EVENT: u32 = 1;
const IO_RETURN_EXCLUSIVE_ACCESS: i32 = 0xE000_02C5_u32 as i32;
const CF_NUMBER_SINT32: isize = 3;
const CF_STRING_UTF8: u32 = 0x0800_0100;

struct HidState<'a> {
    audio: &'a Audio,
    pressed: Cell<u128>,
}

pub struct Input<'a> {
    state: Box<HidState<'a>>,
    manager: IOHIDManagerRef,
}

#[derive(Debug)]
pub struct AccessError;

impl std::fmt::Display for AccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to open HID keyboards. Check Input Monitoring permissions.")
    }
}

impl<'a> Input<'a> {
    pub fn new(audio: &'a Audio) -> Self {
        Input {
            state: Box::new(HidState {
                audio,
                pressed: Cell::new(0),
            }),
            manager: ptr::null_mut(),
        }
    }

    pub fn start(&mut self) -> Result<(), AccessError> {
        unsafe {
            if !IOHIDRequestAccess(REQUEST_LISTEN_EVENT) {
                return Err(AccessError);
            }
            let manager = IOHIDManagerCreate(ptr::null(), 0);
            if manager.is_null() {
                return Err(AccessError);
            }
            let matching = device_matching();
            IOHIDManagerSetDeviceMatchingMultiple(manager, matching);
            CFRelease(matching);
            IOHIDManagerRegisterInputValueCallback(
                manager,
                value_callback,
                (&*self.state as *const HidState).cast_mut().cast(),
            );
            IOHIDManagerScheduleWithRunLoop(manager, CFRunLoopGetCurrent(), kCFRunLoopCommonModes);
            if !matches!(IOHIDManagerOpen(manager, 0), 0 | IO_RETURN_EXCLUSIVE_ACCESS) {
                IOHIDManagerUnscheduleFromRunLoop(
                    manager,
                    CFRunLoopGetCurrent(),
                    kCFRunLoopCommonModes,
                );
                CFRelease(manager);
                return Err(AccessError);
            }
            self.manager = manager;
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        let manager = std::mem::replace(&mut self.manager, ptr::null_mut());
        if manager.is_null() {
            return;
        }
        unsafe {
            IOHIDManagerUnscheduleFromRunLoop(
                manager,
                CFRunLoopGetCurrent(),
                kCFRunLoopCommonModes,
            );
            IOHIDManagerClose(manager, 0);
            CFRelease(manager);
        }
        self.state.pressed.set(0);
    }
}

impl Drop for Input<'_> {
    fn drop(&mut self) {
        self.stop();
    }
}

unsafe fn device_matching() -> CFArrayRef {
    let page_key = cf_string(c"DeviceUsagePage");
    let usage_key = cf_string(c"DeviceUsage");
    let pairs = [
        (PAGE_GENERIC_DESKTOP as i32, USAGE_KEYBOARD),
        (PAGE_CONSUMER as i32, USAGE_CONSUMER_CONTROL),
    ];
    let dicts = pairs.map(|(page, usage)| {
        let values = [cf_number(page), cf_number(usage)];
        let keys = [page_key, usage_key];
        let dict = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            2,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        values.iter().for_each(|&v| CFRelease(v));
        dict
    });
    let array = CFArrayCreate(ptr::null(), dicts.as_ptr(), 2, &kCFTypeArrayCallBacks);
    dicts.iter().for_each(|&d| CFRelease(d));
    CFRelease(page_key);
    CFRelease(usage_key);
    array
}

unsafe fn cf_string(s: &CStr) -> *const c_void {
    CFStringCreateWithCString(ptr::null(), s.as_ptr(), CF_STRING_UTF8)
}

unsafe fn cf_number(n: i32) -> *const c_void {
    CFNumberCreate(ptr::null(), CF_NUMBER_SINT32, (&n as *const i32).cast())
}

unsafe extern "C" fn value_callback(
    context: *mut c_void,
    _result: i32,
    _sender: *mut c_void,
    value: IOHIDValueRef,
) {
    let element = IOHIDValueGetElement(value);
    let page = IOHIDElementGetUsagePage(element);
    let usage = IOHIDElementGetUsage(element);
    let Some(key_code) = key_code(page, usage) else {
        return;
    };
    let state = &*(context as *const HidState);
    let down = IOHIDValueGetIntegerValue(value) != 0;
    let bit = 1u128 << key_code;
    let pressed = state.pressed.get();
    if (pressed & bit != 0) == down {
        return;
    }
    let pressed = pressed ^ bit;
    state.pressed.set(pressed);
    let event_time = IOHIDValueGetTimeStamp(value);
    if down {
        if pressed & CTRL_KEYS != 0 && pressed & ALT_KEYS != 0 && pressed & CMD_KEYS != 0 {
            switch_theme(state.audio, key_code);
        }
        state.audio.play(key_code, SoundType::Press, event_time);
    } else {
        state.audio.play(key_code, SoundType::Release, event_time);
    }
}

fn key_code(page: u32, usage: u32) -> Option<i32> {
    let code = match (page, usage) {
        (PAGE_KEYBOARD, 0x04..=0xE7) => KEYBOARD_CODES[usage as usize - 0x04],
        (PAGE_APPLE_TOP_CASE | PAGE_APPLE_KEYBOARD, 0x03) => KEY_FN,
        (PAGE_CONSUMER, 0x70) | (PAGE_APPLE_TOP_CASE, 0x05) | (PAGE_APPLE_KEYBOARD, 0x21) => 122,
        (PAGE_CONSUMER, 0x6F) | (PAGE_APPLE_TOP_CASE, 0x04) | (PAGE_APPLE_KEYBOARD, 0x20) => 120,
        (PAGE_CONSUMER, 0x29F) | (PAGE_APPLE_KEYBOARD, 0x10) => 99,
        (PAGE_CONSUMER, 0x221 | 0x2A2) | (PAGE_APPLE_KEYBOARD, 0x01 | 0x04) => 118,
        (PAGE_CONSUMER, 0xCF) | (PAGE_APPLE_TOP_CASE, 0x09) => 96,
        (PAGE_GENERIC_DESKTOP, 0x9B) | (PAGE_APPLE_TOP_CASE, 0x08) => 97,
        (PAGE_CONSUMER, 0xB4 | 0xB6) => 98,
        (PAGE_CONSUMER, 0xCD) => 100,
        (PAGE_CONSUMER, 0xB3 | 0xB5) => 101,
        (PAGE_CONSUMER, 0xE2) => 109,
        (PAGE_CONSUMER, 0xEA) => 103,
        (PAGE_CONSUMER, 0xE9) => 111,
        _ => return None,
    };
    (code >= 0).then_some(code)
}

const KEYBOARD_CODES: [i32; 0xE8 - 0x04] = {
    let mut t = [-1; 0xE8 - 0x04];
    let pairs: [(usize, i32); 125] = [
        (0x04, 0),
        (0x05, 11),
        (0x06, 8),
        (0x07, 2),
        (0x08, 14),
        (0x09, 3),
        (0x0A, 5),
        (0x0B, 4),
        (0x0C, 34),
        (0x0D, 38),
        (0x0E, 40),
        (0x0F, 37),
        (0x10, 46),
        (0x11, 45),
        (0x12, 31),
        (0x13, 35),
        (0x14, 12),
        (0x15, 15),
        (0x16, 1),
        (0x17, 17),
        (0x18, 32),
        (0x19, 9),
        (0x1A, 13),
        (0x1B, 7),
        (0x1C, 16),
        (0x1D, 6),
        (0x1E, 18),
        (0x1F, 19),
        (0x20, 20),
        (0x21, 21),
        (0x22, 23),
        (0x23, 22),
        (0x24, 26),
        (0x25, 28),
        (0x26, 25),
        (0x27, 29),
        (0x28, 36),
        (0x29, 53),
        (0x2A, 51),
        (0x2B, 48),
        (0x2C, 49),
        (0x2D, 27),
        (0x2E, 24),
        (0x2F, 33),
        (0x30, 30),
        (0x31, 42),
        (0x32, 42),
        (0x33, 41),
        (0x34, 39),
        (0x35, 50),
        (0x36, 43),
        (0x37, 47),
        (0x38, 44),
        (0x39, 57),
        (0x3A, 122),
        (0x3B, 120),
        (0x3C, 99),
        (0x3D, 118),
        (0x3E, 96),
        (0x3F, 97),
        (0x40, 98),
        (0x41, 100),
        (0x42, 101),
        (0x43, 109),
        (0x44, 103),
        (0x45, 111),
        (0x46, 105),
        (0x47, 107),
        (0x48, 113),
        (0x49, 114),
        (0x4A, 115),
        (0x4B, 116),
        (0x4C, 117),
        (0x4D, 119),
        (0x4E, 121),
        (0x4F, 124),
        (0x50, 123),
        (0x51, 125),
        (0x52, 126),
        (0x53, 71),
        (0x54, 75),
        (0x55, 67),
        (0x56, 78),
        (0x57, 69),
        (0x58, 76),
        (0x59, 83),
        (0x5A, 84),
        (0x5B, 85),
        (0x5C, 86),
        (0x5D, 87),
        (0x5E, 88),
        (0x5F, 89),
        (0x60, 91),
        (0x61, 92),
        (0x62, 82),
        (0x63, 65),
        (0x64, 10),
        (0x65, 110),
        (0x67, 81),
        (0x68, 105),
        (0x69, 107),
        (0x6A, 113),
        (0x6B, 106),
        (0x6C, 64),
        (0x6D, 79),
        (0x6E, 80),
        (0x6F, 90),
        (0x7F, 74),
        (0x80, 72),
        (0x81, 73),
        (0x85, 95),
        (0x87, 94),
        (0x89, 93),
        (0x90, 104),
        (0x91, 102),
        (0xE0, 59),
        (0xE1, 56),
        (0xE2, 58),
        (0xE3, 55),
        (0xE4, 62),
        (0xE5, 60),
        (0xE6, 61),
        (0xE7, 54),
        (0x75, 114),
        (0x76, 110),
    ];
    let mut i = 0;
    while i < pairs.len() {
        t[pairs[i].0 - 0x04] = pairs[i].1;
        i += 1;
    }
    t
};

const DIGIT_KEYS: [i32; 9] = [18, 19, 20, 21, 23, 22, 26, 28, 25];

#[cold]
fn switch_theme(audio: &Audio, key_code: i32) {
    match key_code {
        123 => audio.step_theme(-1),
        124 => audio.step_theme(1),
        _ => {
            if let Some(i) = DIGIT_KEYS.iter().position(|&k| k == key_code) {
                if i < audio.theme_count() {
                    audio.set_theme(i);
                }
            }
        }
    }
}

type IOHIDManagerRef = *mut c_void;
type IOHIDValueRef = *mut c_void;
type IOHIDElementRef = *mut c_void;
type CFArrayRef = *const c_void;
type CFRunLoopRef = *mut c_void;
type CFStringRef = *const c_void;
type ValueCallback = unsafe extern "C" fn(*mut c_void, i32, *mut c_void, IOHIDValueRef);

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOHIDRequestAccess(request: u32) -> bool;
    fn IOHIDManagerCreate(alloc: *const c_void, options: u32) -> IOHIDManagerRef;
    fn IOHIDManagerSetDeviceMatchingMultiple(manager: IOHIDManagerRef, multiple: CFArrayRef);
    fn IOHIDManagerRegisterInputValueCallback(
        manager: IOHIDManagerRef,
        callback: ValueCallback,
        context: *mut c_void,
    );
    fn IOHIDManagerScheduleWithRunLoop(
        manager: IOHIDManagerRef,
        run_loop: CFRunLoopRef,
        mode: CFStringRef,
    );
    fn IOHIDManagerUnscheduleFromRunLoop(
        manager: IOHIDManagerRef,
        run_loop: CFRunLoopRef,
        mode: CFStringRef,
    );
    fn IOHIDManagerOpen(manager: IOHIDManagerRef, options: u32) -> i32;
    fn IOHIDManagerClose(manager: IOHIDManagerRef, options: u32) -> i32;
    fn IOHIDValueGetElement(value: IOHIDValueRef) -> IOHIDElementRef;
    fn IOHIDValueGetIntegerValue(value: IOHIDValueRef) -> isize;
    fn IOHIDValueGetTimeStamp(value: IOHIDValueRef) -> u64;
    fn IOHIDElementGetUsagePage(element: IOHIDElementRef) -> u32;
    fn IOHIDElementGetUsage(element: IOHIDElementRef) -> u32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopCommonModes: CFStringRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    static kCFTypeArrayCallBacks: c_void;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFStringCreateWithCString(
        alloc: *const c_void,
        s: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFNumberCreate(alloc: *const c_void, ty: isize, value: *const c_void) -> *const c_void;
    fn CFDictionaryCreate(
        alloc: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> *const c_void;
    fn CFArrayCreate(
        alloc: *const c_void,
        values: *const *const c_void,
        count: isize,
        callbacks: *const c_void,
    ) -> CFArrayRef;
    fn CFRelease(cf: *const c_void);
}
