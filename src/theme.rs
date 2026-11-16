#[derive(Clone, Copy)]
pub struct Group<const N: usize> {
    pub freqs: [f32; N],
    pub t60_ms: [f32; N],
    pub gains_db: [f32; N],
    pub detune: f32,
}

#[derive(Clone, Copy)]
pub struct Low {
    pub modes: Group<4>,
    pub large_freq: f32,
    pub drive_hz: f32,
}

#[derive(Clone, Copy)]
pub struct Plate {
    pub modes: Group<12>,
    pub floor: f32,
}

#[derive(Clone, Copy)]
pub struct Noise {
    pub hp_hz: f32,
    pub lp_hz: f32,
    pub attack_ms: f32,
    pub t60_ms: f32,
    pub level_db: f32,
}

#[derive(Clone, Copy)]
pub struct Hit {
    pub width_ms: [f32; 2],
    pub sec_gap_ms: [f32; 2],
    pub sec_level_db: [f32; 2],
}

#[derive(Clone, Copy)]
pub struct Click {
    pub level_db: f32,
    pub width_ms: [f32; 2],
    pub delay_ms: [f32; 2],
    pub release_delay_ms: [f32; 2],
    pub release_db: f32,
    pub double_ms: [f32; 2],
    pub double_db: f32,
}

#[derive(Clone, Copy)]
pub struct Variation {
    pub pitch: f32,
    pub pitch_clip: f32,
    pub mode_pitch: f32,
    pub mode_gain_db: f32,
    pub level_db: f32,
    pub decay: f32,
    pub hardness: f32,
    pub sec_gap: f32,
    pub sec_level_db: f32,
}

#[derive(Clone, Copy)]
pub struct Velocity {
    pub window_ms: f32,
    pub range_ms: f32,
    pub smoothing: f32,
    pub level_db: f32,
    pub hardness: f32,
    pub gap: f32,
}

#[derive(Clone, Copy)]
pub struct Stab {
    pub min_width: f32,
    pub count: [u32; 2],
    pub delay_ms: [f32; 2],
    pub level_db: [f32; 2],
    pub damping: f32,
    pub low_db: f32,
}

#[derive(Clone, Copy)]
pub struct Release {
    pub hardness: f32,
    pub decay: f32,
    pub low_db: f32,
    pub plate_db: f32,
    pub keycap_db: f32,
    pub switch_db: f32,
    pub noise_db: f32,
    pub level_db: f32,
    pub sec_chance: f32,
    pub sec_gap_ms: [f32; 2],
    pub sec_level_db: f32,
}

#[derive(Clone, Copy)]
pub struct KeyVoice {
    pub code: u32,
    pub gains_db: [f32; 5],
    pub decay: f32,
    pub level_db: f32,
}

pub const KEY_VOICES: usize = 8;
pub const UNUSED: KeyVoice = KeyVoice {
    code: u32::MAX,
    gains_db: [0.0; 5],
    decay: 1.0,
    level_db: 0.0,
};

#[derive(Clone, Copy)]
pub struct Master {
    pub seed: u64,
    pub stereo_width: f32,
    pub size_freq_exp: f32,
    pub size_decay_exp: f32,
    pub key_spread_db: f32,
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub master: Master,
    pub low: Low,
    pub plate: Plate,
    pub keycap: Group<4>,
    pub switch: Group<4>,
    pub noise: Noise,
    pub hit: Hit,
    pub click: Click,
    pub variation: Variation,
    pub velocity: Velocity,
    pub stab: Stab,
    pub release: Release,
    pub keys: [KeyVoice; KEY_VOICES],
}

