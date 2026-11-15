pub mod layout;
pub mod rng;
pub mod theme;
pub mod wav_out;

use std::f32::consts::PI;

use layout::KeyGeom;
use rng::{splitmix64, Xorshift32, GOLDEN};
use theme::{Group, Theme};

pub const MODES: usize = 24;
pub const VOICES: usize = 24;
pub const KEYS: usize = 128;
pub const LOW: usize = 0;
pub const PLATE: usize = 4;
pub const KEYCAP: usize = 16;
pub const SWITCH: usize = 20;
const MAX_PULSES: usize = 6;
const MAX_SHAPE: usize = 128;
const MAX_CLICK: usize = 32;
const CLICK_OFF_DB: f32 = -100.0;
const BLOCK: usize = 128;
const LN_1000: f32 = 6.907_755;
const NEPER_PER_DB: f32 = 0.115_129_25;
const TAIL: f32 = 1.05;
const REFERENCE_KEY: usize = 0;
const RING: usize = 32;
const BOARD_UNITS: [f32; 2] = [15.0, 6.0];
const ITD_S: f32 = 0.000_17;
const ITD_MAX_HZ: f32 = 1500.0;
const LOUDNESS_TARGET: f64 = 1e-4;
const REFERENCE_PEAK: f32 = 0.5;
const HEADROOM: f32 = 0.75;

#[derive(Clone, Copy)]
struct Field {
    k: [[f32; 2]; 2],
    phase: [f32; 2],
}

impl Field {
    fn new(rng: &mut Xorshift32) -> Field {
        let wave = |rng: &mut Xorshift32| {
            let a = 2.0 * PI * rng.uniform();
            let c = 1.0 / rng.range(8.0, 20.0);
            [c * a.cos(), c * a.sin()]
        };
        Field {
            k: [wave(rng), wave(rng)],
            phase: [2.0 * PI * rng.uniform(), 2.0 * PI * rng.uniform()],
        }
    }

    fn at(&self, g: &KeyGeom) -> f32 {
        let (u, v) = (g.x * BOARD_UNITS[0], g.y * BOARD_UNITS[1]);
        let w = |i: usize| (2.0 * PI * (self.k[i][0] * u + self.k[i][1] * v) + self.phase[i]).sin();
        0.5 * (w(0) + w(1))
    }
}

struct Fields {
    mode: [Field; MODES],
    gain: [Field; 5],
    hit: [Field; 3],
}

impl Fields {
    fn new(seed: u64) -> Fields {
        let s = splitmix64(seed ^ 0x5350_4143_4500_0000);
        let mut rng = Xorshift32::new((s ^ (s >> 32)) as u32 | 1);
        Fields {
            mode: std::array::from_fn(|_| Field::new(&mut rng)),
            gain: std::array::from_fn(|_| Field::new(&mut rng)),
            hit: std::array::from_fn(|_| Field::new(&mut rng)),
        }
    }
}

#[derive(Clone, Copy)]
pub struct KeyModel {
    pub w: [f32; MODES],
    cos: [f32; MODES],
    sin: [f32; MODES],
    norm: [f32; MODES],
    pub lnr: [f32; MODES],
    pub amp: [f32; MODES],
    gain: [f32; 2],
    delay: [u32; 2],
    shadow: [f32; 2],
    width: f32,
    click_width: f32,
    sec_gap: f32,
    sec_amp: f32,
    stab_count: u32,
    noise_amp: f32,
}

impl KeyModel {
    fn scale(&mut self, s: f32) {
        for a in self.amp.iter_mut() {
            *a *= s;
        }
        self.noise_amp *= s;
    }
}

const EMPTY_MODEL: KeyModel = KeyModel {
    w: [0.0; MODES],
    cos: [1.0; MODES],
    sin: [0.0; MODES],
    norm: [0.0; MODES],
    lnr: [-1.0; MODES],
    amp: [0.0; MODES],
    gain: [1.0; 2],
    delay: [0; 2],
    shadow: [1.0; 2],
    width: 4.0,
    click_width: 3.0,
    sec_gap: 0.0,
    sec_amp: 0.0,
    stab_count: 0,
    noise_amp: 0.0,
};

