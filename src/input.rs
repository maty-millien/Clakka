use std::cell::Cell;
use std::ffi::c_void;
use std::ptr;

use crate::audio::{Audio, SoundType};

const MODIFIER_MASKS: [u64; 5] = [
    0x0002_0000,
    0x0004_0000,
    0x0008_0000,
    0x0010_0000,
    0x0080_0000,
];
const FLAG_ALPHA_SHIFT: u64 = 0x0001_0000;

const CAPS_RELEASE_DELAY_S: f64 = 0.1;

pub(crate) const EVENT_KEY_DOWN: u32 = 10;
pub(crate) const EVENT_KEY_UP: u32 = 11;
pub(crate) const EVENT_FLAGS_CHANGED: u32 = 12;
const EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
const FIELD_KEYCODE: u32 = 9;
const FIELD_AUTOREPEAT: u32 = 8;

pub(crate) struct TapState<'a> {
    audio: &'a Audio,
    last_flags: Cell<Option<u64>>,
    caps_key_code: Cell<i32>,
    tap: Cell<CFMachPortRef>,
}

pub struct Input<'a> {
    pub(crate) state: Box<TapState<'a>>,
    run_loop_source: CFRunLoopSourceRef,
}

#[derive(Debug)]
pub struct TapError;

impl std::fmt::Display for TapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to create event tap. Check Accessibility permissions.")
    }
}

impl<'a> Input<'a> {
    pub fn new(audio: &'a Audio) -> Self {
        Input {
            state: Box::new(TapState {
                audio,
                last_flags: Cell::new(None),
                caps_key_code: Cell::new(0),
                tap: Cell::new(ptr::null_mut()),
            }),
            run_loop_source: ptr::null_mut(),
        }
    }

