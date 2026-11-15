use std::cell::{Cell, UnsafeCell};
use std::ffi::{c_char, c_int, c_void};
use std::ptr::{self, NonNull};
use std::sync::atomic::{fence, AtomicU64, Ordering};

use crate::synth::profile::Profile;
use crate::synth::rng::splitmix64;
use crate::synth::{SynthBank, SynthVoice, VOICES};

pub const MAX_VOICES: usize = 16;
const BUFFER_FRAMES: u32 = 128;
const QUEUE_CAPACITY: usize = 64;
const KEY_TABLE: usize = 128;
const SYNTH_MAX_FRAMES: u32 = 4096;
pub(crate) const SYNTH_BIT: u64 = 1 << 63;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Wav,
    Synth,
}

#[inline(always)]
pub(crate) fn synth_word(key_code: i32, ty: SoundType, velocity: u8, seed: u32) -> u64 {
    SYNTH_BIT
        | u64::from(seed) << 16
        | u64::from(velocity & 0x7F) << 9
        | (ty as u64) << 8
        | (key_code as u64 & 0xFF)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundType {
    Press = 0,
    Release = 1,
}

impl SoundType {
    fn name(self) -> &'static [u8] {
        match self {
            SoundType::Press => b"press\0",
            SoundType::Release => b"release\0",
        }
    }
}

pub(crate) struct Sound {
    data: Box<[f32]>,
    frames: usize,
    right_offset: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Voice {
    left: *const f32,
    right: *const f32,
    frames: usize,
    pos: usize,
}

const IDLE: Voice = Voice { left: ptr::null(), right: ptr::null(), frames: 0, pos: 0 };

pub(crate) struct RenderState {
    pub(crate) voices: [Voice; MAX_VOICES],
    next_voice: usize,
    synth_voices: [SynthVoice; VOICES],
    next_synth: usize,
    rt_scheduled: bool,
    fp_ready: bool,
    sample_rate: f64,
}

#[repr(align(128))]
pub(crate) struct Padded<T>(pub(crate) T);

pub(crate) struct TriggerQueue {
    pub(crate) slots: [AtomicU64; QUEUE_CAPACITY],
    head: Padded<Cell<usize>>,
    tail: Padded<Cell<usize>>,
}

impl TriggerQueue {
    #[inline]
    fn push(&self, word: u64) -> bool {
        let head = self.head.0.get();
        let slot = &self.slots[head % QUEUE_CAPACITY];
        if slot.load(Ordering::Relaxed) != 0 {
            return false;
        }
        slot.store(word, Ordering::Relaxed);
        self.head.0.set(head.wrapping_add(1));
        true
    }

    #[inline]
    pub(crate) fn drain(&self, mut f: impl FnMut(u64)) {
        let mut tail = self.tail.0.get();
        loop {
            let slot = &self.slots[tail % QUEUE_CAPACITY];
            let word = slot.load(Ordering::Relaxed);
            if word == 0 {
                break;
            }
            slot.store(0, Ordering::Relaxed);
            fence(Ordering::Acquire);
            f(word);
            tail = tail.wrapping_add(1);
        }
        self.tail.0.set(tail);
    }
}

pub(crate) struct Engine {
    pub(crate) queue: TriggerQueue,
    pub(crate) render: UnsafeCell<RenderState>,
    pub(crate) synth: Option<Box<SynthBank>>,
}

pub struct Audio {
    pub(crate) audio_unit: AudioUnit,
    pub(crate) engine: NonNull<Engine>,
    #[allow(clippy::vec_box)]
    sounds: Vec<Box<Sound>>,
    explicit: [[*const Sound; KEY_TABLE]; 2],
    explicit_other: [Vec<(i32, *const Sound)>; 2],
    fallback: [Vec<*const Sound>; 2],
    next_fallback: [Cell<usize>; 2],
    sample_rate: f64,
    initialized: bool,
    timebase: MachTimebaseInfo,
    mode: Mode,
    velocity: Velocity,
}

struct Velocity {
    ms_per_tick: f32,
    window_ms: f32,
    range_ms: f32,
    smoothing: f32,
    counter: Cell<u64>,
    last_press: Cell<u64>,
    avg_ms: Cell<f32>,
    value: Cell<u8>,
}

impl Velocity {
    #[inline(always)]
    fn press(&self, now: u64) {
        let last = self.last_press.replace(now);
        let dt = if last == 0 { 1000.0 } else { (now.wrapping_sub(last) as f32 * self.ms_per_tick).min(1000.0) };
        let avg = self.avg_ms.get() + self.smoothing * (dt - self.avg_ms.get());
        self.avg_ms.set(avg);
        let v = ((self.window_ms - avg) / self.range_ms).clamp(0.0, 1.0);
        self.value.set((v * 127.0 + 0.5) as u8);
    }