struct Params {
    var: f32,
    spatial: bool,
    pitch: f32,
    pitch_clip: f32,
    mode_pitch: f32,
    mode_gain: f32,
    level: f32,
    decay: f32,
    hardness: f32,
    sec_gap: f32,
    sec_level: f32,
    vel_level: f32,
    vel_hardness: f32,
    vel_gap: f32,
    stab_delay: [f32; 2],
    stab_level: [f32; 2],
    rel_hardness: f32,
    rel_decay: f32,
    rel_gain: [f32; MODES],
    rel_noise: f32,
    rel_level: f32,
    rel_sec_chance: f32,
    rel_sec_gap: [f32; 2],
    rel_sec_amp: f32,
    click_on: bool,
    itd_split: f32,
    click_amp: f32,
    click_delay: [f32; 2],
    click_rel_delay: [f32; 2],
    click_rel_amp: f32,
    click_double: [f32; 2],
    click_double_amp: f32,
    noise_hp: f32,
    noise_lp: f32,
    noise_attack: f32,
    noise_lnr: f32,
    drive: f32,
    drive_tail: u32,
}

pub struct SynthBank {
    pub keys: [KeyModel; KEYS],
    params: Params,
}

#[inline(always)]
fn exp_approx(x: f32) -> f32 {
    let t = x * 0.25;
    let p = 1.0 + t * (1.0 + t * (0.5 + t * (1.0 / 6.0 + t * (1.0 / 24.0 + t * (1.0 / 120.0)))));
    let p = p * p;
    p * p
}

#[inline(always)]
fn exp_small(x: f32) -> f32 {
    1.0 + x * (1.0 + x * (0.5 + x * (1.0 / 6.0)))
}

#[inline(always)]
fn sin_pi(u: f32) -> f32 {
    let u = if u > 0.5 { 1.0 - u } else { u };
    let t = PI * u;
    let t2 = t * t;
    t * (1.0 - t2 / 6.0 * (1.0 - t2 / 20.0 * (1.0 - t2 / 42.0 * (1.0 - t2 / 72.0))))
}

fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

fn set_group<const N: usize>(
    g: &Group<N>,
    base: usize,
    freqs: &mut [f32; MODES],
    t60: &mut [f32; MODES],
    gains: &mut [f32; MODES],
) {
    for i in 0..N {
        freqs[base + i] = g.freqs[i];
        t60[base + i] = g.t60_ms[i] * 0.001;
        gains[base + i] = db(g.gains_db[i]);
    }
}

fn key_model(
    p: &Theme,
    patterns: &[[f32; 2]; 12],
    fields: &Fields,
    code: usize,
    fs: f32,
) -> KeyModel {
    let seed = splitmix64(p.master.seed ^ (code as u64).wrapping_mul(GOLDEN));
    let mut rng = Xorshift32::new((seed ^ (seed >> 32)) as u32);
    let geom = layout::geometry(code as u8).unwrap_or_else(|| KeyGeom {
        x: rng.uniform(),
        y: rng.uniform(),
        width: 1.0,
    });
    let large = geom.width >= p.stab.min_width;
    let octaves = geom.width.max(1.0).log2();
    let damping = 1.0 + (p.stab.damping - 1.0) * ((geom.width - 1.0) / 5.25).clamp(0.0, 1.0);
    let size_f = geom.width.powf(p.master.size_freq_exp);
    let size_d = geom.width.powf(p.master.size_decay_exp);

    let (mut freqs, mut t60, mut gains) = ([0.0; MODES], [0.0; MODES], [0.0; MODES]);
    set_group(&p.low.modes, LOW, &mut freqs, &mut t60, &mut gains);
    set_group(&p.plate.modes, PLATE, &mut freqs, &mut t60, &mut gains);
    set_group(&p.keycap, KEYCAP, &mut freqs, &mut t60, &mut gains);
    set_group(&p.switch, SWITCH, &mut freqs, &mut t60, &mut gains);

    let voice = p.key_voice(code as u32);
    let spread: [f32; 5] = std::array::from_fn(|g| {
        let amount = if g == 0 { 0.3 } else { 1.0 };
        db(fields.gain[g].at(&geom) * amount * p.master.key_spread_db + voice.gains_db[g])
    });
    for m in 0..MODES {
        gains[m] *= spread[group(m)];
        t60[m] *= voice.decay;
    }
    let low_shift = (p.low.large_freq / p.low.modes.freqs[0]).powf(octaves);
    let detune = |m: usize, amount: f32| 1.0 + amount * fields.mode[m].at(&geom);
    for m in LOW..PLATE {
        freqs[m] *= low_shift * detune(m, p.low.modes.detune);
        gains[m] *= db(p.stab.low_db * octaves);
    }
    for m in PLATE..KEYCAP {
        freqs[m] *= detune(m, p.plate.modes.detune);
        let [pm, qm] = patterns[m - PLATE];
        let shape = ((PI * pm * geom.x).sin() * (PI * qm * geom.y).sin()).abs();
        gains[m] *= p.plate.floor + (1.0 - p.plate.floor) * shape;
    }
    for m in KEYCAP..SWITCH {
        freqs[m] *= size_f * detune(m, p.keycap.detune);
        t60[m] *= size_d;
    }
    for (m, f) in freqs.iter_mut().enumerate().skip(SWITCH) {
        *f *= detune(m, p.switch.detune);
    }

    let mut k = EMPTY_MODEL;
    for m in 0..MODES {
        let w = 2.0 * PI * freqs[m].clamp(20.0, 0.45 * fs) / fs;
        k.w[m] = w;
        k.cos[m] = w.cos();
        k.sin[m] = w.sin();
        k.norm[m] = if m < PLATE {
            w.sin()
        } else {
            w.sin() / (4.0 * (0.5 * w).sin().powi(2))
        };
        k.lnr[m] = -LN_1000 / (t60[m].max(0.002) * damping * fs);
        k.amp[m] = gains[m];
    }
    let pan = ((geom.x - 0.5) * 2.0 * p.master.stereo_width).clamp(-1.0, 1.0);
    let angle = 0.25 * PI * (pan + 1.0);
    k.gain = [
        angle.cos() * std::f32::consts::SQRT_2,
        angle.sin() * std::f32::consts::SQRT_2,
    ];
    let lag = ((ITD_S * fs * pan.abs()) as u32).min(RING as u32 - 1);
    let shade = 1.0 - (-2.0 * PI * (18_000.0 - 12_000.0 * pan.abs()).min(0.45 * fs) / fs).exp();
    let far = usize::from(pan < 0.0);
    k.delay[far] = lag;
    k.shadow[far] = shade;
    let lerp = |r: [f32; 2], f: &Field| r[0] + (r[1] - r[0]) * (0.5 + 0.5 * f.at(&geom));
    k.width = lerp(p.hit.width_ms, &fields.hit[0]) * 0.001 * fs;
    k.click_width = lerp(p.click.width_ms, &fields.hit[0]) * 0.001 * fs;
    k.sec_gap = lerp(p.hit.sec_gap_ms, &fields.hit[1]) * 0.001 * fs;
    k.sec_amp = db(lerp(p.hit.sec_level_db, &fields.hit[2]));
    k.stab_count = if large {
        p.stab.count[0] + rng.next_u32() % (p.stab.count[1].saturating_sub(p.stab.count[0]) + 1)
    } else {
        0
    };
    k.noise_amp = db(p.noise.level_db) * spread[4];
    k
}

