use crate::audio::SoundType;
use crate::cli::{self, Args, Event, WavTheme};
use crate::synth::profile::{Group, KeyVoice, Profile};
use crate::theme::KEY_CODES;

pub const BANDS: usize = 21;
const SEGMENT_MS: f64 = 200.0;
const PRE_MS: f64 = 2.0;

pub fn band_centers() -> [f64; BANDS] {
    std::array::from_fn(|k| 1000.0 * 10f64.powf((k as f64 - 10.0) / 10.0))
}

fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

pub struct Spectrum {
    pub power: Vec<f64>,
    pub hz_per_bin: f64,
}

pub fn segment(x: &[f32], rate: f64) -> &[f32] {
    let peak = x.iter().fold(0.0f32, |a, v| a.max(v.abs()));
    let onset = x.iter().position(|v| v.abs() >= 0.1 * peak).unwrap_or(0);
    let start = onset.saturating_sub((PRE_MS * rate / 1000.0) as usize);
    let len = ((SEGMENT_MS * rate / 1000.0) as usize).min(x.len() - start);
    &x[start..start + len]
}

pub fn spectrum(x: &[f32], rate: f64) -> Spectrum {
    let seg = segment(x, rate);
    let n = seg.len().next_power_of_two().max(8192);
    let mut re: Vec<f64> = seg.iter().map(|&v| f64::from(v)).chain(std::iter::repeat(0.0)).take(n).collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im);
    let power = (0..n / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect();
    Spectrum { power, hz_per_bin: rate / n as f64 }
}

fn band_power(s: &Spectrum, lo: f64, hi: f64) -> f64 {
    let a = (lo / s.hz_per_bin).ceil() as usize;
    let b = ((hi / s.hz_per_bin).floor() as usize).min(s.power.len() - 1);
    s.power[a.min(b)..=b].iter().sum::<f64>().max(1e-30)
}

pub fn bands(s: &Spectrum) -> [f64; BANDS] {
    let total: f64 = s.power.iter().sum::<f64>().max(1e-30);
    let edge = 10f64.powf(0.05);
    band_centers().map(|fc| 10.0 * (band_power(s, fc / edge, fc * edge) / total).log10())
}

pub fn smooth_db(s: &Spectrum) -> Vec<f64> {
    let total: f64 = s.power.iter().sum::<f64>().max(1e-30);
    let half = 2f64.powf(1.0 / 6.0);
    let mut out = Vec::new();
    let mut f = 100.0;
    while f <= 10_000.0 {
        let n = ((f * half / s.hz_per_bin).floor() - (f / half / s.hz_per_bin).ceil() + 1.0).max(1.0);
        out.push(10.0 * (band_power(s, f / half, f * half) / n / total).log10());
        f *= 2f64.powf(1.0 / 12.0);
    }
    out
}

pub fn distance(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    (a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>() / n as f64).sqrt()
}

