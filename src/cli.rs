use crate::audio::{Audio, SoundType};
use crate::synth::rng::Xorshift32;
use crate::synth::theme::Theme;
use crate::synth::wav_out;

const BLOCK: usize = 128;

#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub t_ms: f64,
    pub key: i32,
    pub ty: SoundType,
}

pub struct Args(Vec<String>);

impl Args {
    pub fn new(v: Vec<String>) -> Self {
        Args(v)
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .position(|a| a == name)
            .and_then(|i| self.0.get(i + 1))
            .map(String::as_str)
    }

    pub fn flag(&self, name: &str) -> bool {
        self.0.iter().any(|a| a == name)
    }

    pub fn num<T: std::str::FromStr>(&self, name: &str, default: T) -> Result<T, String> {
        match self.value(name) {
            None => Ok(default),
            Some(v) => v
                .parse()
                .map_err(|_| format!("invalid value for {name}: {v}")),
        }
    }
}

pub fn load_theme(args: &Args) -> Result<Theme, String> {
    match args.value("--theme") {
        Some(path) => Theme::load(path),
        None => Ok(Theme::CREAMY),
    }
}

pub fn render_synth(
    theme: &Theme,
    rate: f64,
    events: &[Event],
    total_ms: f64,
    seed: u64,
) -> (Vec<f32>, Vec<f32>) {
    let mut audio = Audio::new();
    audio.open_offline(theme, rate);
    let frames = (total_ms * rate / 1000.0).ceil() as usize;
    let (mut l, mut r) = (vec![0.0f32; frames], vec![0.0f32; frames]);
    let base = audio
        .ms_to_ticks(1000.0)
        .wrapping_add(seed.wrapping_mul(7919));
    let mut next = 0;
    let mut pos = 0;
    while pos < frames {
        let n = BLOCK.min(frames - pos);
        while next < events.len() && events[next].t_ms * rate / 1000.0 <= pos as f64 {
            let e = events[next];
            audio.play_at(e.key, e.ty, base + audio.ms_to_ticks(e.t_ms));
            next += 1;
        }
        audio.render_offline(&mut l[pos..pos + n], &mut r[pos..pos + n]);
        pos += n;
    }
    (l, r)
}

pub fn presses(keys: &[i32], gap_ms: f64, hold_ms: f64, release: bool) -> Vec<Event> {
    let mut ev = Vec::new();
    for (i, &key) in keys.iter().enumerate() {
        let t = 50.0 + i as f64 * gap_ms;
        ev.push(Event {
            t_ms: t,
            key,
            ty: SoundType::Press,
        });
        if release {
            ev.push(Event {
                t_ms: t + hold_ms,
                key,
                ty: SoundType::Release,
            });
        }
    }
    ev.sort_by(|a, b| a.t_ms.total_cmp(&b.t_ms));
    ev
}