pub fn group(m: usize) -> usize {
    match m {
        LOW..PLATE => 0,
        PLATE..KEYCAP => 1,
        KEYCAP..SWITCH => 2,
        _ => 3,
    }
}

impl SynthBank {
    pub fn new(p: &Theme, sample_rate: f64) -> Box<SynthBank> {
        let fs = sample_rate as f32;
        let ms = 0.001 * fs;
        let mut prng = Xorshift32::new((p.master.seed ^ (p.master.seed >> 32)) as u32);
        let patterns: [[f32; 2]; 12] = std::array::from_fn(|_| {
            [
                (1 + prng.next_u32() % 4) as f32,
                (1 + prng.next_u32() % 4) as f32,
            ]
        });
        let r = &p.release;
        let mut rel_gain = [0.0; MODES];
        for (m, g) in rel_gain.iter_mut().enumerate() {
            *g = db([r.low_db, r.plate_db, r.keycap_db, r.switch_db][group(m)]);
        }
        let v = &p.variation;
        let mut bank = Box::new(SynthBank {
            keys: [EMPTY_MODEL; KEYS],
            params: Params {
                var: 1.0,
                spatial: true,
                pitch: v.pitch,
                pitch_clip: v.pitch_clip,
                mode_pitch: v.mode_pitch,
                mode_gain: v.mode_gain_db * NEPER_PER_DB,
                level: v.level_db * NEPER_PER_DB,
                decay: v.decay,
                hardness: v.hardness,
                sec_gap: v.sec_gap,
                sec_level: v.sec_level_db * NEPER_PER_DB,
                vel_level: p.velocity.level_db * NEPER_PER_DB,
                vel_hardness: p.velocity.hardness,
                vel_gap: p.velocity.gap,
                stab_delay: [p.stab.delay_ms[0] * ms, p.stab.delay_ms[1] * ms],
                stab_level: [
                    p.stab.level_db[0] * NEPER_PER_DB,
                    p.stab.level_db[1] * NEPER_PER_DB,
                ],
                rel_hardness: r.hardness,
                rel_decay: r.decay,
                rel_gain,
                rel_noise: db(r.noise_db),
                rel_level: db(r.level_db),
                rel_sec_chance: r.sec_chance,
                rel_sec_gap: [r.sec_gap_ms[0] * ms, r.sec_gap_ms[1] * ms],
                rel_sec_amp: db(r.sec_level_db),
                click_on: p.click.level_db > CLICK_OFF_DB,
                itd_split: 1.0 - (-2.0 * PI * ITD_MAX_HZ / fs).exp(),
                click_amp: db(p.click.level_db),
                click_delay: [p.click.delay_ms[0] * ms, p.click.delay_ms[1] * ms],
                click_rel_delay: [
                    p.click.release_delay_ms[0] * ms,
                    p.click.release_delay_ms[1] * ms,
                ],
                click_rel_amp: db(p.click.release_db),
                click_double: [p.click.double_ms[0] * ms, p.click.double_ms[1] * ms],
                click_double_amp: db(p.click.double_db),
                noise_hp: 1.0 / (1.0 + 2.0 * PI * p.noise.hp_hz / fs),
                noise_lp: 1.0 - (-2.0 * PI * p.noise.lp_hz.min(0.45 * fs) / fs).exp(),
                noise_attack: (p.noise.attack_ms * ms).max(1.0),
                noise_lnr: -LN_1000 / (p.noise.t60_ms.max(1.0) * ms),
                drive: 1.0 - (-2.0 * PI * p.low.drive_hz.clamp(10.0, 0.45 * fs) / fs).exp(),
                drive_tail: (LN_1000 * 3.0 * fs
                    / (2.0 * PI * p.low.drive_hz.clamp(10.0, 0.45 * fs)))
                    as u32,
            },
        });
        let fields = Fields::new(p.master.seed);
        for (code, k) in bank.keys.iter_mut().enumerate() {
            *k = key_model(p, &patterns, &fields, code, fs);
        }
        bank.normalize(p, db(p.release.level_db), fs);
        bank
    }

