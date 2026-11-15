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
const FLAG_SECONDARY_FN: u64 = MODIFIER_MASKS[4];
const TRACKED_FLAGS: u64 = MODIFIER_MASKS[0]
    | MODIFIER_MASKS[1]
    | MODIFIER_MASKS[2]
    | MODIFIER_MASKS[3]
    | MODIFIER_MASKS[4]
    | FLAG_ALPHA_SHIFT;

const CAPS_RELEASE_DELAY_S: f64 = 0.1;
const TIMER_IDLE_S: f64 = 1e9;

const EVENT_KEY_DOWN: u32 = 10;
const EVENT_KEY_UP: u32 = 11;
const EVENT_FLAGS_CHANGED: u32 = 12;
const EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
const FIELD_KEYCODE: u32 = 9;
const FIELD_AUTOREPEAT: u32 = 8;

struct TapState<'a> {
    audio: &'a Audio,
    last_flags: Cell<Option<u64>>,
    caps_key_code: Cell<i32>,
    caps_pending: Cell<bool>,
    caps_timer: Cell<CFRunLoopTimerRef>,
    tap: Cell<CFMachPortRef>,
}

pub struct Input<'a> {
    state: Box<TapState<'a>>,
    run_loop_source: CFRunLoopSourceRef,
}

#[derive(Debug)]
pub struct TapError;

impl std::fmt::Display for TapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to create event tap. Check Input Monitoring permissions.")
    }
}

impl<'a> Input<'a> {
    pub fn new(audio: &'a Audio) -> Self {
        Input {
            state: Box::new(TapState {
                audio,
                last_flags: Cell::new(None),
                caps_key_code: Cell::new(0),
                caps_pending: Cell::new(false),
                caps_timer: Cell::new(ptr::null_mut()),
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
                1,
                mask,
                keyboard_callback,
                self.state_ptr(),
            );
            if tap.is_null() {
                return Err(TapError);
            }
            self.state.tap.set(tap);
            let mut ctx = CFRunLoopTimerContext {
                version: 0,
                info: self.state_ptr(),
                retain: ptr::null(),
                release: ptr::null(),
                copy_description: ptr::null(),
            };
            let timer = CFRunLoopTimerCreate(
                ptr::null(),
                CFAbsoluteTimeGetCurrent() + TIMER_IDLE_S,
                TIMER_IDLE_S,
                0,
                0,
                caps_release,
                &mut ctx,
            );
            if !timer.is_null() {
                CFRunLoopAddTimer(CFRunLoopGetCurrent(), timer, kCFRunLoopCommonModes);
            }
            self.state.caps_timer.set(timer);
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
            let timer = self.state.caps_timer.replace(ptr::null_mut());
            if !timer.is_null() {
                CFRunLoopTimerInvalidate(timer);
                CFRelease(timer);
            }
        }
        self.state.caps_pending.set(false);
        self.run_loop_source = ptr::null_mut();
    }

    fn state_ptr(&self) -> *mut c_void {
        (&*self.state as *const TapState).cast_mut().cast()
    }
}

impl Drop for Input<'_> {
    fn drop(&mut self) {
        self.stop();
    }
}

unsafe extern "C" fn keyboard_callback(
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
            let flags = CGEventGetFlags(event) & TRACKED_FLAGS;
            let last = state.last_flags.get().unwrap_or(flags ^ modifier_mask(key_code));
            let changed = flags ^ last;

            for mask in MODIFIER_MASKS {
                if changed & mask != 0 {
                    let ty = if flags & mask != 0 { SoundType::Press } else { SoundType::Release };
                    state.audio.play(key_code, ty, event_time);
                }
            }

            if changed & FLAG_ALPHA_SHIFT != 0 {
                let timer = state.caps_timer.get();
                if !timer.is_null() {
                    if state.caps_pending.replace(true) {
                        state.audio.play(state.caps_key_code.get(), SoundType::Release, event_time);
                    }
                    CFRunLoopTimerSetNextFireDate(timer, CFAbsoluteTimeGetCurrent() + CAPS_RELEASE_DELAY_S);
                }
                state.audio.play(key_code, SoundType::Press, event_time);
                state.caps_key_code.set(key_code);
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
        state.last_flags.set(Some(CGEventGetFlags(event) & TRACKED_FLAGS & !FLAG_SECONDARY_FN));
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

unsafe extern "C" fn caps_release(_timer: CFRunLoopTimerRef, info: *mut c_void) {
    let state = &*(info as *const TapState);
    if state.caps_pending.replace(false) {
        state.audio.play(state.caps_key_code.get(), SoundType::Release, mach_absolute_time());
    }
}

type CGEventRef = *mut c_void;
type CFMachPortRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CFRunLoopTimerRef = *mut c_void;
type CFStringRef = *const c_void;
type TapCallback = unsafe extern "C" fn(*mut c_void, u32, CGEventRef, *mut c_void) -> CGEventRef;
type TimerCallback = unsafe extern "C" fn(CFRunLoopTimerRef, *mut c_void);

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
    ) -> CFRunLoopTimerRef;
    fn CFRunLoopAddTimer(rl: CFRunLoopRef, timer: CFRunLoopTimerRef, mode: CFStringRef);
    fn CFRunLoopTimerSetNextFireDate(timer: CFRunLoopTimerRef, fire_date: f64);
    fn CFRunLoopTimerInvalidate(timer: CFRunLoopTimerRef);
    fn CFAbsoluteTimeGetCurrent() -> f64;
    fn CFRelease(cf: *const c_void);
}

extern "C" {
    fn mach_absolute_time() -> u64;
}
