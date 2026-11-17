use std::cell::{Cell, UnsafeCell};
use std::ffi::{c_char, c_int, c_void};
use std::ptr::{self, NonNull};
use std::sync::atomic::{fence, AtomicU64, AtomicUsize, Ordering};

use crate::synth::{splitmix64, SynthBank, SynthVoice, VOICES};
use crate::theme::Theme;

const BUFFER_FRAMES: u32 = 32;
const MAX_FRAMES_PER_SLICE: u32 = 4096;
const DEFAULT_SAMPLE_RATE: f64 = 48_000.0;
const QUEUE_CAPACITY: usize = 64;
const SYNTH_BIT: u64 = 1 << 63;
const LIMIT: f32 = 0.9;
const LIMIT_RELEASE_S: f64 = 0.06;

#[inline(always)]
fn synth_word(key_code: i32, ty: SoundType, velocity: u8, seed: u32) -> u64 {
    SYNTH_BIT
        | u64::from(seed) << 16
        | u64::from(velocity & 0x7F) << 9
        | (ty as u64) << 8
        | (key_code as u64 & 0xFF)
}

#[derive(Clone, Copy, PartialEq)]
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

struct RenderState {
    synth_voices: [SynthVoice; VOICES],
    next_synth: usize,
    fp_ready: bool,
    gain: f32,
    release: f32,
}

#[repr(align(128))]
struct Padded<T>(T);

struct TriggerQueue {
    slots: [AtomicU64; QUEUE_CAPACITY],
    head: Padded<Cell<usize>>,
    tail: Padded<Cell<usize>>,
}

impl TriggerQueue {
    #[inline]
    fn push(&self, word: u64) {
        let head = self.head.0.get();
        let slot = &self.slots[head % QUEUE_CAPACITY];
        if slot.load(Ordering::Relaxed) != 0 {
            return;
        }
        slot.store(word, Ordering::Relaxed);
        self.head.0.set(head.wrapping_add(1));
    }

