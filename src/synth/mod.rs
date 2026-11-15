pub mod layout;
pub mod profile;
pub mod rng;
pub mod wav_out;

use std::f32::consts::PI;

use layout::KeyGeom;
use profile::{Group, Profile};
use rng::{splitmix64, Xorshift32, GOLDEN};

pub const MODES: usize = 24;
pub const VOICES: usize = 24;
pub const KEYS: usize = 128;
pub const LOW: usize = 0;
pub const PLATE: usize = 4;
pub const KEYCAP: usize = 16;
pub const SWITCH: usize = 20;
const MAX_PULSES: usize = 6;
const MAX_SHAPE: usize = 128;
const BLOCK: usize = 128;
const LN_1000: f32 = 6.907_755;
const NEPER_PER_DB: f32 = 0.115_129_25;
const TAIL: f32 = 1.05;
const REFERENCE_KEY: usize = 0;

#[derive(Clone, Copy)]
pub struct KeyModel {
    pub w: [f32; MODES],
    cos: [f32; MODES],
    sin: [f32; MODES],
    norm: [f32; MODES],
    pub lnr: [f32; MODES],
    pub amp: [f32; MODES],
    pan: [f32; 2],
    width: f32,
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
    pan: [1.0; 2],
    width: 4.0,
    sec_gap: 0.0,
    sec_amp: 0.0,
    stab_count: 0,
    noise_amp: 0.0,
};