fn char_key(c: char) -> Option<(i32, bool)> {
    const LETTERS: [i32; 26] = [
        0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16,
        6,
    ];
    const DIGITS: [i32; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
    match c {
        'a'..='z' => Some((LETTERS[c as usize - 'a' as usize], false)),
        'A'..='Z' => Some((LETTERS[c as usize - 'A' as usize], true)),
        '0'..='9' => Some((DIGITS[c as usize - '0' as usize], false)),
        ' ' => Some((49, false)),
        '\n' => Some((36, false)),
        ',' => Some((43, false)),
        '.' => Some((47, false)),
        ';' => Some((41, false)),
        '\'' => Some((39, false)),
        '-' => Some((27, false)),
        '?' => Some((44, true)),
        '!' => Some((18, true)),
        _ => None,
    }
}

pub fn typing(text: &str, wpm: f64, duration_ms: f64, seed: u32) -> Vec<Event> {
    let mut rng = Xorshift32::new(seed);
    let mean = 60_000.0 / (wpm * 5.0);
    let mut ev = Vec::new();
    let mut t = 100.0;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut typo_in = 40 + rng.next_u32() % 60;
    while t < duration_ms - 600.0 {
        let c = chars[i % chars.len()];
        i += 1;
        let Some((key, shifted)) = char_key(c) else {
            continue;
        };
        let stroke = |ev: &mut Vec<Event>, t: f64, key: i32, rng: &mut Xorshift32| {
            let hold = 70.0 + 50.0 * f64::from(rng.uniform());
            ev.push(Event {
                t_ms: t,
                key,
                ty: SoundType::Press,
            });
            ev.push(Event {
                t_ms: t + hold,
                key,
                ty: SoundType::Release,
            });
        };
        if shifted {
            ev.push(Event {
                t_ms: t - 60.0,
                key: 56,
                ty: SoundType::Press,
            });
            ev.push(Event {
                t_ms: t + 90.0,
                key: 56,
                ty: SoundType::Release,
            });
        }
        stroke(&mut ev, t, key, &mut rng);
        typo_in -= 1;
        if typo_in == 0 {
            typo_in = 40 + rng.next_u32() % 60;
            t += mean * 2.5;
            stroke(&mut ev, t, 51, &mut rng);
        }
        let jitter = f64::from(rng.gauss()) * 0.35;
        let pause = if c == ' ' && rng.uniform() < 0.15 {
            250.0
        } else {
            0.0
        };
        t += mean * (1.0 + jitter).clamp(0.35, 2.5) + pause + if c == '\n' { 600.0 } else { 0.0 };
    }
    ev.sort_by(|a, b| a.t_ms.total_cmp(&b.t_ms));
    ev
}

pub fn write(path: &str, l: &[f32], r: &[f32], rate: f64) -> Result<(), String> {
    wav_out::write(path, l, r, rate as u32).map_err(|e| format!("cannot write {path}: {e}"))?;
    println!("wrote {path}");
    Ok(())
}

pub fn render(args: &Args) -> Result<(), String> {
    let theme = load_theme(args)?;
    let rate: f64 = args.num("--rate", 48_000.0)?;
    if args.flag("--demo") {
        return demo(&theme, rate, args.value("--dir").unwrap_or("out"));
    }
    let key: i32 = args.num("--key", 0)?;
    let n: usize = args.num("--n", 20)?;
    let gap: f64 = args.num("--gap", 300.0)?;
    let hold: f64 = args.num("--hold", 90.0)?;
    let seed: u64 = args.num("--seed", 0)?;
    let out = args.value("--out").unwrap_or("out/render.wav");
    let ev = presses(&vec![key; n], gap, hold, !args.flag("--press-only"));
    let (l, r) = render_synth(&theme, rate, &ev, 50.0 + n as f64 * gap + 300.0, seed);
    write(out, &l, &r, rate)
}

const PASSAGE: &str =
    "The quick brown fox jumps over the lazy dog. Typing on a good board feels calm, \
and every key has its own small voice; some are deep, some are bright, none are the same. \
We wrote this engine to make that feeling last.\nNumbers like 42 and 1984 sound different too, \
and so does the long space bar between words. ";

fn demo(theme: &Theme, rate: f64, dir: &str) -> Result<(), String> {
    let cats: [(&str, &[i32]); 8] = [
        ("letters", &[0, 1, 2, 3, 4, 5]),
        ("space", &[49; 4]),
        ("enter", &[36; 4]),
        ("shift", &[56; 4]),
        ("back", &[51; 4]),
        ("tab", &[48; 4]),
        ("caps", &[57; 4]),
        ("ctrl", &[59; 4]),
    ];
    let out = |name: &str, ev: &[Event], ms: f64| {
        let (l, r) = render_synth(theme, rate, ev, ms, 1);
        write(&format!("{dir}/{name}.wav"), &l, &r, rate)
    };
    for (name, keys) in cats {
        out(
            name,
            &presses(keys, 350.0, 110.0, true),
            50.0 + keys.len() as f64 * 350.0 + 300.0,
        )?;
    }
    out(
        "same_key_10x",
        &presses(&[0; 10], 300.0, 100.0, true),
        3300.0,
    )?;
    let row = [12, 13, 14, 15, 17, 16, 32, 34, 31, 35, 33, 30];
    out(
        "row_walk",
        &presses(&row, 220.0, 100.0, true),
        50.0 + 12.0 * 220.0 + 300.0,
    )?;
    out(
        "passage_80wpm",
        &typing(PASSAGE, 80.0, 30_000.0, 42),
        30_000.0,
    )
}
