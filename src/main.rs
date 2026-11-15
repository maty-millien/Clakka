mod analyze;
mod audio;
mod cli;
mod input;
mod synth;
mod theme;

use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};

use audio::Audio;
use input::Input;

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn signal_handler(signum: c_int) {
    if signum == SIGINT {
        RUNNING.store(false, Ordering::Relaxed);
    }
}

fn themes_root(dir: &str) -> String {
    let path = std::path::Path::new(dir);
    if path.join("Creamy").is_dir() {
        return dir.to_string();
    }
    path.parent().map_or_else(|| dir.to_string(), |p| p.to_string_lossy().into_owned())
}

fn run(audio: &mut Audio, args: &cli::Args) {
    println!("Kream engine starting...");

    if let Some(dir) = args.value("--theme") {
        if !theme::load_themes(audio, &themes_root(dir)) {
            eprintln!("Warning: Theme loading had issues, continuing with available sounds");
        }
    } else {
        let profile = match cli::load_profile(args) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Error: {e}");
                return;
            }
        };
        if !audio.start_synth(&profile) {
            eprintln!("Error: Failed to start synth audio output");
            return;
        }
        println!("Synth mode at {} Hz", audio.sample_rate());
    }

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

const USAGE: &str = "usage: kream_engine [--profile FILE | --theme DIR]\n\
       kream_engine render [--key CODE] [--n N] [--gap MS] [--hold MS] [--rate HZ] [--seed S] [--press-only] [--out FILE] [--profile FILE]\n\
       kream_engine render --ab [--dir DIR] [--profile FILE]\n\
       kream_engine analyze [--profile FILE]";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let tool = match argv.first().map(String::as_str) {
        Some("render") => Some(cli::render as fn(&cli::Args) -> Result<(), String>),
        Some("analyze") => Some(analyze::run as fn(&cli::Args) -> Result<(), String>),
        Some("-h" | "--help") => {
            println!("{USAGE}");
            return;
        }
        _ => None,
    };
    if let Some(tool) = tool {
        if let Err(e) = tool(&cli::Args::new(argv[1..].to_vec())) {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
        return;
    }
    let args = cli::Args::new(argv);
    let mut audio = Audio::new();
    unsafe { signal(SIGINT, signal_handler) };
    run(&mut audio, &args);
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