struct Params {
    var: f32,
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

fn key_model(p: &Profile, patterns: &[[f32; 2]; 12], code: usize, fs: f32) -> KeyModel {
    let seed = splitmix64(p.master.seed ^ (code as u64).wrapping_mul(GOLDEN));
    let mut rng = Xorshift32::new((seed ^ (seed >> 32)) as u32);
    let geom = layout::geometry(code as u8)
        .unwrap_or_else(|| KeyGeom { x: rng.uniform(), y: rng.uniform(), width: 1.0 });
    let large = geom.width >= p.stab.min_width;
    let damping = if matches!(code, 49 | 56 | 60) { p.stab.damping } else { 1.0 };
    let size_f = geom.width.powf(p.master.size_freq_exp);
    let size_d = geom.width.powf(p.master.size_decay_exp);

    let (mut freqs, mut t60, mut gains) = ([0.0; MODES], [0.0; MODES], [0.0; MODES]);
    set_group(&p.low.modes, LOW, &mut freqs, &mut t60, &mut gains);
    set_group(&p.plate.modes, PLATE, &mut freqs, &mut t60, &mut gains);
    set_group(&p.keycap, KEYCAP, &mut freqs, &mut t60, &mut gains);
    set_group(&p.switch, SWITCH, &mut freqs, &mut t60, &mut gains);

    let voice = p.key_voice(code as u32);
    let spread: [f32; 5] = std::array::from_fn(|g| db(rng.gauss() * p.master.key_spread_db + voice.gains_db[g]));
    for m in 0..MODES {
        gains[m] *= spread[group(m)];
        t60[m] *= voice.decay;
    }
    let low_shift = if large { p.low.large_freq / p.low.modes.freqs[0] } else { 1.0 };
    for m in LOW..PLATE {
        freqs[m] *= low_shift * (1.0 + p.low.modes.detune * rng.bipolar());
        if large {
            gains[m] *= db(p.stab.low_db);
        }
    }
    for m in PLATE..KEYCAP {
        freqs[m] *= 1.0 + p.plate.modes.detune * rng.bipolar();
        let [pm, qm] = patterns[m - PLATE];
        let shape = ((PI * pm * geom.x).sin() * (PI * qm * geom.y).sin()).abs();
        gains[m] *= p.plate.floor + (1.0 - p.plate.floor) * shape;
    }
    for m in KEYCAP..SWITCH {
        freqs[m] *= size_f * (1.0 + p.keycap.detune * rng.bipolar());
        t60[m] *= size_d;
    }
    for f in freqs[SWITCH..].iter_mut() {
        *f *= 1.0 + p.switch.detune * rng.bipolar();
    }

    let mut k = EMPTY_MODEL;
    for m in 0..MODES {
        let w = 2.0 * PI * freqs[m].clamp(20.0, 0.45 * fs) / fs;
        k.w[m] = w;
        k.cos[m] = w.cos();
        k.sin[m] = w.sin();
        k.norm[m] = if m < PLATE { w.sin() } else { w.sin() / (4.0 * (0.5 * w).sin().powi(2)) };
        k.lnr[m] = -LN_1000 / (t60[m].max(0.002) * damping * fs);
        k.amp[m] = gains[m];
    }
    let pan = (geom.x - 0.5) * 2.0 * p.master.stereo_width;
    k.pan = [1.0 - pan, 1.0 + pan];
    k.width = rng.range(p.hit.width_ms[0], p.hit.width_ms[1]) * 0.001 * fs;
    k.sec_gap = rng.range(p.hit.sec_gap_ms[0], p.hit.sec_gap_ms[1]) * 0.001 * fs;
    k.sec_amp = db(rng.range(p.hit.sec_level_db[0], p.hit.sec_level_db[1]));
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
    pub fn new(p: &Profile, sample_rate: f64) -> Box<SynthBank> {
        let fs = sample_rate as f32;
        let ms = 0.001 * fs;
        let mut prng = Xorshift32::new((p.master.seed ^ (p.master.seed >> 32)) as u32);
        let patterns: [[f32; 2]; 12] =
            std::array::from_fn(|_| [(1 + prng.next_u32() % 4) as f32, (1 + prng.next_u32() % 4) as f32]);
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
                stab_level: [p.stab.level_db[0] * NEPER_PER_DB, p.stab.level_db[1] * NEPER_PER_DB],
                rel_hardness: r.hardness,
                rel_decay: r.decay,
                rel_gain,
                rel_noise: db(r.noise_db),
                rel_level: db(r.level_db),
                rel_sec_chance: r.sec_chance,
                rel_sec_gap: [r.sec_gap_ms[0] * ms, r.sec_gap_ms[1] * ms],
                rel_sec_amp: db(r.sec_level_db),
                noise_hp: 1.0 / (1.0 + 2.0 * PI * p.noise.hp_hz / fs),
                noise_lp: 1.0 - (-2.0 * PI * p.noise.lp_hz.min(0.45 * fs) / fs).exp(),
                noise_attack: (p.noise.attack_ms * ms).max(1.0),
                noise_lnr: -LN_1000 / (p.noise.t60_ms.max(1.0) * ms),
                drive: 1.0 - (-2.0 * PI * p.low.drive_hz.clamp(10.0, 0.45 * fs) / fs).exp(),
                drive_tail: (LN_1000 * 3.0 * fs / (2.0 * PI * p.low.drive_hz.clamp(10.0, 0.45 * fs))) as u32,
            },
        });
        for (code, k) in bank.keys.iter_mut().enumerate() {
            *k = key_model(p, &patterns, code, fs);
        }
        bank.normalize(p, db(p.master.peak_db), db(p.release.level_db));
        bank
    }

    fn nominal_peak(&mut self, code: usize, release: bool) -> f32 {
        self.params.var = 0.0;
        let mut v = SynthVoice::IDLE;
        v.start(self, code, release, 0, 1);
        self.params.var = 1.0;
        let mut peak = 0.0f32;
        let (mut l, mut r) = ([0.0; BLOCK], [0.0; BLOCK]);
        while v.active() {
            l.fill(0.0);
            r.fill(0.0);
            v.render(&mut l, &mut r);
            peak = l.iter().fold(peak, |a, x| a.max(x.abs()));
        }
        peak
    }

    fn normalize(&mut self, p: &Profile, target: f32, release: f32) {
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
        let scales: Vec<f32> = (0..KEYS)
            .map(|code| target * db(p.key_voice(code as u32).level_db) / self.nominal_peak(code, false).max(1e-9))
            .collect();
        for (k, s) in self.keys.iter_mut().zip(scales) {
            k.scale(s);
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
    pan: [f32; 2],
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
        pan: [0.0; 2],
        pos: 0,
        frames: 0,
    };

    #[inline(always)]
    pub fn active(&self) -> bool {
        self.pos < self.frames
    }

    #[cfg(test)]
    pub fn frames(&self) -> u32 {
        self.frames
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
            (self.gl[m], self.gh[m]) = if m < PLATE { (a, 0.0) } else { (0.0, a) };
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
        self.exc_end = self.pulse_at[..n].iter().max().copied().unwrap_or(0) + len as u32 + 2;
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

        self.pan = k.pan;
        self.pos = 0;
        let modal = if min_lnr < f32::MAX { (LN_1000 * TAIL / min_lnr) as u32 } else { 0 };
        self.frames = modal.max(self.noise_end).max(self.exc_end);
    }

    #[inline]
    pub fn render(&mut self, out_l: &mut [f32], out_r: &mut [f32]) {
        let total = out_l.len().min(out_r.len());
        let mut done = 0;
        while done < total && self.active() {
            let edge = [self.exc_end, self.noise_end, self.frames].into_iter().filter(|&e| e > self.pos).min().unwrap_or(self.frames);
            let n = (total - done).min(BLOCK).min((edge - self.pos) as usize);
            let mut mono = [0.0f32; BLOCK];
            self.render_block(&mut mono[..n]);
            let [pl, pr] = self.pan;
            for ((l, r), s) in out_l[done..done + n].iter_mut().zip(&mut out_r[done..done + n]).zip(&mono[..n]) {
                *l += s * pl;
                *r += s * pr;
            }
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
            self.resonate::<true>(&lo, &d, out);
        } else {
            self.resonate::<false>(&[], &[], out);
        }
        if pos < self.noise_end {
            self.noise(out);
        }
        self.pos += n as u32;
    }

    #[inline(always)]
    fn resonate<const EXCITE: bool>(&mut self, lo: &[f32], hi: &[f32], out: &mut [f32]) {
        let (b1, b2, gl, gh) = (self.b1, self.b2, self.gl, self.gh);
        let (mut y1, mut y2) = (self.y1, self.y2);
        for (i, o) in out.iter_mut().enumerate() {
            let mut y = [0.0f32; MODES];
            for m in 0..MODES {
                y[m] = b1[m] * y1[m] - b2[m] * y2[m];
            }
            if EXCITE {
                let (xl, xh) = (lo[i], hi[i]);
                for m in 0..MODES {
                    y[m] += gl[m] * xl + gh[m] * xh;
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
        let (a, b, amp, mul, step) = (self.hp_a, self.lp_b, self.noise_amp, self.env_mul, self.att_step);
        let (mut hx, mut hy, mut ly, mut env) = (self.hp_x, self.hp_y, self.lp_y, self.env);
        for (t, o) in (self.pos..).zip(out.iter_mut()) {
            let x = self.noise_rng.bipolar();
            hy = a * (hy + x - hx);
            hx = x;
            ly += b * (hy - ly);
            env = if t < self.att_end { env + step } else { env * mul };
            *o += ly * env * amp;
        }
        self.hp_x = hx;
        self.hp_y = hy;
        self.lp_y = ly;
        self.env = env;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;

    fn render(bank: &SynthBank, code: usize, release: bool, velocity: u8, seed: u32, chunk: usize) -> (Vec<f32>, u32) {
        let mut v = SynthVoice::IDLE;
        v.start(bank, code, release, velocity, seed);
        let frames = v.frames();
        let mut l = Vec::new();
        while v.active() {
            let (mut bl, mut br) = (vec![0.0; chunk], vec![0.0; chunk]);
            v.render(&mut bl, &mut br);
            l.extend_from_slice(&bl);
        }
        l.truncate(frames as usize);
        (l, frames)
    }

    fn bits_equal(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
    }

    #[test]
    fn same_key_and_seed_is_bit_identical() {
        let bank = SynthBank::new(&Profile::CREAMY, FS);
        for code in [0, 49, 36, 100] {
            assert!(bits_equal(&render(&bank, code, false, 40, 1234, 128).0, &render(&bank, code, false, 40, 1234, 128).0));
        }
    }

    #[test]
    fn chunk_size_does_not_change_output() {
        let bank = SynthBank::new(&Profile::CREAMY, FS);
        let (a, _) = render(&bank, 5, false, 90, 77, 128);
        for chunk in [1, 32, 37, 512] {
            assert!(bits_equal(&a, &render(&bank, 5, false, 90, 77, chunk).0), "chunk {chunk}");
        }
    }

    #[test]
    fn different_seeds_and_keys_differ() {
        let bank = SynthBank::new(&Profile::CREAMY, FS);
        let diff = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
        let (a, _) = render(&bank, 0, false, 0, 1, 128);
        let (b, _) = render(&bank, 0, false, 0, 2, 128);
        let (c, _) = render(&bank, 1, false, 0, 1, 128);
        assert!(diff(&a, &b) > 1e-3);
        assert!(diff(&a, &c) > 1e-3);
    }

    #[test]
    fn stable_bounded_and_ends_in_time() {
        let bank = SynthBank::new(&Profile::CREAMY, FS);
        for code in 0..KEYS {
            for (release, velocity, seed) in [(false, 0, 9), (false, 127, 10), (true, 64, 11)] {
                let mut v = SynthVoice::IDLE;
                v.start(&bank, code, release, velocity, seed);
                assert!(v.b2.iter().all(|&b2| b2 < 1.0 && b2 > 0.0), "key {code}");
                let (x, frames) = render(&bank, code, release, velocity, seed, 128);
                assert_eq!(x.len(), frames as usize);
                assert!(frames > 0 && f64::from(frames) < 0.5 * FS, "key {code} frames {frames}");
                let peak = x.iter().fold(0.0f32, |a, s| a.max(s.abs()));
                assert!(peak.is_finite() && peak > 1e-3 && peak < 2.0, "key {code} peak {peak}");
                let tail = x[x.len() - 64..].iter().fold(0.0f32, |a, s| a.max(s.abs()));
                assert!(tail < peak * 0.01, "key {code} tail {tail} peak {peak}");
            }
        }
    }

    #[test]
    fn idle_voice_renders_nothing() {
        let mut v = SynthVoice::IDLE;
        let (mut l, mut r) = ([0.0f32; 64], [0.0f32; 64]);
        v.render(&mut l, &mut r);
        assert!(!v.active() && l.iter().chain(&r).all(|s| *s == 0.0));
    }

    #[test]
    fn reference_voice_peaks_at_master_level() {
        let mut bank = SynthBank::new(&Profile::CREAMY, FS);
        let peak = bank.nominal_peak(REFERENCE_KEY, false);
        let target = db(Profile::CREAMY.master.peak_db);
        assert!((peak / target - 1.0).abs() < 1e-3, "{peak} {target}");
    }

    #[test]
    fn single_mode_hits_its_frequency() {
        let mut p = Profile::CREAMY;
        p.low.modes.gains_db = [-200.0; 4];
        p.plate.modes.gains_db = [-200.0; 12];
        p.keycap.gains_db = [-200.0; 4];
        p.switch.gains_db = [-200.0; 4];
        p.noise.level_db = -200.0;
        for m in [LOW + 2, PLATE + 5, KEYCAP + 1, SWITCH] {
            let mut q = p;
            match group(m) {
                0 => q.low.modes.gains_db[m - LOW] = 0.0,
                1 => q.plate.modes.gains_db[m - PLATE] = 0.0,
                2 => q.keycap.gains_db[m - KEYCAP] = 0.0,
                _ => q.switch.gains_db[m - SWITCH] = 0.0,
            }
            let mut bank = SynthBank::new(&q, FS);
            bank.params.var = 0.0;
            let code = 4;
            let expected = f64::from(bank.keys[code].w[m]) * FS / (2.0 * std::f64::consts::PI);
            let (x, _) = render(&bank, code, false, 0, 3, 128);
            let start = (0.015 * FS) as usize;
            let seg = &x[start..start + (0.03 * FS) as usize];
            let crossings: Vec<f64> = seg
                .windows(2)
                .enumerate()
                .filter(|(_, w)| w[0] <= 0.0 && w[1] > 0.0)
                .map(|(i, w)| i as f64 + f64::from(w[0] / (w[0] - w[1])))
                .collect();
            let period = (crossings[crossings.len() - 1] - crossings[0]) / (crossings.len() - 1) as f64;
            let measured = FS / period;
            assert!((measured / expected - 1.0).abs() < 0.005, "mode {m}: {measured} vs {expected}");
        }
    }

    #[test]
    fn velocity_raises_level() {
        let bank = SynthBank::new(&Profile::CREAMY, FS);
        let energy = |v: u8| render(&bank, 3, false, v, 5, 128).0.iter().map(|s| s * s).sum::<f32>();
        assert!(energy(127) > energy(0) * 1.3);
    }

    #[test]
    fn approximations_are_accurate() {
        for i in -20..=20 {
            let x = i as f32 * 0.05;
            assert!((exp_approx(x) / x.exp() - 1.0).abs() < 1e-5, "{x}");
        }
        for i in 0..=100 {
            let u = i as f32 / 100.0;
            assert!((sin_pi(u) - (PI * u).sin()).abs() < 1e-5, "{u}");
        }
        let x = -0.02f32;
        assert!((exp_small(x) - x.exp()).abs() < 1e-8);
    }
}
