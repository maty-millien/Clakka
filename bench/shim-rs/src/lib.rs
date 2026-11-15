//! Implements bench/shim.h for the Rust engine.
#![allow(dead_code, clippy::missing_safety_doc)]

#[path = "../../../src/audio.rs"]
mod audio;
#[path = "../../../src/input.rs"]
mod input;
#[path = "../../../src/theme.rs"]
mod theme;

use std::ffi::c_void;
use std::sync::atomic::Ordering;

use audio::{Audio, AudioBuffer, AudioBufferList, SoundType};
use input::Input;

static mut AUDIO: *mut Audio = std::ptr::null_mut();
static mut INPUT: *mut Input<'static> = std::ptr::null_mut();

unsafe fn audio() -> &'static mut Audio {
    &mut *AUDIO
}

#[no_mangle]
pub unsafe extern "C" fn kb_init() {
    AUDIO = Box::into_raw(Box::new(Audio::new()));
}

#[no_mangle]
pub unsafe extern "C" fn kb_load_themes() {
    theme::load_themes(audio(), "assets/themes");
    // Input borrows Audio for the rest of the process, like in main.rs.
    INPUT = Box::into_raw(Box::new(Input::new(&*AUDIO)));
}

#[no_mangle]
pub unsafe extern "C" fn kb_stop_audio_unit() {
    let a = audio();
    if !a.audio_unit.is_null() {
        audio::AudioOutputUnitStop(a.audio_unit);
    }
}

#[no_mangle]
pub unsafe extern "C" fn kb_play(key_code: i32, ty: i32, event_time: u64) {
    let ty = if ty == 0 { SoundType::Press } else { SoundType::Release };
    (*AUDIO).play(key_code, ty, event_time);
}

#[no_mangle]
pub unsafe extern "C" fn kb_callback(ty: u32, event: *mut c_void) {
    input::keyboard_callback(std::ptr::null_mut(), ty, event, (*INPUT).state_ptr());
}

#[repr(C)]
struct TwoBufferList {
    number_buffers: u32,
    buffers: [AudioBuffer; 2],
}

#[no_mangle]
pub unsafe extern "C" fn kb_render(frames: u32, left: *mut f32, right: *mut f32) {
    let mut list = TwoBufferList {
        number_buffers: 2,
        buffers: [
            AudioBuffer { number_channels: 1, data_byte_size: frames * 4, data: left.cast() },
            AudioBuffer { number_channels: 1, data_byte_size: frames * 4, data: right.cast() },
        ],
    };
    let mut flags = 0u32;
    audio::render_callback(
        (*AUDIO).engine.as_ptr().cast(),
        &mut flags,
        std::ptr::null(),
        0,
        frames,
        (&raw mut list).cast::<AudioBufferList>(),
    );
}

#[no_mangle]
pub unsafe extern "C" fn kb_input_start() -> i32 {
    match (*INPUT).start() {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// The seq-th play() is visible to the audio thread once its mailbox is non-null.
#[no_mangle]
pub unsafe extern "C" fn kb_poll_published(seq: u64) -> i32 {
    let slots = &(*AUDIO).engine.as_ref().queue.slots;
    !slots[seq as usize % slots.len()].load(Ordering::Relaxed).is_null() as i32
}

/// Consume pending plays as the (stopped) audio thread would.
#[no_mangle]
pub unsafe extern "C" fn kb_ack(_seq: u64) {
    (*AUDIO).engine.as_ref().queue.drain(|_| {});
}
