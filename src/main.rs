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

const PROFILES_DIR: &str = "assets/profiles";
const THEMES_DIR: &str = "assets/themes";

fn title(stem: &str) -> String {
    let mut c = stem.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

fn load_profiles(extra: Option<&str>) -> Result<(Vec<String>, Vec<synth::profile::Profile>), String> {
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(PROFILES_DIR)
        .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "kprofile")).collect())
        .unwrap_or_default();
    files.sort_by_key(|p| (p.file_stem().is_none_or(|s| s != "creamy"), p.clone()));
    if let Some(path) = extra {
        let path = std::path::PathBuf::from(path);
        files.retain(|p| p.canonicalize().ok() != path.canonicalize().ok());
        files.insert(0, path);
    }
    let (mut names, mut profiles) = (Vec::new(), Vec::new());
    for path in files {
        let stem = path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let profile = synth::profile::Profile::load(&path.to_string_lossy())?;
        names.push(title(&stem));
        profiles.push(profile);
    }
    if profiles.is_empty() {
        names.push("Creamy".to_string());
        profiles.push(synth::profile::Profile::CREAMY);
    }
    Ok((names, profiles))
}

fn run(audio: &mut Audio, args: &cli::Args) {
    println!("Kream engine starting...");

    let (mut names, profiles) = match load_profiles(args.value("--profile")) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {e}");
            return;
        }
    };
    if !audio.start_synth(&profiles) {
        eprintln!("Error: Failed to start audio output");
        return;
    }
    let themes_dir = args.value("--theme").map_or_else(|| THEMES_DIR.to_string(), themes_root);
    if std::path::Path::new(&themes_dir).join("Creamy").is_dir() {
        if !theme::load_themes(audio, &themes_dir) {
            eprintln!("Warning: Theme loading had issues, continuing with available sounds");
        }
        if audio.theme_count() > profiles.len() {
            names.push("Creamy (samples)".to_string());
        }
    }
    if args.value("--theme").is_some() {
        audio.set_theme(profiles.len());
    }

    let audio: &Audio = audio;
    let mut input = Input::new(audio);
    if let Err(e) = input.start() {
        eprintln!("Error: {e}");
        return;
    }

    println!("Kream engine started. Press keys... (Ctrl+C to quit)");
    println!("Themes: {}", names.iter().enumerate().map(|(i, n)| format!("{}. {n}", i + 1)).collect::<Vec<_>>().join("  "));
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
    println!("Kream engine stopped.");
}

const USAGE: &str = "usage: kream_engine [--profile FILE] [--theme DIR]\n\
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
