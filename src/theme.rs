//! Theme sound loader.
//!
//! Expected filename format for sound files:
//!
//! Key-specific sounds: `{keyname}_{action}.wav`
//!   - keyname: one of the mapped key names (back, enter, tab, space, shift, ctrl, caps)
//!   - action: "press" or "release" (anything other than "press" counts as release)
//!   - examples: `space_press.wav`, `enter_release.wav`, `tab_press.wav`
//!
//! Fallback sounds: `key{N}_{action}.wav`
//!   - any name starting with "key" that is not a mapped key name
//!   - played round-robin, in directory order, for keys without a specific sound
//!   - examples: `key1_press.wav`, `key3_release.wav`

use std::os::unix::ffi::OsStrExt;

use crate::audio::{Audio, SoundType};

/// Human-readable key names to macOS virtual key codes.
const KEY_CODES: [(&[u8], i32); 7] = [
    (b"back", 51),
    (b"enter", 36),
    (b"tab", 48),
    (b"space", 49),
    (b"shift", 56),
    (b"ctrl", 59),
    (b"caps", 57),
];

/// Loads every theme under `themes_path` into `audio`. Keeps going when a sound
/// fails; returns true only if all sounds loaded and at least one did.
pub fn load_themes(audio: &mut Audio, themes_path: &str) -> bool {
    let theme_dirs = [format!("{themes_path}/Creamy")];

    let mut loaded = 0;
    let mut failed = 0;

    for theme_dir in &theme_dirs {
        let Ok(dir) = std::fs::read_dir(theme_dir) else {
            eprintln!("Warning: Could not open theme directory: {theme_dir}");
            continue;
        };

        for entry in dir.flatten() {
            // Regular files only (d_type, no extra stat): skips directories and symlinks
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let file_name = entry.file_name();
            let name = file_name.as_bytes();
            let Some(stem) = name.strip_suffix(b".wav") else { continue };
            if stem.is_empty() {
                continue;
            }
            // "space_press" -> key name "space", action "press"
            let Some(split) = stem.iter().rposition(|&c| c == b'_') else { continue };
            let (key_name, action) = (&stem[..split], &stem[split + 1..]);
            let ty = if action == b"press" { SoundType::Press } else { SoundType::Release };

            let path = format!("{theme_dir}/{}", String::from_utf8_lossy(name));
            let ok = if let Some(&(_, code)) = KEY_CODES.iter().find(|(k, _)| *k == key_name) {
                audio.load_key_sound(code, ty, &path)
            } else if key_name.starts_with(b"key") {
                audio.load_fallback_sound(ty, &path)
            } else {
                continue; // matches neither pattern: ignored
            };
            if ok {
                loaded += 1;
            } else {
                failed += 1;
            }
        }
    }

    if failed > 0 {
        eprintln!("Warning: Failed to load {failed} sound(s), {loaded} loaded successfully");
    } else if loaded > 0 {
        println!("Loaded {loaded} sound(s) successfully");
    } else {
        eprintln!("Warning: No sounds were loaded");
    }

    failed == 0 && loaded > 0
}