    fn nominal_peak(&mut self, code: usize, release: bool) -> f32 {
        self.peak(code, release, false)
    }

    fn peak(&mut self, code: usize, release: bool, spatial: bool) -> f32 {
        self.params.var = 0.0;
        self.params.spatial = spatial;
        let mut v = SynthVoice::IDLE;
        v.start(self, code, release, 0, 1);
        self.params.var = 1.0;
        self.params.spatial = true;
        let mut peak = 0.0f32;
        let (mut l, mut r) = ([0.0; BLOCK], [0.0; BLOCK]);
        while v.active() {
            l.fill(0.0);
            r.fill(0.0);
            v.render(&mut l, &mut r);
            peak = l.iter().chain(&r).fold(peak, |a, x| a.max(x.abs()));
        }
        peak
    }

    fn loudness(&mut self, code: usize, release: bool, fs: f32) -> f64 {
        self.params.var = 0.0;
        let mut v = SynthVoice::IDLE;
        v.start(self, code, release, 0, 1);
        self.params.var = 1.0;
        let (shelf, hp) = k_weighting(f64::from(fs));
        let mut state = [[0.0f64; 4]; 6];
        let mut e = [0.0f64; 3];
        let (mut l, mut r) = ([0.0; BLOCK], [0.0; BLOCK]);
        while v.active() {
            l.fill(0.0);
            r.fill(0.0);
            v.render(&mut l, &mut r);
            for (a, b) in l.iter().zip(&r) {
                let (a, b) = (f64::from(*a), f64::from(*b));
                for (c, x) in [a, b, 0.5 * (a + b)].into_iter().enumerate() {
                    let shelved = biquad(&shelf, &mut state[2 * c], x);
                    let y = biquad(&hp, &mut state[2 * c + 1], shelved);
                    e[c] += y * y;
                }
            }
        }
        let fs = f64::from(fs);
        (0.5 * (e[0] + e[1]) / fs * 2.0 * e[2] / fs).sqrt()
    }