    #[inline(always)]
    fn seed(&self, now: u64) -> u32 {
        let c = self.counter.get().wrapping_add(1);
        self.counter.set(c);
        splitmix64(c ^ now) as u32
    }
}

impl Audio {
    pub fn new() -> Self {
        let engine = Box::new(Engine {
            queue: TriggerQueue {
                slots: std::array::from_fn(|_| AtomicU64::new(0)),
                head: Padded(Cell::new(0)),
                tail: Padded(Cell::new(0)),
            },
            render: UnsafeCell::new(RenderState {
                voices: [IDLE; MAX_VOICES],
                next_voice: 0,
                synth_voices: [SynthVoice::IDLE; VOICES],
                next_synth: 0,
                rt_scheduled: false,
                fp_ready: false,
                sample_rate: 0.0,
            }),
            synth: None,
        });
        let mut timebase = MachTimebaseInfo { numer: 0, denom: 0 };
        unsafe { mach_timebase_info(&mut timebase) };
        Audio {
            audio_unit: ptr::null_mut(),
            engine: NonNull::from(Box::leak(engine)),
            sounds: Vec::new(),
            explicit: [[ptr::null(); KEY_TABLE]; 2],
            explicit_other: [Vec::new(), Vec::new()],
            fallback: [Vec::new(), Vec::new()],
            next_fallback: [Cell::new(0), Cell::new(0)],
            sample_rate: 0.0,
            initialized: false,
            velocity: Velocity {
                ms_per_tick: (timebase.numer as f64 / timebase.denom as f64 / 1e6) as f32,
                window_ms: 250.0,
                range_ms: 200.0,
                smoothing: 0.3,
                counter: Cell::new(0),
                last_press: Cell::new(0),
                avg_ms: Cell::new(1000.0),
                value: Cell::new(0),
            },
            timebase,
            mode: Mode::Wav,
        }
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    fn prepare_synth(&mut self, profile: &Profile, rate: f64) {
        let bank = SynthBank::new(profile, rate);
        let v = &mut self.velocity;
        v.window_ms = profile.velocity.window_ms;
        v.range_ms = profile.velocity.range_ms.max(1.0);
        v.smoothing = profile.velocity.smoothing.clamp(0.0, 1.0);
        self.sample_rate = rate;
        unsafe {
            let engine = &mut *self.engine.as_ptr();
            (*engine.render.get()).sample_rate = rate;
            engine.synth = Some(bank);
        }
        fence(Ordering::Release);
        self.mode = Mode::Synth;
    }

    pub fn start_synth(&mut self, profile: &Profile) -> bool {
        if self.initialized {
            return false;
        }
        let Some(device) = default_output_device() else { return false };
        let rate = device_sample_rate(device).unwrap_or(48_000.0);
        self.prepare_synth(profile, rate);
        self.open_unit(device, rate, true)
    }

    pub fn open_offline(&mut self, profile: &Profile, rate: f64) {
        self.prepare_synth(profile, rate);
    }

    #[inline]
    pub fn trigger(&self, key_code: i32, ty: SoundType, velocity: u8, seed: u32) -> bool {
        unsafe { self.engine.as_ref() }.queue.push(synth_word(key_code, ty, velocity, seed))
    }

    pub fn render_offline(&self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len()) as u32;
        let mut list = TwoBuffers {
            number_buffers: 2,
            buffers: [
                AudioBuffer { number_channels: 1, data_byte_size: frames * 4, data: left.as_mut_ptr().cast() },
                AudioBuffer { number_channels: 1, data_byte_size: frames * 4, data: right.as_mut_ptr().cast() },
            ],
        };
        let mut flags = 0u32;
        unsafe {
            render_callback(
                self.engine.as_ptr().cast(),
                &mut flags,
                ptr::null(),
                0,
                frames,
                (&raw mut list).cast::<AudioBufferList>(),
            );
        }
    }