    #[inline]
    fn drain(&self, mut f: impl FnMut(u64)) {
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

struct Engine {
    queue: TriggerQueue,
    render: UnsafeCell<RenderState>,
    banks: Vec<Box<SynthBank>>,
    current: AtomicUsize,
}

pub struct Audio {
    audio_unit: AudioUnit,
    engine: NonNull<Engine>,
    sample_rate: f64,
    buffer_frames: u32,
    device_frames: u32,
    latency_log: bool,
    timebase: MachTimebaseInfo,
    velocity: Velocity,
    theme_velocity: Vec<[f32; 3]>,
    theme: Cell<usize>,
}

struct Velocity {
    ms_per_tick: f32,
    counter: Cell<u64>,
    last_press: Cell<u64>,
    avg_ms: Cell<f32>,
    value: Cell<u8>,
}

impl Velocity {
    #[inline(always)]
    fn press(&self, now: u64, [window_ms, range_ms, smoothing]: [f32; 3]) {
        let last = self.last_press.replace(now);
        let dt = if last == 0 {
            1000.0
        } else {
            (now.wrapping_sub(last) as f32 * self.ms_per_tick).min(1000.0)
        };
        let avg = self.avg_ms.get() + smoothing * (dt - self.avg_ms.get());
        self.avg_ms.set(avg);
        let v = ((window_ms - avg) / range_ms).clamp(0.0, 1.0);
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
                synth_voices: [SynthVoice::IDLE; VOICES],
                next_synth: 0,
                fp_ready: false,
                gain: 1.0,
                release: 0.0,
            }),
            banks: Vec::new(),
            current: AtomicUsize::new(0),
        });
        let mut timebase = MachTimebaseInfo { numer: 0, denom: 0 };
        unsafe { mach_timebase_info(&mut timebase) };
        Audio {
            audio_unit: ptr::null_mut(),
            engine: NonNull::from(Box::leak(engine)),
            sample_rate: 0.0,
            buffer_frames: 0,
            device_frames: 0,
            latency_log: std::env::var_os("CLAKKA_LATENCY_LOG").is_some_and(|v| v == "1"),
            velocity: Velocity {
                ms_per_tick: (timebase.numer as f64 / timebase.denom as f64 / 1e6) as f32,
                counter: Cell::new(0),
                last_press: Cell::new(0),
                avg_ms: Cell::new(1000.0),
                value: Cell::new(0),
            },
            timebase,
            theme_velocity: Vec::new(),
            theme: Cell::new(0),
        }
    }

    pub fn prepare(&mut self, themes: &[Theme], rate: f64) {
        let banks = themes.iter().map(|p| SynthBank::new(p, rate)).collect();
        self.theme_velocity = themes
            .iter()
            .map(|p| {
                [
                    p.velocity.window_ms,
                    p.velocity.range_ms.max(1.0),
                    p.velocity.smoothing.clamp(0.0, 1.0),
                ]
            })
            .collect();
        self.sample_rate = rate;
        unsafe {
            let engine = &mut *self.engine.as_ptr();
            engine.banks = banks;
            engine.render.get_mut().release =
                (1.0 - (-1.0 / (LIMIT_RELEASE_S * rate)).exp()) as f32;
        }
        fence(Ordering::Release);
        self.set_theme(0);
    }

    pub fn theme_count(&self) -> usize {
        self.theme_velocity.len()
    }

    pub fn theme(&self) -> usize {
        self.theme.get()
    }

    pub fn set_theme(&self, index: usize) {
        let count = self.theme_count();
        if count == 0 {
            return;
        }
        let index = index % count;
        self.theme.set(index);
        unsafe { self.engine.as_ref() }
            .current
            .store(index, Ordering::Relaxed);
    }

    pub fn step_theme(&self, step: isize) {
        let count = self.theme_count() as isize;
        if count > 0 {
            self.set_theme((self.theme.get() as isize + step).rem_euclid(count) as usize);
        }
    }

    pub fn render_offline(&self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        unsafe { process(self.engine.as_ref(), &mut left[..n], &mut right[..n]) };
    }

    #[inline(always)]
    pub fn play_at(&self, key_code: i32, ty: SoundType, now: u64) {
        if ty == SoundType::Press {
            self.velocity
                .press(now, self.theme_velocity[self.theme.get()]);
        }
        let seed = self.velocity.seed(now);
        unsafe { self.engine.as_ref() }.queue.push(synth_word(
            key_code,
            ty,
            self.velocity.value.get(),
            seed,
        ));
    }

    pub fn ms_to_ticks(&self, ms: f64) -> u64 {
        (ms * 1e6 * self.timebase.denom as f64 / self.timebase.numer as f64) as u64
    }

    #[inline]
    pub fn play(&self, key_code: i32, ty: SoundType, event_time: u64) {
        let now = if event_time != 0 {
            event_time
        } else {
            unsafe { mach_absolute_time() }
        };
        self.play_at(key_code, ty, now);
        if self.latency_log && event_time != 0 {
            self.log_latency(key_code, ty, event_time);
        }
    }

    #[cold]
    fn log_latency(&self, key_code: i32, ty: SoundType, event_time: u64) {
        let now = unsafe { mach_absolute_time() };
        let latency_us = now.wrapping_sub(event_time) as f64 * self.timebase.numer as f64
            / self.timebase.denom as f64
            / 1000.0;
        let buffer_ms = self.buffer_frames as f64 * 1000.0 / self.sample_rate;
        let device_ms = self.device_frames as f64 * 1000.0 / self.sample_rate;
        let mut buf = [0u8; 200];
        let n = unsafe {
            snprintf(
                buf.as_mut_ptr().cast(),
                buf.len(),
                c"[Latency] key=%d type=%s software=%g\xc2\xb5s +buffer\xe2\x89\x88%g-%gms +device\xe2\x89\x88%gms (%u frames @ %g Hz)\n"
                    .as_ptr(),
                key_code as c_int,
                ty.name().as_ptr().cast::<c_char>(),
                latency_us,
                buffer_ms,
                2.0 * buffer_ms,
                device_ms,
                self.buffer_frames,
                self.sample_rate,
            )
        };
        if n > 0 {
            let len = (n as usize).min(buf.len() - 1);
            unsafe { write(2, buf.as_ptr().cast(), len) };
        }
    }

    pub fn start(&mut self, themes: &[Theme]) -> Result<(), String> {
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
                return Err("Failed to find HAL output audio component".into());
            }
            check(
                AudioComponentInstanceNew(component, &mut self.audio_unit),
                "create audio unit instance",
            )?;
            let device_id =
                object_property::<u32>(AUDIO_SYSTEM_OBJECT, b"dOut", SCOPE_GLOBAL_OBJECT).map_err(
                    |s| format!("Failed to get default audio output device (OSStatus {s})"),
                )?;

            self.set_property(AU_CURRENT_DEVICE, SCOPE_GLOBAL, 0, &device_id);
            self.sample_rate = object_property::<f64>(device_id, b"nsrt", SCOPE_GLOBAL_OBJECT)
                .ok()
                .filter(|r| *r > 0.0)
                .unwrap_or(DEFAULT_SAMPLE_RATE);
            self.prepare(themes, self.sample_rate);

            let [min, _] = object_property::<[f64; 2]>(device_id, b"fsz#", SCOPE_GLOBAL_OBJECT)
                .unwrap_or([BUFFER_FRAMES as f64; 2]);
            let requested = min.max(1.0) as u32;
            set_object_property(device_id, b"fsiz", SCOPE_GLOBAL_OBJECT, &requested);
            self.buffer_frames = object_property::<u32>(device_id, b"fsiz", SCOPE_GLOBAL_OBJECT)
                .unwrap_or(requested);
            self.set_property(
                AU_MAX_FRAMES_PER_SLICE,
                SCOPE_GLOBAL,
                0,
                &self.buffer_frames.max(MAX_FRAMES_PER_SLICE),
            );
            let stream_latency = object_property::<u32>(device_id, b"stm#", SCOPE_OUTPUT_OBJECT)
                .and_then(|stream| object_property::<u32>(stream, b"ltnc", SCOPE_GLOBAL_OBJECT))
                .unwrap_or(0);
            self.device_frames = object_property::<u32>(device_id, b"ltnc", SCOPE_OUTPUT_OBJECT)
                .unwrap_or(0)
                + object_property::<u32>(device_id, b"saft", SCOPE_OUTPUT_OBJECT).unwrap_or(0)
                + stream_latency;

            self.set_property(AU_ENABLE_IO, SCOPE_INPUT, 1, &0u32);
            self.set_property(AU_ENABLE_IO, SCOPE_OUTPUT, 0, &1u32);

            let format = AudioStreamBasicDescription {
                sample_rate: self.sample_rate,
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

            check(
                AudioUnitInitialize(self.audio_unit),
                "initialize audio unit",
            )?;
            check(AudioOutputUnitStart(self.audio_unit), "start audio unit")?;
            self.buffer_frames = object_property::<u32>(device_id, b"fsiz", SCOPE_GLOBAL_OBJECT)
                .unwrap_or(self.buffer_frames);
        }
        println!(
            "Audio output: {} Hz, {}-frame buffer, {} frames of device latency",
            self.sample_rate, self.buffer_frames, self.device_frames
        );
        Ok(())
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