    fn normalize(&mut self, p: &Theme, release: f32, fs: f32) {
        let target = REFERENCE_PEAK;
        let saved: [f32; KEYS] = std::array::from_fn(|k| self.keys[k].noise_amp);
        for k in self.keys.iter_mut() {
            k.noise_amp = 0.0;
        }
        let modal = self.nominal_peak(REFERENCE_KEY, false).max(1e-9);
        for (k, s) in self.keys.iter_mut().zip(saved) {
            k.noise_amp = modal * s;
        }
        let scale = target / self.nominal_peak(REFERENCE_KEY, false).max(1e-9);
        for k in self.keys.iter_mut() {
            k.scale(scale);
        }
        self.params.rel_level = 1.0;
        self.params.rel_level = target * release / self.nominal_peak(REFERENCE_KEY, true).max(1e-9);
        let loudness: Vec<f64> = (0..KEYS)
            .map(|code| self.loudness(code, false, fs).max(1e-20))
            .collect();
        let placed: Vec<f64> = (0..KEYS)
            .filter(|&c| layout::geometry(c as u8).is_some())
            .map(|c| loudness[c])
            .collect();
        let mean = placed.iter().sum::<f64>() / placed.len().max(1) as f64;
        let level = (LOUDNESS_TARGET / mean).sqrt();
        for (code, k) in self.keys.iter_mut().enumerate() {
            let even = (mean / loudness[code]).sqrt();
            k.scale(db(p.key_voice(code as u32).level_db) * (even * level) as f32);
        }
        let press = self.loudness(REFERENCE_KEY, false, fs);
        let released = self.loudness(REFERENCE_KEY, true, fs).max(1e-20);
        self.params.rel_level *= release * (press / released).sqrt() as f32;
        let loudest = (0..KEYS)
            .map(|c| self.peak(c, false, true))
            .fold(0.0f32, f32::max);
        if loudest > HEADROOM {
            let s = HEADROOM / loudest;
            for k in self.keys.iter_mut() {
                k.scale(s);
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct SynthVoice {
    b1: [f32; MODES],
    b2: [f32; MODES],
    gl: [f32; MODES],
    gh: [f32; MODES],
    y1: [f32; MODES],
    y2: [f32; MODES],
    shape: [f32; MAX_SHAPE],
    shape_len: u32,
    x_hist: [f32; 2],
    gc: [f32; MODES],
    click_shape: [f32; MAX_CLICK],
    click_len: u32,
    click_at: [u32; 2],
    click_amp: [f32; 2],
    clicks: u32,
    c_hist: [f32; 2],
    drive: f32,
    drive_state: [f32; 3],
    pulse_at: [u32; MAX_PULSES],
    pulse_amp: [f32; MAX_PULSES],
    pulses: u32,
    exc_end: u32,
    noise_rng: Xorshift32,
    hp_a: f32,
    hp_x: f32,
    hp_y: f32,
    lp_b: f32,
    lp_y: f32,
    env: f32,
    env_mul: f32,
    att_step: f32,
    att_end: u32,
    noise_amp: f32,
    noise_end: u32,
    gain: [f32; 2],
    delay: [u32; 2],
    shadow: [f32; 2],
    shade: [f32; 2],
    split: f32,
    low: f32,
    ring: [f32; RING],
    pos: u32,
    frames: u32,
}

impl SynthVoice {
    pub const IDLE: SynthVoice = SynthVoice {
        b1: [0.0; MODES],
        b2: [0.0; MODES],
        gl: [0.0; MODES],
        gh: [0.0; MODES],
        y1: [0.0; MODES],
        y2: [0.0; MODES],
        shape: [0.0; MAX_SHAPE],
        shape_len: 0,
        x_hist: [0.0; 2],
        gc: [0.0; MODES],
        click_shape: [0.0; MAX_CLICK],
        click_len: 0,
        click_at: [0; 2],
        click_amp: [0.0; 2],
        clicks: 0,
        c_hist: [0.0; 2],
        drive: 0.0,
        drive_state: [0.0; 3],
        pulse_at: [0; MAX_PULSES],
        pulse_amp: [0.0; MAX_PULSES],
        pulses: 0,
        exc_end: 0,
        noise_rng: Xorshift32::ZERO,
        hp_a: 0.0,
        hp_x: 0.0,
        hp_y: 0.0,
        lp_b: 0.0,
        lp_y: 0.0,
        env: 0.0,
        env_mul: 0.0,
        att_step: 0.0,
        att_end: 0,
        noise_amp: 0.0,
        noise_end: 0,
        gain: [0.0; 2],
        delay: [0; 2],
        shadow: [1.0; 2],
        shade: [0.0; 2],
        split: 0.0,
        low: 0.0,
        ring: [0.0; RING],
        pos: 0,
        frames: 0,
    };

    #[inline(always)]
    pub fn active(&self) -> bool {
        self.pos < self.frames
    }

    pub fn start(&mut self, bank: &SynthBank, code: usize, release: bool, velocity: u8, seed: u32) {
        let k = &bank.keys[code % KEYS];
        let p = &bank.params;
        let var = p.var;
        let mut rng = Xorshift32::new(seed);
        let vel = f32::from(velocity.min(127)) * (1.0 / 127.0);

        let pitch = (rng.gauss() * p.pitch * var).clamp(-p.pitch_clip, p.pitch_clip);
        let mut level = exp_approx(rng.gauss() * p.level * var + vel * p.vel_level);
        let mut decay = 1.0;
        if release {
            level *= p.rel_level;
            decay = p.rel_decay;
        }
        let mut min_lnr = f32::MAX;
        for m in 0..MODES {
            let d = k.w[m] * (pitch + rng.gauss() * p.mode_pitch * var);
            let (c0, s0) = (k.cos[m], k.sin[m]);
            let c = c0 - s0 * d - 0.5 * c0 * d * d;
            let lnr = k.lnr[m] / ((1.0 + rng.gauss() * p.decay * var) * decay);
            let r = exp_small(lnr);
            self.b1[m] = 2.0 * r * c;
            self.b2[m] = r * r;
            let mut a = k.amp[m] * level * exp_approx(rng.gauss() * p.mode_gain * var);
            if release {
                a *= p.rel_gain[m];
            }
            let a = a * k.norm[m];
            (self.gl[m], self.gh[m], self.gc[m]) = match m {
                _ if m < PLATE => (a, 0.0, 0.0),
                _ if m >= SWITCH && p.click_on => (0.0, 0.0, a),
                _ => (0.0, a, 0.0),
            };
            self.y1[m] = 0.0;
            self.y2[m] = 0.0;
            if a != 0.0 {
                min_lnr = min_lnr.min(-lnr);
            }
        }

        let mut hard = (1.0 + rng.gauss() * p.hardness * var) * (1.0 + vel * p.vel_hardness);
        if release {
            hard *= p.rel_hardness;
        }
        let len = ((k.width / hard.max(0.1)) as usize).clamp(4, MAX_SHAPE);
        let inv = 1.0 / len as f32;
        for (j, v) in self.shape[..len].iter_mut().enumerate() {
            let s = sin_pi((j as f32 + 0.5) * inv);
            *v = 2.0 * inv * s * s;
        }
        self.shape_len = len as u32;

        self.pulse_at[0] = 0;
        self.pulse_amp[0] = 1.0;
        let mut n = 1;
        if !release {
            let gap = k.sec_gap * (1.0 + rng.gauss() * p.sec_gap * var) * (1.0 - vel * p.vel_gap);
            self.pulse_at[n] = gap.max(0.0) as u32;
            self.pulse_amp[n] = k.sec_amp * exp_approx(rng.gauss() * p.sec_level * var);
            n += 1;
        } else if rng.uniform() < p.rel_sec_chance {
            self.pulse_at[n] = rng.range(p.rel_sec_gap[0], p.rel_sec_gap[1]) as u32;
            self.pulse_amp[n] = p.rel_sec_amp * exp_approx(rng.gauss() * p.sec_level * var);
            n += 1;
        }
        for _ in 0..k.stab_count.min((MAX_PULSES - n) as u32) {
            self.pulse_at[n] = rng.range(p.stab_delay[0], p.stab_delay[1]) as u32;
            self.pulse_amp[n] = exp_approx(rng.range(p.stab_level[0], p.stab_level[1]));
            n += 1;
        }
        self.pulses = n as u32;
        self.clicks = 0;
        self.c_hist = [0.0; 2];
        let mut click_end = 0;
        if p.click_on {
            let delay = if release {
                rng.range(p.click_rel_delay[0], p.click_rel_delay[1])
            } else {
                rng.range(p.click_delay[0], p.click_delay[1]) * (1.0 - vel * p.vel_gap)
            };
            let offset = delay.max(0.0) as u32;
            for at in self.pulse_at[..n].iter_mut() {
                *at += offset;
            }
            let amp = p.click_amp
                * if release { p.click_rel_amp } else { 1.0 }
                * exp_approx(rng.gauss() * p.mode_gain * var);
            let double = rng.range(p.click_double[0], p.click_double[1]) as u32;
            self.click_at = [0, double];
            self.click_amp = [
                amp,
                amp * p.click_double_amp * exp_approx(rng.gauss() * p.mode_gain * var),
            ];
            self.clicks = 2;
            let w = k.click_width.clamp(2.0, MAX_CLICK as f32);
            let clen = (w.ceil() as usize).min(MAX_CLICK);
            let mut area = 0.0;
            for (j, v) in self.click_shape[..clen].iter_mut().enumerate() {
                let u = (j as f32 + 0.5) / w;
                let s = if u < 1.0 { sin_pi(u) } else { 0.0 };
                *v = s * s;
                area += *v;
            }
            for v in self.click_shape[..clen].iter_mut() {
                *v /= area;
            }
            self.click_len = clen as u32;
            click_end = double + clen as u32 + 2;
        }
        self.exc_end =
            (self.pulse_at[..n].iter().max().copied().unwrap_or(0) + len as u32 + 2).max(click_end);
        self.x_hist = [0.0; 2];
        self.exc_end += p.drive_tail;
        self.drive = p.drive;
        self.drive_state = [0.0; 3];

        let noise_lnr = p.noise_lnr / ((1.0 + rng.gauss() * p.decay * var) * decay);
        self.noise_rng = Xorshift32::new(rng.next_u32());
        self.hp_a = p.noise_hp;
        self.lp_b = p.noise_lp;
        self.hp_x = 0.0;
        self.hp_y = 0.0;
        self.lp_y = 0.0;
        self.env = 0.0;
        self.env_mul = exp_small(noise_lnr);
        self.att_step = 1.0 / p.noise_attack;
        self.att_end = p.noise_attack as u32;
        self.noise_amp = k.noise_amp * level * if release { p.rel_noise } else { 1.0 };
        self.noise_end = if self.noise_amp != 0.0 {
            self.att_end + (LN_1000 * TAIL / -noise_lnr) as u32
        } else {
            0
        };

        if p.spatial {
            (self.gain, self.delay, self.shadow) = (k.gain, k.delay, k.shadow);
        } else {
            (self.gain, self.delay, self.shadow) = ([1.0; 2], [0; 2], [1.0; 2]);
        }
        self.shade = [0.0; 2];
        self.split = p.itd_split;
        self.low = 0.0;
        self.ring = [0.0; RING];
        self.pos = 0;
        let modal = if min_lnr < f32::MAX {
            (LN_1000 * TAIL / min_lnr) as u32
        } else {
            0
        };
        self.frames =
            modal.max(self.noise_end).max(self.exc_end) + self.delay[0].max(self.delay[1]);
    }

    #[inline]
    pub fn render(&mut self, out_l: &mut [f32], out_r: &mut [f32]) {
        let total = out_l.len().min(out_r.len());
        let mut done = 0;
        while done < total && self.active() {
            let edge = [self.exc_end, self.noise_end, self.frames]
                .into_iter()
                .filter(|&e| e > self.pos)
                .min()
                .unwrap_or(self.frames);
            let n = (total - done).min(BLOCK).min((edge - self.pos) as usize);
            let mut mono = [0.0f32; BLOCK];
            self.render_block(&mut mono[..n]);
            let start = self.pos - n as u32;
            let ([gl, gr], [dl, dr], [al, ar]) = (self.gain, self.delay, self.shadow);
            let [mut zl, mut zr] = self.shade;
            for (i, ((l, r), s)) in out_l[done..done + n]
                .iter_mut()
                .zip(&mut out_r[done..done + n])
                .zip(&mono[..n])
                .enumerate()
            {
                let t = start + i as u32;
                self.low += self.split * (*s - self.low);
                let high = *s - self.low;
                self.ring[t as usize % RING] = self.low;
                zl += al * (self.ring[t.wrapping_sub(dl) as usize % RING] + high - zl);
                zr += ar * (self.ring[t.wrapping_sub(dr) as usize % RING] + high - zr);
                *l += zl * gl;
                *r += zr * gr;
            }
            self.shade = [zl, zr];
            done += n;
        }
    }

    #[inline(always)]
    fn render_block(&mut self, out: &mut [f32]) {
        let n = out.len();
        let pos = self.pos;
        if pos < self.exc_end {
            let mut x = [0.0f32; BLOCK + 2];
            x[0] = self.x_hist[0];
            x[1] = self.x_hist[1];
            let len = self.shape_len;
            for p in 0..self.pulses as usize {
                let s = self.pulse_at[p];
                if s >= pos + n as u32 || s + len <= pos {
                    continue;
                }
                let a = self.pulse_amp[p];
                let j0 = pos.saturating_sub(s);
                let j1 = (pos + n as u32 - s).min(len);
                for j in j0..j1 {
                    x[(s + j - pos) as usize + 2] += a * self.shape[j as usize];
                }
            }
            self.x_hist = [x[n], x[n + 1]];
            let mut d = [0.0f32; BLOCK];
            for i in 0..n {
                d[i] = x[i + 2] - 2.0 * x[i + 1] + x[i];
            }
            let mut lo = [0.0f32; BLOCK];
            let (a, mut z) = (self.drive, self.drive_state);
            for i in 0..n {
                z[0] += a * (x[i + 2] - z[0]);
                z[1] += a * (z[0] - z[1]);
                z[2] += a * (z[1] - z[2]);
                lo[i] = z[2];
            }
            self.drive_state = z;
            let mut dc = [0.0f32; BLOCK];
            if self.clicks > 0 {
                let mut c = [0.0f32; BLOCK + 2];
                c[0] = self.c_hist[0];
                c[1] = self.c_hist[1];
                let len = self.click_len;
                for k in 0..self.clicks as usize {
                    let s = self.click_at[k];
                    if s >= pos + n as u32 || s + len <= pos {
                        continue;
                    }
                    let a = self.click_amp[k];
                    let j0 = pos.saturating_sub(s);
                    let j1 = (pos + n as u32 - s).min(len);
                    for j in j0..j1 {
                        c[(s + j - pos) as usize + 2] += a * self.click_shape[j as usize];
                    }
                }
                self.c_hist = [c[n], c[n + 1]];
                for i in 0..n {
                    dc[i] = c[i + 2] - 2.0 * c[i + 1] + c[i];
                }
            }
            self.resonate::<true>(&lo, &d, &dc, out);
        } else {
            self.resonate::<false>(&[], &[], &[], out);
        }
        if pos < self.noise_end {
            self.noise(out);
        }
        self.pos += n as u32;
    }

    #[inline(always)]
    fn resonate<const EXCITE: bool>(
        &mut self,
        lo: &[f32],
        hi: &[f32],
        click: &[f32],
        out: &mut [f32],
    ) {
        let (b1, b2, gl, gh, gc) = (self.b1, self.b2, self.gl, self.gh, self.gc);
        let (mut y1, mut y2) = (self.y1, self.y2);
        for (i, o) in out.iter_mut().enumerate() {
            let mut y = [0.0f32; MODES];
            for m in 0..MODES {
                y[m] = b1[m] * y1[m] - b2[m] * y2[m];
            }
            if EXCITE {
                let (xl, xh, xc) = (lo[i], hi[i], click[i]);
                for m in 0..MODES {
                    y[m] += gl[m] * xl + gh[m] * xh + gc[m] * xc;
                }
            }
            y2 = y1;
            y1 = y;
            let mut acc = [0.0f32; 4];
            for c in y.as_chunks::<4>().0 {
                for l in 0..4 {
                    acc[l] += c[l];
                }
            }
            *o = (acc[0] + acc[2]) + (acc[1] + acc[3]);
        }
        self.y1 = y1;
        self.y2 = y2;
    }

    #[inline(always)]
    fn noise(&mut self, out: &mut [f32]) {
        let (a, b, amp, mul, step) = (
            self.hp_a,
            self.lp_b,
            self.noise_amp,
            self.env_mul,
            self.att_step,
        );
        let (mut hx, mut hy, mut ly, mut env) = (self.hp_x, self.hp_y, self.lp_y, self.env);
        for (t, o) in (self.pos..).zip(out.iter_mut()) {
            let x = self.noise_rng.bipolar();
            hy = a * (hy + x - hx);
            hx = x;
            ly += b * (hy - ly);
            env = if t < self.att_end {
                env + step
            } else {
                env * mul
            };
            *o += ly * env * amp;
        }
        self.hp_x = hx;
        self.hp_y = hy;
        self.lp_y = ly;
        self.env = env;
    }
}

fn k_weighting(fs: f64) -> ([f64; 5], [f64; 5]) {
    let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
    let a = 10f64.powf(gain_db / 40.0);
    let w = 2.0 * std::f64::consts::PI * f0 / fs;
    let (c, alpha) = (w.cos(), w.sin() / (2.0 * q));
    let sa = 2.0 * a.sqrt() * alpha;
    let a0 = (a + 1.0) - (a - 1.0) * c + sa;
    let shelf = [
        a * ((a + 1.0) + (a - 1.0) * c + sa) / a0,
        -2.0 * a * ((a - 1.0) + (a + 1.0) * c) / a0,
        a * ((a + 1.0) + (a - 1.0) * c - sa) / a0,
        2.0 * ((a - 1.0) - (a + 1.0) * c) / a0,
        ((a + 1.0) - (a - 1.0) * c - sa) / a0,
    ];
    let (f0, q) = (38.13547087602444, 0.5003270373238773);
    let w = 2.0 * std::f64::consts::PI * f0 / fs;
    let (c, alpha) = (w.cos(), w.sin() / (2.0 * q));
    let a0 = 1.0 + alpha;
    let hp = [
        (1.0 + c) / 2.0 / a0,
        -(1.0 + c) / a0,
        (1.0 + c) / 2.0 / a0,
        -2.0 * c / a0,
        (1.0 - alpha) / a0,
    ];
    (shelf, hp)
}

fn biquad(k: &[f64; 5], z: &mut [f64; 4], x: f64) -> f64 {
    let y = k[0] * x + k[1] * z[0] + k[2] * z[1] - k[3] * z[2] - k[4] * z[3];
    *z = [x, z[0], y, z[2]];
    y
}
