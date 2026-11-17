mod audio;
mod input;
mod render;
mod synth;
mod theme;

use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};

use audio::Audio;
use input::Input;
use theme::Theme;

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn signal_handler(signum: c_int) {
    if signum == SIGINT {
        RUNNING.store(false, Ordering::Relaxed);
    }
}

fn run(audio: &mut Audio, args: &render::Args) {
    println!("Clakka engine starting...");

    let (names, themes) = match Theme::load_all(args.value("--theme")) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {e}");
            return;
        }
    };
    if let Err(e) = audio.start(&themes) {
        eprintln!("Error: {e}");
        return;
    }
    unsafe { pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) };
    let mut input = Input::new(audio);
    if let Err(e) = input.start() {
        eprintln!("Error: {e}");
        return;
    }

    println!("Clakka engine started. Press keys... (Ctrl+C to quit)");
    println!(
        "Themes: {}",
        names
            .iter()
            .enumerate()
            .map(|(i, n)| format!("{}. {n}", i + 1))
            .collect::<Vec<_>>()
            .join("  ")
    );
    println!("Switch theme: Ctrl+Option+Cmd + Left/Right, or + 1-9");

    let mut shown = usize::MAX;
    while RUNNING.load(Ordering::Relaxed) {
        let current = audio.theme();
        if current != shown {
            shown = current;
            println!("Theme: {}", names.get(current).map_or("?", String::as_str));
        }
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, true) };
    }

    input.stop();
    println!("Clakka engine stopped.");
}

const USAGE: &str = "usage: clakka_engine [--theme FILE]\n\
       clakka_engine render [--key CODE] [--n N] [--gap MS] [--hold MS] [--rate HZ] [--seed S] [--press-only] [--out FILE] [--theme FILE]\n\
       clakka_engine render --demo [--dir DIR] [--theme FILE]";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match argv.first().map(String::as_str) {
        Some("render") => {
            if let Err(e) = render::render(&render::Args(argv[1..].to_vec())) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Some("-h" | "--help") => println!("{USAGE}"),
        _ => {
            let mut audio = Audio::new();
            unsafe { signal(SIGINT, signal_handler) };
            run(&mut audio, &render::Args(argv));
        }
    }
}

const SIGINT: c_int = 2;
const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;

extern "C" {
    fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: c_int) -> c_int;
    fn signal(sig: c_int, handler: extern "C" fn(c_int)) -> *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopDefaultMode: *const c_void;
    fn CFRunLoopRunInMode(
        mode: *const c_void,
        seconds: f64,
        return_after_source_handled: bool,
    ) -> i32;
}