    pub fn load_key_sound(&mut self, key_code: i32, ty: SoundType, path: &str) -> bool {
        let Some((sound, rate)) = load_wav(path) else {
            eprintln!("Error: Unable to load key sound {path}");
            return false;
        };
        let p = self.keep(sound);
        let t = ty as usize;
        match usize::try_from(key_code) {
            Ok(k) if k < KEY_TABLE => self.explicit[t][k] = p,
            _ => match self.explicit_other[t].iter_mut().find(|(k, _)| *k == key_code) {
                Some(slot) => slot.1 = p,
                None => self.explicit_other[t].push((key_code, p)),
            },
        }
        self.initialized || self.init_audio_unit(rate)
    }

    pub fn load_fallback_sound(&mut self, ty: SoundType, path: &str) -> bool {
        let Some((sound, rate)) = load_wav(path) else {
            eprintln!("Error: Unable to load fallback sound {path}");
            return false;
        };
        let p = self.keep(sound);
        self.fallback[ty as usize].push(p);
        self.initialized || self.init_audio_unit(rate)
    }

    fn keep(&mut self, sound: Sound) -> *const Sound {
        let b = Box::new(sound);
        let p: *const Sound = &*b;
        self.sounds.push(b);
        fence(Ordering::Release);
        p
    }

    fn explicit_sound(&self, t: usize, key_code: i32) -> *const Sound {
        match usize::try_from(key_code) {
            Ok(k) if k < KEY_TABLE => self.explicit[t][k],
            _ => self.explicit_other[t]
                .iter()
                .find(|(k, _)| *k == key_code)
                .map_or(ptr::null(), |(_, p)| *p),
        }
    }

    #[inline(always)]
    pub fn play_at(&self, key_code: i32, ty: SoundType, now: u64) -> bool {
        if ty == SoundType::Press {
            self.velocity.press(now);
        }
        let seed = self.velocity.seed(now);
        self.trigger(key_code, ty, self.velocity.value.get(), seed)
    }

    pub fn ms_to_ticks(&self, ms: f64) -> u64 {
        (ms * 1e6 * self.timebase.denom as f64 / self.timebase.numer as f64) as u64
    }

    #[inline]
    pub fn play(&self, key_code: i32, ty: SoundType, event_time: u64) {
        if self.mode == Mode::Synth {
            let now = if event_time != 0 { event_time } else { unsafe { mach_absolute_time() } };
            self.play_at(key_code, ty, now);
            if event_time != 0 {
                self.log_latency(key_code, ty, event_time);
            }
            return;
        }
        let t = ty as usize;
        let mut sound = self.explicit_sound(t, key_code);
        if sound.is_null() {
            let fallback = &self.fallback[t];
            if fallback.is_empty() {
                return;
            }
            let idx = self.next_fallback[t].get() % fallback.len();
            self.next_fallback[t].set((idx + 1) % fallback.len());
            sound = fallback[idx];
        }
        unsafe { self.engine.as_ref() }.queue.push(sound as u64);

        if event_time != 0 {
            self.log_latency(key_code, ty, event_time);
        }
    }