impl Theme {
    pub const CREAMY: Theme = Theme {
        master: Master {
            seed: 0x4B52_4541_4D00_0001,
            stereo_width: 0.6,
            size_freq_exp: -0.1,
            size_decay_exp: -0.25,
            key_spread_db: 3.0,
        },
        low: Low {
            modes: Group {
                freqs: [110.0, 190.0, 140.0, 260.0],
                t60_ms: [70.0, 55.0, 90.0, 60.0],
                gains_db: [-17.0, -15.0, -23.0, -20.0],
                detune: 0.02,
            },
            large_freq: 105.0,
            drive_hz: 320.0,
        },
        plate: Plate {
            modes: Group {
                freqs: [
                    1383.0, 1431.0, 1539.0, 1749.0, 1841.0, 1991.0, 2045.0, 2088.0, 2147.0, 2217.0,
                    2390.0, 2551.0,
                ],
                t60_ms: [
                    16.0, 32.0, 16.0, 32.0, 16.0, 32.0, 16.0, 32.0, 16.0, 32.0, 16.0, 32.0,
                ],
                gains_db: [
                    -3.6, -5.2, 5.0, -5.2, 1.6, -18.1, 1.6, -18.1, 1.6, -18.1, -2.2, -5.5,
                ],
                detune: 0.01,
            },
            floor: 0.35,
        },
        keycap: Group {
            freqs: [900.0, 1050.0, 1200.0, 1350.0],
            t60_ms: [28.0, 26.0, 24.0, 22.0],
            gains_db: [-26.0, -28.0, -31.0, -34.0],
            detune: 0.06,
        },
        switch: Group {
            freqs: [2700.0, 3300.0, 4300.0, 5400.0],
            t60_ms: [10.0, 9.0, 8.0, 7.0],
            gains_db: [-5.0, -13.0, -17.0, -27.0],
            detune: 0.03,
        },
        noise: Noise {
            hp_hz: 900.0,
            lp_hz: 5000.0,
            attack_ms: 0.3,
            t60_ms: 22.0,
            level_db: -16.0,
        },
        hit: Hit {
            width_ms: [0.4, 0.7],
            sec_gap_ms: [6.0, 13.0],
            sec_level_db: [-20.0, -14.0],
        },
        click: Click {
            level_db: -200.0,
            width_ms: [0.08, 0.14],
            delay_ms: [5.0, 9.0],
            release_delay_ms: [2.0, 4.0],
            release_db: -3.0,
            double_ms: [0.5, 1.2],
            double_db: -9.0,
        },
        variation: Variation {
            pitch: 0.003,
            pitch_clip: 0.008,
            mode_pitch: 0.0015,
            mode_gain_db: 1.0,
            level_db: 1.5,
            decay: 0.04,
            hardness: 0.04,
            sec_gap: 0.15,
            sec_level_db: 3.0,
        },
        velocity: Velocity {
            window_ms: 250.0,
            range_ms: 200.0,
            smoothing: 0.3,
            level_db: 3.0,
            hardness: 0.2,
            gap: 0.3,
        },
        stab: Stab {
            min_width: 2.0,
            count: [1, 3],
            delay_ms: [1.5, 6.0],
            level_db: [-20.0, -12.0],
            damping: 0.7,
            low_db: -5.0,
        },
        release: Release {
            hardness: 1.0,
            decay: 1.0,
            low_db: -12.0,
            plate_db: 6.9,
            keycap_db: 14.1,
            switch_db: -18.8,
            noise_db: 2.5,
            level_db: 0.0,
            sec_chance: 0.5,
            sec_gap_ms: [8.0, 10.0],
            sec_level_db: -16.0,
        },
        keys: [UNUSED; KEY_VOICES],
    };
}

const THEMES_DIR: &str = "themes";

fn title(path: &std::path::Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let mut c = stem.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

trait Field {
    fn parse(&mut self, words: &[&str]) -> Result<(), String>;
}

fn parse_one<T: std::str::FromStr>(w: &str) -> Result<T, String> {
    w.parse().map_err(|_| format!("invalid number '{w}'"))
}

fn one<'a>(words: &[&'a str]) -> Result<&'a str, String> {
    match words {
        [w] => Ok(w),
        _ => Err(format!("expected 1 value, got {}", words.len())),
    }
}

impl Field for f32 {
    fn parse(&mut self, words: &[&str]) -> Result<(), String> {
        *self = parse_one(one(words)?)?;
        Ok(())
    }
}

impl Field for u64 {
    fn parse(&mut self, words: &[&str]) -> Result<(), String> {
        let w = one(words)?;
        *self = match w.strip_prefix("0x") {
            Some(h) => u64::from_str_radix(&h.replace('_', ""), 16)
                .map_err(|_| format!("invalid hex '{w}'"))?,
            None => parse_one(w)?,
        };
        Ok(())
    }
}

impl<T: std::str::FromStr + Copy, const N: usize> Field for [T; N] {
    fn parse(&mut self, words: &[&str]) -> Result<(), String> {
        if words.len() != N {
            return Err(format!("expected {N} values, got {}", words.len()));
        }
        for (v, w) in self.iter_mut().zip(words) {
            *v = parse_one(w)?;
        }
        Ok(())
    }
}

macro_rules! fields {
    ($f:ident; $($sec:literal $base:expr => $($name:ident),+;)+) => {
        $($($f($sec, stringify!($name), &mut $base.$name);)+)+
    };
}