pub fn envelope_db(x: &[f32], rate: f64) -> Vec<f64> {
    let w = ((rate / 1000.0) as usize).max(1);
    let mut out = vec![0.0; x.len()];
    let mut acc = 0.0f64;
    for i in 0..x.len() {
        acc += f64::from(x[i]) * f64::from(x[i]);
        if i >= w {
            acc -= f64::from(x[i - w]) * f64::from(x[i - w]);
        }
        out[i] = acc.max(0.0) / w as f64;
    }
    let max = out.iter().cloned().fold(1e-30, f64::max);
    out.iter().map(|v| 10.0 * (v.max(1e-30) / max).log10()).collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Decay {
    pub onset_ms: f64,
    pub t20_ms: f64,
    pub t40_ms: f64,
}

pub fn decay(x: &[f32], rate: f64) -> Decay {
    let env = envelope_db(x, rate);
    let ms = 1000.0 / rate;
    let peak_amp = x.iter().fold(0.0f32, |a, v| a.max(v.abs()));
    let onset = x.iter().position(|v| v.abs() >= 0.1 * peak_amp).unwrap_or(0);
    let peak = env.iter().position(|&v| v >= 0.0).unwrap_or(0);
    let last = |th: f64| env.iter().rposition(|&v| v >= th).unwrap_or(peak);
    Decay {
        onset_ms: onset as f64 * ms,
        t20_ms: (last(-20.0) - onset) as f64 * ms,
        t40_ms: (last(-40.0) - onset) as f64 * ms,
    }
}

pub struct Measured {
    pub bands: [f64; BANDS],
    pub smooth: Vec<f64>,
    pub decay: Decay,
    pub peak_db: f64,
}

pub fn measure(x: &[f32], rate: f64) -> Measured {
    let s = spectrum(x, rate);
    let peak = x.iter().fold(0.0f32, |a, v| a.max(v.abs()));
    Measured { bands: bands(&s), smooth: smooth_db(&s), decay: decay(x, rate), peak_db: 20.0 * f64::from(peak.max(1e-9)).log10() }
}

fn mean_peak(ms: &[Measured]) -> f64 {
    ms.iter().map(|m| m.peak_db).sum::<f64>() / ms.len() as f64
}

pub fn synth_sound(profile: &Profile, rate: f64, key: i32, ty: SoundType, seed: u64) -> Vec<f32> {
    let ev = [Event { t_ms: 0.0, key, ty }];
    let mut centered = *profile;
    centered.master.stereo_width = 0.0;
    let (l, r) = cli::render_synth(&centered, rate, &ev, 400.0, seed);
    l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect()
}

pub struct Category {
    pub name: &'static str,
    pub files: &'static [&'static str],
    pub keys: &'static [i32],
    pub ty: SoundType,
}

pub const CATEGORIES: [Category; 9] = [
    Category {
        name: "letter press",
        files: &["key1_press", "key2_press", "key3_press", "key4_press", "key5_press", "key6_press"],
        keys: &[0, 1, 2, 3, 13, 14, 37, 38, 40, 31, 45, 8],
        ty: SoundType::Press,
    },
    Category {
        name: "letter release",
        files: &["key1_release", "key2_release", "key3_release", "key4_release", "key5_release", "key6_release"],
        keys: &[0, 1, 2, 3, 13, 14, 37, 38, 40, 31, 45, 8],
        ty: SoundType::Release,
    },
    Category { name: "space press", files: &["space_press"], keys: &[49], ty: SoundType::Press },
    Category { name: "space release", files: &["space_release"], keys: &[49], ty: SoundType::Release },
    Category { name: "enter press", files: &["enter_press"], keys: &[36], ty: SoundType::Press },
    Category { name: "shift press", files: &["shift_press"], keys: &[56], ty: SoundType::Press },
    Category { name: "back press", files: &["back_press"], keys: &[51], ty: SoundType::Press },
    Category { name: "caps press", files: &["caps_press"], keys: &[57], ty: SoundType::Press },
    Category { name: "tab press", files: &["tab_press"], keys: &[48], ty: SoundType::Press },
];

fn mean_bands(ms: &[Measured]) -> [f64; BANDS] {
    let mut out = [0.0; BANDS];
    for m in ms {
        for (o, b) in out.iter_mut().zip(m.bands) {
            *o += b / ms.len() as f64;
        }
    }
    out
}

fn mean_decay(ms: &[Measured]) -> (f64, f64) {
    let n = ms.len() as f64;
    (ms.iter().map(|m| m.decay.t20_ms).sum::<f64>() / n, ms.iter().map(|m| m.decay.t40_ms).sum::<f64>() / n)
}

fn pairwise(a: &[Measured], b: &[Measured], same: bool) -> (f64, f64, f64) {
    let mut v = Vec::new();
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            if !same || j > i {
                v.push(distance(&x.smooth, &y.smooth));
            }
        }
    }
    let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
    let min = v.iter().cloned().fold(f64::MAX, f64::min);
    let max = v.iter().cloned().fold(0.0, f64::max);
    (min, mean, max)
}

pub struct Report {
    pub band_err: f64,
    pub decay_err: f64,
    pub lines: Vec<String>,
}

