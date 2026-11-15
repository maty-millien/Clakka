mod audio;
mod input;
mod theme;

use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};

use audio::Audio;
use input::Input;

static RUNNING: AtomicBool = AtomicBool::new(true);

/// Only stores a flag (async-signal-safe); the run loop polls it every 100 ms.
extern "C" fn signal_handler(signum: c_int) {
    if signum == SIGINT {
        RUNNING.store(false, Ordering::Relaxed);
    }
}

fn run(audio: &mut Audio) {
    println!("Kream engine starting...");

    // Load theme sounds - continue even if some sounds fail to load
    if !theme::load_themes(audio, "assets/themes") {
        eprintln!("Warning: Theme loading had issues, continuing with available sounds");
    }

    // Start input capture - this fails if permissions are not granted
    let mut input = Input::new(audio);
    if let Err(e) = input.start() {
        eprintln!("Error: {e}");
        return;
    }

    println!("Kream engine started. Press keys... (Ctrl+C to quit)");

    while RUNNING.load(Ordering::Relaxed) {
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, true) };
    }

    input.stop();
    println!("Kream engine stopped.");
}

fn main() {
    let mut audio = Audio::new();
    unsafe { signal(SIGINT, signal_handler) };
    run(&mut audio);
}

const SIGINT: c_int = 2;

extern "C" {
    fn signal(sig: c_int, handler: extern "C" fn(c_int)) -> *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopDefaultMode: *const c_void;
    fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source_handled: bool) -> i32;
}