    #[cold]
    fn log_latency(&self, key_code: i32, ty: SoundType, event_time: u64) {
        let now = unsafe { mach_absolute_time() };
        let latency_us = now.wrapping_sub(event_time) as f64 * self.timebase.numer as f64
            / self.timebase.denom as f64
            / 1000.0;
        let buffer_ms = (128.0 / self.sample_rate) * 1000.0;
        let mut buf = [0u8; 160];
        let n = unsafe {
            snprintf(
                buf.as_mut_ptr().cast(),
                buf.len(),
                c"[Latency] key=%d type=%s software=%g\xc2\xb5s +buffer\xe2\x89\x88%gms\n".as_ptr(),
                key_code as c_int,
                ty.name().as_ptr().cast::<c_char>(),
                latency_us,
                buffer_ms,
            )
        };
        if n > 0 {
            let len = (n as usize).min(buf.len() - 1);
            unsafe { write(2, buf.as_ptr().cast(), len) };
        }
    }

    fn init_audio_unit(&mut self, file_sample_rate: f64) -> bool {
        if self.mode == Mode::Synth {
            return false;
        }
        self.sample_rate = file_sample_rate;
        unsafe { (*self.engine.as_ref().render.get()).sample_rate = file_sample_rate };
        let Some(device_id) = default_output_device() else { return false };
        self.open_unit(device_id, file_sample_rate, false)
    }

    fn open_unit(&mut self, device_id: u32, sample_rate: f64, synth: bool) -> bool {
        unsafe {
            let desc = AudioComponentDescription {
                component_type: fourcc(b"auou"),
                component_sub_type: fourcc(b"ahal"),
                component_manufacturer: fourcc(b"appl"),
                component_flags: 0,
                component_flags_mask: 0,
            };
            let component = AudioComponentFindNext(ptr::null_mut(), &desc);
            if component.is_null() {
                eprintln!("Error: Failed to find HAL output audio component");
                return false;
            }

            let status = AudioComponentInstanceNew(component, &mut self.audio_unit);
            if status != 0 {
                eprintln!("Error: Failed to create audio unit instance (OSStatus {status})");
                return false;
            }

            self.set_property(AU_CURRENT_DEVICE, SCOPE_GLOBAL, 0, &device_id);

            let addr = AudioObjectPropertyAddress {
                selector: fourcc(b"fsiz"),
                scope: SCOPE_GLOBAL_OBJECT,
                element: 0,
            };
            AudioObjectSetPropertyData(
                device_id,
                &addr,
                0,
                ptr::null(),
                size_of::<u32>() as u32,
                (&BUFFER_FRAMES as *const u32).cast(),
            );
            let max_frames = if synth {
                get_property::<u32>(device_id, fourcc(b"fsiz")).unwrap_or(BUFFER_FRAMES).max(SYNTH_MAX_FRAMES)
            } else {
                BUFFER_FRAMES
            };
            self.set_property(AU_MAX_FRAMES_PER_SLICE, SCOPE_GLOBAL, 0, &max_frames);

            self.set_property(AU_ENABLE_IO, SCOPE_INPUT, 1, &0u32);
            self.set_property(AU_ENABLE_IO, SCOPE_OUTPUT, 0, &1u32);

            let format = AudioStreamBasicDescription {
                sample_rate,
                format_id: fourcc(b"lpcm"),
                format_flags: 9 | 32,
                bytes_per_packet: 4,
                frames_per_packet: 1,
                bytes_per_frame: 4,
                channels_per_frame: 2,
                bits_per_channel: 32,
                reserved: 0,
            };
            self.set_property(AU_STREAM_FORMAT, SCOPE_INPUT, 0, &format);

            let callback = AURenderCallbackStruct {
                input_proc: render_callback,
                input_proc_ref_con: self.engine.as_ptr().cast(),
            };
            self.set_property(AU_SET_RENDER_CALLBACK, SCOPE_INPUT, 0, &callback);

            let status = AudioUnitInitialize(self.audio_unit);
            if status != 0 {
                eprintln!("Error: Failed to initialize audio unit (OSStatus {status})");
                AudioComponentInstanceDispose(self.audio_unit);
                self.audio_unit = ptr::null_mut();
                return false;
            }

            let status = AudioOutputUnitStart(self.audio_unit);
            if status != 0 {
                eprintln!("Error: Failed to start audio unit (OSStatus {status})");
                AudioUnitUninitialize(self.audio_unit);
                AudioComponentInstanceDispose(self.audio_unit);
                self.audio_unit = ptr::null_mut();
                return false;
            }
        }
        self.initialized = true;
        true
    }