pub fn compare(profile: &Profile, theme: &WavTheme) -> Report {
    let rate = theme.rate;
    let mut lines = Vec::new();
    let mut band_err = 0.0f64;
    let mut decay_err = 0.0f64;
    let mut creamy_sets = Vec::new();
    let mut synth_sets = Vec::new();
    let centers = band_centers();
    for c in CATEGORIES.iter() {
        let cm: Vec<Measured> = c.files.iter().filter_map(|f| theme.file(f)).map(|x| measure(x, rate)).collect();
        let sm: Vec<Measured> = c
            .keys
            .iter()
            .enumerate()
            .map(|(i, &k)| measure(&synth_sound(profile, rate, k, c.ty, 100 + i as u64), rate))
            .collect();
        let (cb, sb) = (mean_bands(&cm), mean_bands(&sm));
        let (c20, c40) = mean_decay(&cm);
        let (s20, s40) = mean_decay(&sm);
        let mut worst = 0.0f64;
        let mut row_c = String::new();
        let mut row_d = String::new();
        for k in 0..BANDS {
            let d = sb[k] - cb[k];
            worst = worst.max(d.abs());
            row_c.push_str(&format!("{:6.1}", cb[k]));
            row_d.push_str(&format!("{:+6.1}", d));
        }
        let e20 = s20 / c20 - 1.0;
        let e40 = s40 / c40 - 1.0;
        if c.name.starts_with("letter") || c.name.starts_with("space press") {
            band_err = band_err.max(worst);
            decay_err = decay_err.max(e20.abs()).max(e40.abs());
        }
        lines.push(format!(
            "{:15} t20 {:5.1}/{:5.1} ms ({:+4.0}%)  t40 {:5.1}/{:5.1} ms ({:+4.0}%)  worst band {:4.1} dB  peak {:5.1}/{:5.1} dBFS",
            c.name, c20, s20, e20 * 100.0, c40, s40, e40 * 100.0, worst, mean_peak(&cm), mean_peak(&sm)
        ));
        lines.push(format!("   creamy {row_c}"));
        lines.push(format!("   synth- {row_d}"));
        creamy_sets.push(cm);
        synth_sets.push(sm);
    }
    let mut hdr = String::from("   bands  ");
    for f in centers {
        hdr.push_str(&format!("{:>6}", if f < 1000.0 { format!("{:.0}", f) } else { format!("{:.1}k", f / 1000.0) }));
    }
    lines.insert(0, hdr);

    let dist = |name: &str, a: usize, b: usize, lines: &mut Vec<String>| {
        let (cmin, cmean, cmax) = pairwise(&creamy_sets[a], &creamy_sets[b], a == b);
        let (smin, smean, smax) = pairwise(&synth_sets[a], &synth_sets[b], a == b);
        lines.push(format!(
            "{name:28} creamy {cmin:4.1}..{cmax:4.1} mean {cmean:4.1} | synth {smin:4.1}..{smax:4.1} mean {smean:4.1}"
        ));
    };
    lines.push("log-spectral distance (dB)".into());
    dist("letter press vs press", 0, 0, &mut lines);
    dist("letter release vs release", 1, 1, &mut lines);
    dist("letter press vs release", 0, 1, &mut lines);
    dist("space press vs letters", 2, 0, &mut lines);
    dist("enter press vs letters", 4, 0, &mut lines);
    dist("shift press vs letters", 5, 0, &mut lines);
    dist("back press vs letters", 6, 0, &mut lines);

    let same: Vec<Measured> =
        (0..20).map(|i| measure(&synth_sound(profile, rate, 0, SoundType::Press, 1000 + i), rate)).collect();
    let (smin, smean, smax) = pairwise(&same, &same, true);
    lines.push(format!("{:28} synth {smin:4.1}..{smax:4.1} mean {smean:4.1}", "same key x20 (A)"));
    Report { band_err, decay_err, lines }
}