#[inline]
fn enable_flush_to_zero() {
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

unsafe extern "C" fn render_callback(
    ref_con: *mut c_void,
    _flags: *mut u32,
    _time: *const c_void,
    _bus: u32,
    frames: u32,
    io: *mut AudioBufferList,
) -> i32 {
    if (*io).number_buffers < 2 {
        return 0;
    }
    let bufs = (&raw mut (*io).buffers).cast::<AudioBuffer>();
    let n = (frames as usize)
        .min((*bufs).data_byte_size as usize / 4)
        .min((*bufs.add(1)).data_byte_size as usize / 4);
    process(
        &*(ref_con as *const Engine),
        std::slice::from_raw_parts_mut((*bufs).data.cast::<f32>(), n),
        std::slice::from_raw_parts_mut((*bufs.add(1)).data.cast::<f32>(), n),
    );
    0
}

#[inline(always)]
unsafe fn process(engine: &Engine, out_l: &mut [f32], out_r: &mut [f32]) {
    let st = &mut *engine.render.get();

    if !st.fp_ready {
        enable_flush_to_zero();
        st.fp_ready = true;
    }

    let synth = engine
        .banks
        .get(engine.current.load(Ordering::Relaxed))
        .map(|b| &**b);
    engine.queue.drain(|word| {
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
    });
    mix(st, out_l, out_r);
}

fn check(status: i32, what: &str) -> Result<(), String> {
    match status {
        0 => Ok(()),
        _ => Err(format!("Failed to {what} (OSStatus {status})")),
    }
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
    if active {
        limit(st, out_l, out_r);
    } else {
        st.gain = 1.0;
    }
}

#[inline(always)]
fn limit(st: &mut RenderState, out_l: &mut [f32], out_r: &mut [f32]) {
    let (release, mut g) = (st.release, st.gain);
    for (l, r) in out_l.iter_mut().zip(out_r.iter_mut()) {
        g += release * (1.0 - g);
        let peak = l.abs().max(r.abs());
        if peak * g > LIMIT {
            g = LIMIT / peak;
        }
        *l *= g;
        *r *= g;
    }
    st.gain = g;
}

unsafe fn object_property<T>(object: u32, selector: &[u8; 4], scope: u32) -> Result<T, i32> {
    let addr = AudioObjectPropertyAddress {
        selector: fourcc(selector),
        scope,
        element: 0,
    };
    let mut value = std::mem::MaybeUninit::<T>::zeroed();
    let mut size = size_of::<T>() as u32;
    match AudioObjectGetPropertyData(
        object,
        &addr,
        0,
        ptr::null(),
        &mut size,
        value.as_mut_ptr().cast(),
    ) {
        0 if size as usize == size_of::<T>() => Ok(value.assume_init()),
        0 => Err(-1),
        status => Err(status),
    }
}

unsafe fn set_object_property<T>(object: u32, selector: &[u8; 4], scope: u32, value: &T) -> i32 {
    let addr = AudioObjectPropertyAddress {
        selector: fourcc(selector),
        scope,
        element: 0,
    };
    AudioObjectSetPropertyData(
        object,
        &addr,
        0,
        ptr::null(),
        size_of::<T>() as u32,
        (value as *const T).cast(),
    )
}

type AudioUnit = *mut c_void;
type AudioComponent = *mut c_void;

const fn fourcc(c: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*c)
}