    unsafe fn set_property<T>(&self, id: u32, scope: u32, element: u32, value: &T) -> i32 {
        AudioUnitSetProperty(
            self.audio_unit,
            id,
            scope,
            element,
            (value as *const T).cast(),
            size_of::<T>() as u32,
        )
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        unsafe {
            if !self.audio_unit.is_null() {
                AudioOutputUnitStop(self.audio_unit);
                AudioUnitUninitialize(self.audio_unit);
                AudioComponentInstanceDispose(self.audio_unit);
            }
            drop(Box::from_raw(self.engine.as_ptr()));
        }
    }
}

unsafe fn get_property<T: Copy + Default>(object: u32, selector: u32) -> Option<T> {
    let addr = AudioObjectPropertyAddress { selector, scope: SCOPE_GLOBAL_OBJECT, element: 0 };
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    let status = AudioObjectGetPropertyData(object, &addr, 0, ptr::null(), &mut size, (&raw mut value).cast());
    (status == 0 && size as usize == size_of::<T>()).then_some(value)
}

fn default_output_device() -> Option<u32> {
    let device = unsafe { get_property::<u32>(AUDIO_SYSTEM_OBJECT, fourcc(b"dOut")) };
    if device.is_none() {
        eprintln!("Error: Failed to get default audio output device");
    }
    device
}

fn device_sample_rate(device: u32) -> Option<f64> {
    unsafe { get_property::<f64>(device, fourcc(b"nsrt")) }.filter(|r| *r >= 8000.0)
}

#[inline]
pub(crate) fn enable_flush_to_zero() {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let fpcr: u64;
        std::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
        std::arch::asm!("msr fpcr, {}", in(reg) fpcr | (1 << 24));
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut csr = 0u32;
        std::arch::asm!("stmxcsr [{}]", in(reg) &raw mut csr);
        csr |= 0x8040;
        std::arch::asm!("ldmxcsr [{}]", in(reg) &raw const csr);
    }
}

fn setup_realtime_thread(buffer_frames: u32, sample_rate: f64) {
    let period = ((buffer_frames as f64 / sample_rate) * 1e9) as u32;
    let computation = period / 2;
    let policy = [period, computation, period, 0];
    unsafe {
        let thread = pthread_mach_thread_np(pthread_self());
        thread_policy_set(thread, THREAD_TIME_CONSTRAINT_POLICY, policy.as_ptr(), 4);
    }
}