pub fn run(args: &Args) -> Result<(), String> {
    let mut profile = cli::load_profile(args)?;
    let theme = WavTheme::load(cli::THEME_DIR)?;
    if let Some(names) = args.value("--envelope") {
        let times = [0.5, 1.0, 2.0, 3.0, 5.0, 7.0, 10.0, 13.0, 16.0, 20.0, 25.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 100.0];
        print!("{:>14}", "ms");
        for t in times {
            print!("{t:>6}");
        }
        println!();
        for name in names.split(',') {
            let x = match name.parse::<i32>() {
                Ok(key) => synth_sound(&profile, theme.rate, key, SoundType::Press, 1),
                Err(_) => theme.file(name).ok_or_else(|| format!("unknown theme file {name}"))?.to_vec(),
            };
            let env = envelope_db(&x, theme.rate);
            let d = decay(&x, theme.rate);
            let onset = (d.onset_ms * theme.rate / 1000.0) as usize;
            print!("{name:>14}");
            for t in times {
                let i = onset + (t * theme.rate / 1000.0) as usize;
                print!("{:6.1}", env.get(i).copied().unwrap_or(-99.0));
            }
            println!();
        }
        return Ok(());
    }
    if let Some(name) = args.value("--spectrum") {
        let x = match name.parse::<i32>() {
            Ok(key) => synth_sound(&profile, theme.rate, key, SoundType::Press, 1),
            Err(_) => theme.file(name).ok_or_else(|| format!("unknown theme file {name}"))?.to_vec(),
        };
        let s = spectrum(&x, theme.rate);
        let total: f64 = s.power.iter().sum();
        let mut f = args.num("--from", 60.0)?;
        while f <= args.num("--to", 12_000.0)? {
            let k = (f / s.hz_per_bin) as usize;
            let w = ((f * 0.01 / s.hz_per_bin) as usize).max(1);
            let p = s.power[k.saturating_sub(w)..(k + w).min(s.power.len())].iter().cloned().fold(0.0, f64::max);
            let db = 10.0 * (p / total).log10();
            println!("{f:8.0} {db:6.1} {}", "#".repeat(((db + 80.0).max(0.0)) as usize));
            f *= 2f64.powf(1.0 / 24.0);
        }
        return Ok(());
    }
    if args.flag("--fit") {
        profile = fit(&profile, &theme, args.num("--iters", 8)?, args.flag("--fit-keys"));
        let out = args.value("--write").unwrap_or("out/fitted.kprofile");
        std::fs::write(out, profile.to_text()).map_err(|e| format!("cannot write {out}: {e}"))?;
        println!("wrote {out}");
    }
    let report = compare(&profile, &theme);
    for l in &report.lines {
        println!("{l}");
    }
    println!("max band error {:.1} dB, max decay error {:.0}%", report.band_err, report.decay_err * 100.0);
    Ok(())
}

fn band_of(f: f32) -> usize {
    ((10.0 * (f64::from(f) / 1000.0).log10()).round() + 10.0).clamp(0.0, (BANDS - 1) as f64) as usize
}

fn category_bands(profile: &Profile, rate: f64, c: &Category) -> [f64; BANDS] {
    let m: Vec<Measured> = c
        .keys
        .iter()
        .enumerate()
        .map(|(i, &k)| measure(&synth_sound(profile, rate, k, c.ty, 100 + i as u64), rate))
        .collect();
    mean_bands(&m)
}

fn mean_err(err: &[f64; BANDS], lo: usize, hi: usize) -> f32 {
    (err[lo..=hi].iter().sum::<f64>() / (hi - lo + 1) as f64) as f32
}

fn fit_group<const N: usize>(g: &mut Group<N>, err: &[f64; BANDS], step: f32) {
    for i in 0..N {
        let e = err[band_of(g.freqs[i])] as f32;
        g.gains_db[i] -= step * e;
    }
}

fn recenter(p: &mut Profile) {
    let top = [&p.low.modes.gains_db[..], &p.plate.modes.gains_db, &p.keycap.gains_db, &p.switch.gains_db]
        .concat()
        .into_iter()
        .fold(f32::MIN, f32::max);
    for g in p.low.modes.gains_db.iter_mut().chain(&mut p.plate.modes.gains_db).chain(&mut p.keycap.gains_db) {
        *g = (*g - top).max(-60.0);
    }
    for g in p.switch.gains_db.iter_mut() {
        *g = (*g - top).max(-60.0);
    }
    let r = &mut p.release;
    let mean = (r.low_db + r.plate_db + r.keycap_db + r.switch_db + r.noise_db) / 5.0;
    for g in [&mut r.low_db, &mut r.plate_db, &mut r.keycap_db, &mut r.switch_db, &mut r.noise_db] {
        *g -= mean;
    }
}