#[rustfmt::skip]
fn visit(p: &mut Theme, f: &mut dyn FnMut(&'static str, &'static str, &mut dyn Field)) {
    fields!(f;
        "master" p.master => seed, stereo_width, size_freq_exp, size_decay_exp, key_spread_db;
        "low" p.low.modes => freqs, t60_ms, gains_db, detune;
        "low" p.low => large_freq, drive_hz;
        "plate" p.plate.modes => freqs, t60_ms, gains_db, detune;
        "plate" p.plate => floor;
        "keycap" p.keycap => freqs, t60_ms, gains_db, detune;
        "switch" p.switch => freqs, t60_ms, gains_db, detune;
        "noise" p.noise => hp_hz, lp_hz, attack_ms, t60_ms, level_db;
        "hit" p.hit => width_ms, sec_gap_ms, sec_level_db;
        "click" p.click => level_db, width_ms, delay_ms, release_delay_ms, release_db, double_ms, double_db;
        "variation" p.variation => pitch, pitch_clip, mode_pitch, mode_gain_db, level_db, decay, hardness, sec_gap, sec_level_db;
        "velocity" p.velocity => window_ms, range_ms, smoothing, level_db, hardness, gap;
        "stab" p.stab => min_width, count, delay_ms, level_db, damping, low_db;
        "release" p.release => hardness, decay, low_db, plate_db, keycap_db, switch_db, noise_db, level_db, sec_chance, sec_gap_ms, sec_level_db;
    );
}

impl Theme {
    pub fn load(path: &str) -> Result<Theme, String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("cannot read theme {path}: {e}"))?;
        Theme::parse(&text).map_err(|e| format!("{path}: {e}"))
    }

    pub fn load_all(extra: Option<&str>) -> Result<(Vec<String>, Vec<Theme>), String> {
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(THEMES_DIR)
            .map(|d| {
                d.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "toml"))
                    .collect()
            })
            .unwrap_or_default();
        files.sort_by_key(|p| (p.file_stem().is_none_or(|s| s != "creamy"), p.clone()));
        if let Some(path) = extra {
            let path = std::path::PathBuf::from(path);
            files.retain(|p| p.canonicalize().ok() != path.canonicalize().ok());
            files.insert(0, path);
        }
        let (mut names, mut themes) = (Vec::new(), Vec::new());
        for path in files {
            themes.push(Theme::load(&path.to_string_lossy())?);
            names.push(title(&path));
        }
        if themes.is_empty() {
            names.push("Creamy".to_string());
            themes.push(Theme::CREAMY);
        }
        Ok((names, themes))
    }

    fn parse(text: &str) -> Result<Theme, String> {
        let mut p = Theme::CREAMY;
        let mut section = String::new();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("line {}: expected key = value", n + 1));
            };
            let (sec, key) = match key.trim().split_once('.') {
                Some((s, k)) => (s.trim().to_string(), k.trim()),
                None => (section.clone(), key.trim()),
            };
            let value = value.trim();
            let value = value
                .strip_prefix('[')
                .and_then(|v| v.strip_suffix(']'))
                .unwrap_or(value);
            let words: Vec<&str> = value
                .split([',', ' ', '\t'])
                .filter(|w| !w.is_empty())
                .collect();
            if sec == "keys" {
                p.set_key(key, &words)
                    .map_err(|e| format!("line {}: keys.{key}: {e}", n + 1))?;
                continue;
            }
            let mut result = Err(format!("line {}: unknown key {sec}.{key}", n + 1));
            visit(&mut p, &mut |s, k, field| {
                if s == sec && k == key {
                    result = field
                        .parse(&words)
                        .map_err(|e| format!("line {}: {sec}.{key}: {e}", n + 1));
                }
            });
            result?;
        }
        Ok(p)
    }

    fn set_key(&mut self, key: &str, words: &[&str]) -> Result<(), String> {
        let code: u32 = key
            .parse()
            .ok()
            .filter(|c| *c < 128)
            .ok_or_else(|| format!("invalid key code '{key}'"))?;
        let mut values = [0.0f32; 7];
        values.parse(words)?;
        let slot = self
            .keys
            .iter()
            .position(|k| k.code == code)
            .or_else(|| self.keys.iter().position(|k| k.code == u32::MAX))
            .ok_or_else(|| format!("at most {KEY_VOICES} keys"))?;
        self.keys[slot] = KeyVoice {
            code,
            gains_db: [values[0], values[1], values[2], values[3], values[4]],
            decay: values[5],
            level_db: values[6],
        };
        Ok(())
    }

    pub fn key_voice(&self, code: u32) -> &KeyVoice {
        self.keys.iter().find(|k| k.code == code).unwrap_or(&UNUSED)
    }
}