pub(crate) unsafe extern "C" fn render_callback(
    ref_con: *mut c_void,
    _flags: *mut u32,
    _time: *const c_void,
    _bus: u32,
    frames: u32,
    io: *mut AudioBufferList,
) -> i32 {
    let engine = &*(ref_con as *const Engine);
    let st = &mut *engine.render.get();

    if !st.fp_ready {
        enable_flush_to_zero();
        st.fp_ready = true;
    }
    if !st.rt_scheduled {
        setup_realtime_thread(frames, st.sample_rate);
        st.rt_scheduled = true;
    }

    let synth = engine.synth.as_deref();
    engine.queue.drain(|word| {
        if word & SYNTH_BIT != 0 {
            if let Some(bank) = synth {
                st.synth_voices[st.next_synth].start(
                    bank,
                    (word & 0x7F) as usize,
                    word & 0x100 != 0,
                    ((word >> 9) & 0x7F) as u8,
                    (word >> 16) as u32,
                );
                st.next_synth = (st.next_synth + 1) % VOICES;
            }
            return;
        }
        let s = &*(word as *const Sound);
        let base = s.data.as_ptr();
        st.voices[st.next_voice] = Voice {
            left: base,
            right: base.add(s.right_offset),
            frames: s.frames,
            pos: 0,
        };
        st.next_voice = (st.next_voice + 1) % MAX_VOICES;
    });

    if (*io).number_buffers < 2 {
        return 0;
    }
    let bufs = (&raw mut (*io).buffers).cast::<AudioBuffer>();
    let n = frames as usize;
    let out_l = std::slice::from_raw_parts_mut((*bufs).data.cast::<f32>(), n);
    let out_r = std::slice::from_raw_parts_mut((*bufs.add(1)).data.cast::<f32>(), n);
    mix(st, out_l, out_r);
    0
}

#[inline(always)]
fn mix(st: &mut RenderState, out_l: &mut [f32], out_r: &mut [f32]) {
    out_l.fill(0.0);
    out_r.fill(0.0);
    let mut active = false;
    for v in st.synth_voices.iter_mut() {
        if v.active() {
            active = true;
            v.render(out_l, out_r);
        }
    }
    for v in st.voices.iter_mut() {
        if v.left.is_null() {
            continue;
        }
        active = true;
        let n = out_l.len().min(v.frames - v.pos);
        let (src_l, src_r) = unsafe {
            (
                std::slice::from_raw_parts(v.left.add(v.pos), n),
                std::slice::from_raw_parts(v.right.add(v.pos), n),
            )
        };
        add(&mut out_l[..n], src_l);
        add(&mut out_r[..n], src_r);
        v.pos += n;
        if v.pos >= v.frames {
            *v = IDLE;
        }
    }
    if active {
        clip(out_l);
        clip(out_r);
    }
}

#[inline(always)]
fn add(out: &mut [f32], src: &[f32]) {
    for (o, s) in out.iter_mut().zip(src) {
        *o += *s;
    }
}

#[inline(always)]
fn clip(out: &mut [f32]) {
    for x in out {
        *x = x.clamp(-1.0, 1.0);
    }
}

fn load_wav(path: &str) -> Option<(Sound, f64)> {
    let bytes = std::fs::read(path).ok()?;
    decode_wav(&bytes)
}

pub(crate) fn read_wav_mono(path: &str) -> Option<(Vec<f32>, f64)> {
    let (s, rate) = load_wav(path)?;
    let (l, r) = (&s.data[..s.frames], &s.data[s.right_offset..s.right_offset + s.frames]);
    Some((l.iter().zip(r).map(|(a, b)| 0.5 * (a + b)).collect(), rate))
}