pub fn fit(base: &Profile, theme: &WavTheme, iters: usize, keys: bool) -> Profile {
    let rate = theme.rate;
    let target = |c: &Category| {
        let m: Vec<Measured> = c.files.iter().filter_map(|f| theme.file(f)).map(|x| measure(x, rate)).collect();
        mean_bands(&m)
    };
    let (tp, tr) = (target(&CATEGORIES[0]), target(&CATEGORIES[1]));
    let mut p = *base;
    let mut best = (f64::MAX, p);
    let step = 0.4;
    for it in 0..iters {
        let before = p;
        let sp = category_bands(&p, rate, &CATEGORIES[0]);
        let ep: [f64; BANDS] = std::array::from_fn(|k| sp[k] - tp[k]);
        fit_group(&mut p.low.modes, &ep, step);
        fit_group(&mut p.plate.modes, &ep, step);
        fit_group(&mut p.keycap, &ep, step);
        fit_group(&mut p.switch, &ep, step);
        p.noise.level_db -= step * mean_err(&ep, 18, 20);
        let sr = category_bands(&p, rate, &CATEGORIES[1]);
        let er: [f64; BANDS] = std::array::from_fn(|k| sr[k] - tr[k]);
        let r = &mut p.release;
        r.low_db -= step * mean_err(&er, 0, 4);
        r.keycap_db -= step * mean_err(&er, 8, 11);
        r.plate_db -= step * mean_err(&er, 11, 14);
        r.switch_db -= step * mean_err(&er, 15, 17);
        r.noise_db -= step * mean_err(&er, 18, 20);
        recenter(&mut p);
        let worst = |e: &[f64; BANDS]| e.iter().fold(0.0f64, |a, v| a.max(v.abs()));
        let mut special = 0.0f64;
        for &(name, code) in KEY_CODES.iter().filter(|_| keys) {
            let name = std::str::from_utf8(name).unwrap_or_default();
            let Some(x) = theme.file(&format!("{name}_press")) else { continue };
            let target = measure(x, rate);
            let synth: Vec<Measured> =
                (0..4).map(|i| measure(&synth_sound(&p, rate, code, SoundType::Press, 300 + i), rate)).collect();
            let sb = mean_bands(&synth);
            let e: [f64; BANDS] = std::array::from_fn(|k| sb[k] - target.bands[k]);
            special = special.max(worst(&e));
            let (_, s40) = mean_decay(&synth);
            let mut v = *p.key_voice(code as u32);
            if v.code == u32::MAX {
                v.code = code as u32;
            }
            for (g, (lo, hi)) in [(0, 4), (11, 14), (7, 11), (15, 17), (18, 20)].into_iter().enumerate() {
                v.gains_db[g] -= step * mean_err(&e, lo, hi);
            }
            let mean = v.gains_db.iter().sum::<f32>() / 5.0;
            for g in v.gains_db.iter_mut() {
                *g = (*g - mean).clamp(-40.0, 40.0);
            }
            v.decay = (v.decay * ((target.decay.t40_ms / s40.max(1.0)) as f32).powf(0.5)).clamp(0.3, 2.5);
            set_voice(&mut p, v);
        }
        let shift = *p.key_voice(56);
        set_voice(&mut p, KeyVoice { code: 60, ..shift });
        let score = worst(&ep).max(worst(&er)).max(special);
        if score < best.0 {
            best = (score, before);
        }
        eprintln!(
            "fit {it}: press worst {:.1} dB, release worst {:.1} dB, special worst {special:.1} dB",
            worst(&ep),
            worst(&er)
        );
    }
    best.1
}

fn set_voice(p: &mut Profile, v: KeyVoice) {
    let slot = p.keys.iter().position(|k| k.code == v.code).or_else(|| p.keys.iter().position(|k| k.code == u32::MAX));
    if let Some(i) = slot {
        p.keys[i] = v;
    }
}
