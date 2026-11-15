#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Group<const N: usize> {
    pub freqs: [f32; N],
    pub t60_ms: [f32; N],
    pub gains_db: [f32; N],
    pub detune: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Low {
    pub modes: Group<4>,
    pub large_freq: f32,
    pub drive_hz: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plate {
    pub modes: Group<12>,
    pub floor: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Noise {
    pub hp_hz: f32,
    pub lp_hz: f32,
    pub attack_ms: f32,
    pub t60_ms: f32,
    pub level_db: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub width_ms: [f32; 2],
    pub sec_gap_ms: [f32; 2],
    pub sec_level_db: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Velocity {
    pub window_ms: f32,
    pub range_ms: f32,
    pub smoothing: f32,
    pub level_db: f32,
    pub hardness: f32,
    pub gap: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stab {
    pub min_width: f32,
    pub count: [u32; 2],
    pub delay_ms: [f32; 2],
    pub level_db: [f32; 2],
    pub damping: f32,
    pub low_db: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyVoice {
    pub code: u32,
    pub gains_db: [f32; 5],
    pub decay: f32,
    pub level_db: f32,
}

pub const KEY_VOICES: usize = 8;
pub const UNUSED: KeyVoice = KeyVoice { code: u32::MAX, gains_db: [0.0; 5], decay: 1.0, level_db: 0.0 };

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Master {
    pub seed: u64,
    pub peak_db: f32,
    pub stereo_width: f32,
    pub size_freq_exp: f32,
    pub size_decay_exp: f32,
    pub key_spread_db: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub master: Master,
    pub low: Low,
    pub plate: Plate,
    pub keycap: Group<4>,
    pub switch: Group<4>,
    pub noise: Noise,
    pub hit: Hit,
    pub variation: Variation,
    pub velocity: Velocity,
    pub stab: Stab,
    pub release: Release,
    pub keys: [KeyVoice; KEY_VOICES],
}

impl Profile {
    pub const CREAMY: Profile = Profile {
        master: Master {
            seed: 0x4B52_4541_4D00_0001,
            peak_db: -6.0,
            stereo_width: 0.15,
            size_freq_exp: -0.1,
            size_decay_exp: -0.25,
            key_spread_db: 3.0,
        },
        low: Low {
            modes: Group {
                freqs: [110.0, 190.0, 140.0, 260.0],
                t60_ms: [70.0, 55.0, 90.0, 60.0],
                gains_db: [-14.0, -12.0, -20.0, -17.0],
                detune: 0.02,
            },
            large_freq: 105.0,
            drive_hz: 320.0,
        },
        plate: Plate {
            modes: Group {
                freqs: [1383.0, 1431.0, 1539.0, 1749.0, 1841.0, 1991.0, 2045.0, 2088.0, 2147.0, 2217.0, 2390.0, 2551.0],
                t60_ms: [16.0, 32.0, 16.0, 32.0, 16.0, 32.0, 16.0, 32.0, 16.0, 32.0, 16.0, 32.0],
                gains_db: [-8.6, -10.2, 0.0, -10.2, -3.4, -23.1, -3.4, -23.1, -3.4, -23.1, -7.2, -10.5],
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
            gains_db: [-10.0, -18.0, -22.0, -32.0],
            detune: 0.03,
        },
        noise: Noise { hp_hz: 900.0, lp_hz: 5000.0, attack_ms: 0.3, t60_ms: 22.0, level_db: -20.0 },
        hit: Hit { width_ms: [0.4, 0.7], sec_gap_ms: [6.0, 13.0], sec_level_db: [-20.0, -14.0] },
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
            low_db: 0.0,
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
        keys: [
            KeyVoice { code: 51, gains_db: [-15.7, 3.7, 1.8, -3.6, 1.8], decay: 1.09, level_db: 1.3 },
            KeyVoice { code: 36, gains_db: [-10.7, -6.3, 0.8, 2.5, 1.7], decay: 0.81, level_db: 1.3 },
            KeyVoice { code: 48, gains_db: [2.6, 1.2, -13.6, 11.2, -1.4], decay: 1.14, level_db: 1.3 },
            KeyVoice { code: 49, gains_db: [-12.5, 6.0, 1.6, -1.1, -6.0], decay: 1.12, level_db: 1.3 },
            KeyVoice { code: 56, gains_db: [-12.8, -9.0, 1.2, 8.5, 0.1], decay: 1.11, level_db: 1.3 },
            KeyVoice { code: 59, gains_db: [-0.9, 7.8, 5.2, -17.1, 5.0], decay: 0.96, level_db: 1.3 },
            KeyVoice { code: 57, gains_db: [7.9, -5.0, 7.9, -17.1, 6.3], decay: 1.18, level_db: 1.3 },
            KeyVoice { code: 60, gains_db: [-12.8, -9.0, 1.2, 8.5, 0.1], decay: 1.11, level_db: 1.3 },
        ],
    };
}

trait Field {
    fn parse(&mut self, words: &[&str]) -> Result<(), String>;
    fn show(&self) -> String;
}

fn parse_one<T: std::str::FromStr>(w: &str) -> Result<T, String> {
    w.parse().map_err(|_| format!("invalid number '{w}'"))
}

impl Field for f32 {
    fn parse(&mut self, words: &[&str]) -> Result<(), String> {
        match words {
            [w] => {
                *self = parse_one(w)?;
                Ok(())
            }
            _ => Err(format!("expected 1 value, got {}", words.len())),
        }
    }
    fn show(&self) -> String {
        format!("{self}")
    }
}

impl Field for u64 {
    fn parse(&mut self, words: &[&str]) -> Result<(), String> {
        match words {
            [w] => {
                *self = match w.strip_prefix("0x") {
                    Some(h) => u64::from_str_radix(&h.replace('_', ""), 16).map_err(|_| format!("invalid hex '{w}'"))?,
                    None => parse_one(w)?,
                };
                Ok(())
            }
            _ => Err(format!("expected 1 value, got {}", words.len())),
        }
    }
    fn show(&self) -> String {
        format!("0x{self:016X}")
    }
}

impl<T: std::str::FromStr + std::fmt::Display + Copy, const N: usize> Field for [T; N] {
    fn parse(&mut self, words: &[&str]) -> Result<(), String> {
        if words.len() != N {
            return Err(format!("expected {N} values, got {}", words.len()));
        }
        for (v, w) in self.iter_mut().zip(words) {
            *v = parse_one(w)?;
        }
        Ok(())
    }
    fn show(&self) -> String {
        self.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(" ")
    }
}

fn group<const N: usize>(s: &'static str, g: &mut Group<N>, f: &mut dyn FnMut(&'static str, &'static str, &mut dyn Field)) {
    f(s, "freqs", &mut g.freqs);
    f(s, "t60_ms", &mut g.t60_ms);
    f(s, "gains_db", &mut g.gains_db);
    f(s, "detune", &mut g.detune);
}

fn visit(p: &mut Profile, f: &mut dyn FnMut(&'static str, &'static str, &mut dyn Field)) {
    let m = &mut p.master;
    f("master", "seed", &mut m.seed);
    f("master", "peak_db", &mut m.peak_db);
    f("master", "stereo_width", &mut m.stereo_width);
    f("master", "size_freq_exp", &mut m.size_freq_exp);
    f("master", "size_decay_exp", &mut m.size_decay_exp);
    f("master", "key_spread_db", &mut m.key_spread_db);
    group("low", &mut p.low.modes, f);
    f("low", "large_freq", &mut p.low.large_freq);
    f("low", "drive_hz", &mut p.low.drive_hz);
    group("plate", &mut p.plate.modes, f);
    f("plate", "floor", &mut p.plate.floor);
    group("keycap", &mut p.keycap, f);
    group("switch", &mut p.switch, f);
    let n = &mut p.noise;
    f("noise", "hp_hz", &mut n.hp_hz);
    f("noise", "lp_hz", &mut n.lp_hz);
    f("noise", "attack_ms", &mut n.attack_ms);
    f("noise", "t60_ms", &mut n.t60_ms);
    f("noise", "level_db", &mut n.level_db);
    let h = &mut p.hit;
    f("hit", "width_ms", &mut h.width_ms);
    f("hit", "sec_gap_ms", &mut h.sec_gap_ms);
    f("hit", "sec_level_db", &mut h.sec_level_db);
    let v = &mut p.variation;
    f("variation", "pitch", &mut v.pitch);
    f("variation", "pitch_clip", &mut v.pitch_clip);
    f("variation", "mode_pitch", &mut v.mode_pitch);
    f("variation", "mode_gain_db", &mut v.mode_gain_db);
    f("variation", "level_db", &mut v.level_db);
    f("variation", "decay", &mut v.decay);
    f("variation", "hardness", &mut v.hardness);
    f("variation", "sec_gap", &mut v.sec_gap);
    f("variation", "sec_level_db", &mut v.sec_level_db);
    let v = &mut p.velocity;
    f("velocity", "window_ms", &mut v.window_ms);
    f("velocity", "range_ms", &mut v.range_ms);
    f("velocity", "smoothing", &mut v.smoothing);
    f("velocity", "level_db", &mut v.level_db);
    f("velocity", "hardness", &mut v.hardness);
    f("velocity", "gap", &mut v.gap);
    let s = &mut p.stab;
    f("stab", "min_width", &mut s.min_width);
    f("stab", "count", &mut s.count);
    f("stab", "delay_ms", &mut s.delay_ms);
    f("stab", "level_db", &mut s.level_db);
    f("stab", "damping", &mut s.damping);
    f("stab", "low_db", &mut s.low_db);
    let r = &mut p.release;
    f("release", "hardness", &mut r.hardness);
    f("release", "decay", &mut r.decay);
    f("release", "low_db", &mut r.low_db);
    f("release", "plate_db", &mut r.plate_db);
    f("release", "keycap_db", &mut r.keycap_db);
    f("release", "switch_db", &mut r.switch_db);
    f("release", "noise_db", &mut r.noise_db);
    f("release", "level_db", &mut r.level_db);
    f("release", "sec_chance", &mut r.sec_chance);
    f("release", "sec_gap_ms", &mut r.sec_gap_ms);
    f("release", "sec_level_db", &mut r.sec_level_db);
}

impl Profile {
    pub fn load(path: &str) -> Result<Profile, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read profile {path}: {e}"))?;
        Profile::parse(&text).map_err(|e| format!("{path}: {e}"))
    }

    pub fn parse(text: &str) -> Result<Profile, String> {
        let mut p = Profile::CREAMY;
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
            let words: Vec<&str> = value.split_whitespace().collect();
            if sec == "keys" {
                p.set_key(key, &words).map_err(|e| format!("line {}: keys.{key}: {e}", n + 1))?;
                continue;
            }
            let mut result = Err(format!("line {}: unknown key {sec}.{key}", n + 1));
            visit(&mut p, &mut |s, k, field| {
                if s == sec && k == key {
                    result = field.parse(&words).map_err(|e| format!("line {}: {sec}.{key}: {e}", n + 1));
                }
            });
            result?;
        }
        Ok(p)
    }

    pub fn to_text(self) -> String {
        let mut p = self;
        let mut out = String::new();
        let mut current = "";
        visit(&mut p, &mut |s, k, field| {
            if s != current {
                out.push_str(&format!("{}[{s}]\n", if current.is_empty() { "" } else { "\n" }));
                current = s;
            }
            out.push_str(&format!("{k} = {}\n", field.show()));
        });
        let used: Vec<&KeyVoice> = self.keys.iter().filter(|k| k.code != u32::MAX).collect();
        if !used.is_empty() {
            out.push_str("\n[keys]\n");
            for k in used {
                out.push_str(&format!("{} = {} {} {}\n", k.code, k.gains_db.show(), k.decay, k.level_db));
            }
        }
        out
    }

    fn set_key(&mut self, key: &str, words: &[&str]) -> Result<(), String> {
        let code: u32 = key.parse().ok().filter(|c| *c < 128).ok_or_else(|| format!("invalid key code '{key}'"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trip() {
        let text = Profile::CREAMY.to_text();
        assert_eq!(Profile::parse(&text).unwrap(), Profile::CREAMY);
    }

    #[test]
    fn bundled_creamy_matches_builtin() {
        let text = include_str!("../../assets/profiles/creamy.kprofile");
        assert_eq!(Profile::parse(text).unwrap(), Profile::CREAMY);
    }

    #[test]
    fn parse_sections_and_dotted_keys() {
        let p = Profile::parse(
            "# comment\n[noise]\nlevel_db = -20 # trailing\nplate.freqs = 1 2 3 4 5 6 7 8 9 10 11 12\n[master]\nseed = 0x10\n",
        )
        .unwrap();
        assert_eq!(p.noise.level_db, -20.0);
        assert_eq!(p.plate.modes.freqs[11], 12.0);
        assert_eq!(p.master.seed, 16);
        assert_eq!(p.keycap, Profile::CREAMY.keycap);
    }

    #[test]
    fn key_voices() {
        let p = Profile::parse("[keys]\n49 = -6 1 2 3 4 0.8 0\n36 = 0 0 0 0 0 1 2\n49 = 1 1 1 1 1 1 1\n").unwrap();
        assert_eq!(p.key_voice(49).gains_db, [1.0; 5]);
        assert_eq!(p.key_voice(36).decay, 1.0);
        assert_eq!(p.key_voice(5), &UNUSED);
        assert_eq!(Profile::parse(&p.to_text()).unwrap(), p);
        assert!(Profile::parse("[keys]\n200 = 0 0 0 0 0 1 0").is_err());
        assert!(Profile::parse("[keys]\n1 = 0 0 0").is_err());
    }

    #[test]
    fn parse_errors() {
        assert!(Profile::parse("[plate]\nfreqs = 1 2 3").unwrap_err().contains("expected 12 values"));
        assert!(Profile::parse("[noise]\nbogus = 1").unwrap_err().contains("unknown key"));
        assert!(Profile::parse("[noise]\nlevel_db = x").unwrap_err().contains("invalid number"));
        assert!(Profile::parse("level_db -3").unwrap_err().contains("key = value"));
    }
}