fn decode_wav(b: &[u8]) -> Option<(Sound, f64)> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);

    let mut fmt: Option<(u16, usize, u32, usize)> = None;
    let mut data: Option<&[u8]> = None;
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let size = u32_at(i + 4) as usize;
        let body = i + 8;
        let end = body.saturating_add(size).min(b.len());
        match id {
            b"fmt " if end - body >= 16 => {
                let mut tag = u16_at(body);
                if tag == 0xFFFE && end - body >= 26 {
                    tag = u16_at(body + 24);
                }
                fmt = Some((tag, u16_at(body + 2) as usize, u32_at(body + 4), u16_at(body + 14) as usize));
            }
            b"data" => data = Some(&b[body..end]),
            _ => {}
        }
        i = body.saturating_add(size).saturating_add(size & 1);
    }

    let (tag, channels, rate, bits) = fmt?;
    let data = data?;
    if channels == 0 || bits == 0 || bits % 8 != 0 {
        return None;
    }
    let bps = bits / 8;
    let frame_bytes = bps * channels;
    let frames = data.len() / frame_bytes;
    if frames == 0 {
        return None;
    }
    let data = &data[..frames * frame_bytes];
    let out = match (tag, bps) {
        (1, 1) => planar::<1>(data, channels, |s| s[0] as f32 * (2.0 / 255.0) - 1.0),
        (1, 2) => planar::<2>(data, channels, |s| i16::from_le_bytes(s) as f32 * (1.0 / 32768.0)),
        (1, 3) => planar::<3>(data, channels, |s| {
            let x = (u32::from(s[0]) << 8 | u32::from(s[1]) << 16 | u32::from(s[2]) << 24) as i32 >> 8;
            (x as f64 * 0.000_000_119_209_289_550_781_25) as f32
        }),
        (1, 4) => planar::<4>(data, channels, |s| (i32::from_le_bytes(s) as f64 / 2_147_483_648.0) as f32),
        (3, 4) => planar::<4>(data, channels, f32::from_le_bytes),
        (3, 8) => planar::<8>(data, channels, |s| f64::from_le_bytes(s) as f32),
        _ => return None,
    };
    let right_offset = if channels >= 2 { frames } else { 0 };
    Some((Sound { data: out, frames, right_offset }, rate as f64))
}

fn planar<const B: usize>(data: &[u8], channels: usize, convert: impl Fn([u8; B]) -> f32) -> Box<[f32]> {
    let frames = data.len() / (B * channels);
    let stereo = channels >= 2;
    let mut out = vec![0.0f32; if stereo { frames * 2 } else { frames }].into_boxed_slice();
    let (left, right) = out.split_at_mut(frames);
    let sample = |frame: &[u8], c: usize| convert(frame[c * B..(c + 1) * B].try_into().unwrap());
    for (f, frame) in data.chunks_exact(B * channels).enumerate() {
        left[f] = sample(frame, 0);
        if stereo {
            right[f] = sample(frame, 1);
        }
    }
    out
}

pub(crate) type AudioUnit = *mut c_void;
type AudioComponent = *mut c_void;

const fn fourcc(c: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*c)
}

const AUDIO_SYSTEM_OBJECT: u32 = 1;
const SCOPE_GLOBAL_OBJECT: u32 = fourcc(b"glob");
const SCOPE_GLOBAL: u32 = 0;
const SCOPE_INPUT: u32 = 1;
const SCOPE_OUTPUT: u32 = 2;
const AU_STREAM_FORMAT: u32 = 8;
const AU_MAX_FRAMES_PER_SLICE: u32 = 14;
const AU_SET_RENDER_CALLBACK: u32 = 23;
const AU_CURRENT_DEVICE: u32 = 2000;
const AU_ENABLE_IO: u32 = 2003;
const THREAD_TIME_CONSTRAINT_POLICY: u32 = 2;

#[repr(C)]
struct AudioComponentDescription {
    component_type: u32,
    component_sub_type: u32,
    component_manufacturer: u32,
    component_flags: u32,
    component_flags_mask: u32,
}

#[repr(C)]
struct AudioObjectPropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

#[repr(C)]
struct AudioStreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

type RenderCallback =
    unsafe extern "C" fn(*mut c_void, *mut u32, *const c_void, u32, u32, *mut AudioBufferList) -> i32;

#[repr(C)]
struct AURenderCallbackStruct {
    input_proc: RenderCallback,
    input_proc_ref_con: *mut c_void,
}

#[repr(C)]
pub(crate) struct AudioBuffer {
    pub(crate) number_channels: u32,
    pub(crate) data_byte_size: u32,
    pub(crate) data: *mut c_void,
}

#[repr(C)]
pub(crate) struct AudioBufferList {
    pub(crate) number_buffers: u32,
    pub(crate) buffers: [AudioBuffer; 1],
}

#[repr(C)]
struct TwoBuffers {
    number_buffers: u32,
    buffers: [AudioBuffer; 2],
}

#[repr(C)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