const AUDIO_SYSTEM_OBJECT: u32 = 1;
const SCOPE_GLOBAL_OBJECT: u32 = fourcc(b"glob");
const SCOPE_OUTPUT_OBJECT: u32 = fourcc(b"outp");
const SCOPE_GLOBAL: u32 = 0;
const SCOPE_INPUT: u32 = 1;
const SCOPE_OUTPUT: u32 = 2;
const AU_STREAM_FORMAT: u32 = 8;
const AU_MAX_FRAMES_PER_SLICE: u32 = 14;
const AU_SET_RENDER_CALLBACK: u32 = 23;
const AU_CURRENT_DEVICE: u32 = 2000;
const AU_ENABLE_IO: u32 = 2003;

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

type RenderCallback = unsafe extern "C" fn(
    *mut c_void,
    *mut u32,
    *const c_void,
    u32,
    u32,
    *mut AudioBufferList,
) -> i32;

#[repr(C)]
struct AURenderCallbackStruct {
    input_proc: RenderCallback,
    input_proc_ref_con: *mut c_void,
}

#[repr(C)]
struct AudioBuffer {
    number_channels: u32,
    data_byte_size: u32,
    data: *mut c_void,
}

#[repr(C)]
struct AudioBufferList {
    number_buffers: u32,
    buffers: [AudioBuffer; 1],
}

#[repr(C)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

#[link(name = "AudioToolbox", kind = "framework")]
extern "C" {
    fn AudioComponentFindNext(
        after: AudioComponent,
        desc: *const AudioComponentDescription,
    ) -> AudioComponent;
    fn AudioComponentInstanceNew(component: AudioComponent, out: *mut AudioUnit) -> i32;
    fn AudioComponentInstanceDispose(unit: AudioUnit) -> i32;
    fn AudioUnitSetProperty(
        unit: AudioUnit,
        id: u32,
        scope: u32,
        element: u32,
        data: *const c_void,
        size: u32,
    ) -> i32;
    fn AudioUnitInitialize(unit: AudioUnit) -> i32;
    fn AudioUnitUninitialize(unit: AudioUnit) -> i32;
    fn AudioOutputUnitStart(unit: AudioUnit) -> i32;
    fn AudioOutputUnitStop(unit: AudioUnit) -> i32;
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
    fn snprintf(buf: *mut c_char, size: usize, fmt: *const c_char, ...) -> c_int;
    fn write(fd: c_int, buf: *const c_void, n: usize) -> isize;
}