    pub fn start(&mut self) -> Result<(), TapError> {
        let mask = (1u64 << EVENT_KEY_DOWN) | (1u64 << EVENT_KEY_UP) | (1u64 << EVENT_FLAGS_CHANGED);
        unsafe {
            let tap = CGEventTapCreate(
                1,
                0,
                0,
                mask,
                keyboard_callback,
                self.state_ptr(),
            );
            if tap.is_null() {
                return Err(TapError);
            }
            self.state.tap.set(tap);
            self.run_loop_source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
            CFRunLoopAddSource(CFRunLoopGetCurrent(), self.run_loop_source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        let tap = self.state.tap.replace(ptr::null_mut());
        if tap.is_null() {
            return;
        }
        unsafe {
            CGEventTapEnable(tap, false);
            CFRunLoopRemoveSource(CFRunLoopGetCurrent(), self.run_loop_source, kCFRunLoopCommonModes);
            CFRelease(self.run_loop_source);
            CFRelease(tap);
        }
        self.run_loop_source = ptr::null_mut();
    }

    pub(crate) fn state_ptr(&self) -> *mut c_void {
        (&*self.state as *const TapState).cast_mut().cast()
    }
}

impl Drop for Input<'_> {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) unsafe extern "C" fn keyboard_callback(
    _proxy: *mut c_void,
    ty: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    let event_time = mach_absolute_time();
    let state = &*(user_info as *const TapState);

    match ty {
        EVENT_FLAGS_CHANGED => {
            let key_code = CGEventGetIntegerValueField(event, FIELD_KEYCODE) as i32;
            let flags = CGEventGetFlags(event);
            let last = state.last_flags.get().unwrap_or(flags ^ modifier_mask(key_code));
            let changed = flags ^ last;

            for mask in MODIFIER_MASKS {
                if changed & mask != 0 {
                    let ty = if flags & mask != 0 { SoundType::Press } else { SoundType::Release };
                    state.audio.play(key_code, ty, event_time);
                }
            }

            if changed & FLAG_ALPHA_SHIFT != 0 {
                state.audio.play(key_code, SoundType::Press, event_time);
                state.caps_key_code.set(key_code);
                schedule_caps_release(user_info);
            }

            state.last_flags.set(Some(flags));
        }
        EVENT_KEY_DOWN => {
            init_flags(state, event);
            if CGEventGetIntegerValueField(event, FIELD_AUTOREPEAT) == 0 {
                let key_code = CGEventGetIntegerValueField(event, FIELD_KEYCODE) as i32;
                state.audio.play(key_code, SoundType::Press, event_time);
            }
        }
        EVENT_KEY_UP => {
            init_flags(state, event);
            let key_code = CGEventGetIntegerValueField(event, FIELD_KEYCODE) as i32;
            state.audio.play(key_code, SoundType::Release, event_time);
        }
        EVENT_TAP_DISABLED_BY_TIMEOUT | EVENT_TAP_DISABLED_BY_USER_INPUT => {
            let tap = state.tap.get();
            if !tap.is_null() {
                CGEventTapEnable(tap, true);
            }
        }
        _ => {}
    }
    event
}

#[inline]
unsafe fn init_flags(state: &TapState, event: CGEventRef) {
    if state.last_flags.get().is_none() {
        state.last_flags.set(Some(CGEventGetFlags(event)));
    }
}

fn modifier_mask(key_code: i32) -> u64 {
    match key_code {
        56 | 60 => MODIFIER_MASKS[0],
        59 | 62 => MODIFIER_MASKS[1],
        58 | 61 => MODIFIER_MASKS[2],
        54 | 55 => MODIFIER_MASKS[3],
        63 => MODIFIER_MASKS[4],
        57 => FLAG_ALPHA_SHIFT,
        _ => 0,
    }
}

unsafe fn schedule_caps_release(info: *mut c_void) {
    let mut ctx = CFRunLoopTimerContext {
        version: 0,
        info,
        retain: ptr::null(),
        release: ptr::null(),
        copy_description: ptr::null(),
    };
    let timer = CFRunLoopTimerCreate(
        ptr::null(),
        CFAbsoluteTimeGetCurrent() + CAPS_RELEASE_DELAY_S,
        0.0,
        0,
        0,
        caps_release,
        &mut ctx,
    );
    if !timer.is_null() {
        CFRunLoopAddTimer(CFRunLoopGetCurrent(), timer, kCFRunLoopCommonModes);
        CFRelease(timer);
    }
}

unsafe extern "C" fn caps_release(_timer: *mut c_void, info: *mut c_void) {
    let state = &*(info as *const TapState);
    state.audio.play(state.caps_key_code.get(), SoundType::Release, mach_absolute_time());
}

pub(crate) type CGEventRef = *mut c_void;
type CFMachPortRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CFStringRef = *const c_void;
type TapCallback = unsafe extern "C" fn(*mut c_void, u32, CGEventRef, *mut c_void) -> CGEventRef;
type TimerCallback = unsafe extern "C" fn(*mut c_void, *mut c_void);

#[repr(C)]
struct CFRunLoopTimerContext {
    version: isize,
    info: *mut c_void,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: TapCallback,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetFlags(event: CGEventRef) -> u64;
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopCommonModes: CFStringRef;
    fn CFMachPortCreateRunLoopSource(alloc: *const c_void, port: CFMachPortRef, order: isize) -> CFRunLoopSourceRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRemoveSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopTimerCreate(
        alloc: *const c_void,
        fire_date: f64,
        interval: f64,
        flags: usize,
        order: isize,
        callout: TimerCallback,
        context: *mut CFRunLoopTimerContext,
    ) -> *mut c_void;
    fn CFRunLoopAddTimer(rl: CFRunLoopRef, timer: *mut c_void, mode: CFStringRef);
    fn CFAbsoluteTimeGetCurrent() -> f64;
    fn CFRelease(cf: *const c_void);
}

extern "C" {
    fn mach_absolute_time() -> u64;
}