#[link(name = "AudioToolbox", kind = "framework")]
extern "C" {
    fn AudioComponentFindNext(after: AudioComponent, desc: *const AudioComponentDescription) -> AudioComponent;
    fn AudioComponentInstanceNew(component: AudioComponent, out: *mut AudioUnit) -> i32;
    fn AudioComponentInstanceDispose(unit: AudioUnit) -> i32;
    fn AudioUnitSetProperty(unit: AudioUnit, id: u32, scope: u32, element: u32, data: *const c_void, size: u32) -> i32;
    fn AudioUnitInitialize(unit: AudioUnit) -> i32;
    fn AudioUnitUninitialize(unit: AudioUnit) -> i32;
    fn AudioOutputUnitStart(unit: AudioUnit) -> i32;
    pub(crate) fn AudioOutputUnitStop(unit: AudioUnit) -> i32;
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyData(
        object: u32,
        addr: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectSetPropertyData(
        object: u32,
        addr: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> i32;
}

extern "C" {
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> c_int;
    fn pthread_self() -> usize;
    fn pthread_mach_thread_np(thread: usize) -> u32;
    fn thread_policy_set(thread: u32, flavor: u32, info: *const u32, count: u32) -> c_int;
    fn snprintf(buf: *mut c_char, size: usize, fmt: *const c_char, ...) -> c_int;
    fn write(fd: c_int, buf: *const c_void, n: usize) -> isize;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_word_packs_fields() {
        let w = synth_word(49, SoundType::Release, 100, 0xDEAD_BEEF);
        assert_eq!(w >> 63, 1);
        assert_eq!(w & 0xFF, 49);
        assert_eq!((w >> 8) & 1, 1);
        assert_eq!((w >> 9) & 0x7F, 100);
        assert_eq!((w >> 16) as u32, 0xDEAD_BEEF);
        assert_eq!(synth_word(300, SoundType::Press, 255, 0) & 0xFFFF, 300 & 0xFF | 0x7F << 9);
    }

    #[test]
    fn callback_path_matches_direct_voice() {
        let mut audio = Audio::new();
        audio.open_offline(&Profile::CREAMY, 48_000.0);
        assert!(audio.trigger(7, SoundType::Press, 30, 99));
        let (mut l, mut r) = (vec![0.0f32; 4096], vec![0.0f32; 4096]);
        for (cl, cr) in l.chunks_mut(32).zip(r.chunks_mut(32)) {
            audio.render_offline(cl, cr);
        }
        let bank = SynthBank::new(&Profile::CREAMY, 48_000.0);
        let mut v = SynthVoice::IDLE;
        v.start(&bank, 7, false, 30, 99);
        let (mut dl, mut dr) = (vec![0.0f32; 4096], vec![0.0f32; 4096]);
        v.render(&mut dl, &mut dr);
        for x in dl.iter_mut().chain(dr.iter_mut()) {
            *x = x.clamp(-1.0, 1.0);
        }
        assert_eq!(l, dl);
        assert_eq!(r, dr);
        assert!(l.iter().any(|x| x.abs() > 0.01));
    }

    #[test]
    fn velocity_follows_typing_speed() {
        let mut audio = Audio::new();
        audio.open_offline(&Profile::CREAMY, 48_000.0);
        let mut t = audio.ms_to_ticks(1000.0);
        audio.play_at(0, SoundType::Press, t);
        assert_eq!(audio.velocity.value.get(), 0);
        for _ in 0..20 {
            t += audio.ms_to_ticks(60.0);
            audio.play_at(0, SoundType::Press, t);
            audio.render_offline(&mut [0.0; 32], &mut [0.0; 32]);
        }
        assert!(audio.velocity.value.get() > 100);
        for _ in 0..20 {
            t += audio.ms_to_ticks(600.0);
            audio.play_at(0, SoundType::Press, t);
            audio.render_offline(&mut [0.0; 32], &mut [0.0; 32]);
        }
        assert_eq!(audio.velocity.value.get(), 0);
    }
}
