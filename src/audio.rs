//! CoreAudio HAL output unit, sound bank and real-time mixer.
//!
//! Threading: `play()` runs on the event tap thread, the render callback on the
//! CoreAudio IO thread. They share only a ring of single-word mailboxes
//! (`AtomicPtr<Sound>`, null = empty): `play()` stores a sound pointer, the audio
//! thread takes it and starts a voice. All voice state (buffer, length, position)
//! lives on the audio thread, so a voice can never be seen half-written and the
//! audio thread never writes back over a voice that `play()` just started.
//!
//! Memory ordering: a play publishes nothing but the pointer itself, in one atomic
//! word, so the store can be Relaxed (a plain `str`, no barrier on the tap thread).
//! The pointee (decoded samples) is made visible by fence-to-fence synchronization:
//! every load ends with `fence(Release)`, sequenced before any later `play()` store
//! because `Audio` is !Send/!Sync (loads and plays happen on the same thread); the
//! audio thread issues `fence(Acquire)` after reading a non-null pointer and before
//! touching the samples.
//!
//! The render callback does no allocation, locking, syscalls or printing (apart from
//! the one-time real-time policy call on its first invocation, as before).

use std::cell::{Cell, UnsafeCell};
use std::ffi::{c_char, c_int, c_void};
use std::ptr::{self, NonNull};
use std::sync::atomic::{fence, AtomicPtr, Ordering};

pub const MAX_VOICES: usize = 16;
/// Requested device buffer size (~2.9 ms at 44.1 kHz).
const BUFFER_FRAMES: u32 = 128;
/// Pending plays between two render callbacks. Only the last MAX_VOICES of a burst
/// can be audible anyway; this leaves room for 64 key events per buffer.
const QUEUE_CAPACITY: usize = 64;
/// Explicit sounds for key codes below this live in a flat table; macOS virtual
/// key codes are 0..=127.
const KEY_TABLE: usize = 128;

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

/// Decoded sound, planar: left channel then right channel (mono shares one channel).
pub(crate) struct Sound {
    data: Box<[f32]>,
    frames: usize,
    right_offset: usize,
}

/// Audio-thread view of a playing sound. `left` is null when the voice is idle.
#[derive(Clone, Copy)]
pub(crate) struct Voice {
    left: *const f32,
    right: *const f32,
    frames: usize,
    pos: usize,
}

const IDLE: Voice = Voice { left: ptr::null(), right: ptr::null(), frames: 0, pos: 0 };

/// State owned by the audio thread (written by the main thread only before the
/// output unit starts).
pub(crate) struct RenderState {
    pub(crate) voices: [Voice; MAX_VOICES],
    next_voice: usize,
    rt_scheduled: bool,
    sample_rate: f64,
}

/// Keeps producer and consumer state on separate cache lines (128 bytes on Apple silicon).
#[repr(align(128))]
pub(crate) struct Padded<T>(pub(crate) T);

pub(crate) struct TriggerQueue {
    /// Mailboxes, used round-robin by both sides. Null means empty.
    pub(crate) slots: [AtomicPtr<Sound>; QUEUE_CAPACITY],
    /// Next slot to fill. Touched by the producer (tap thread) only.
    head: Padded<Cell<usize>>,
    /// Next slot to take. Touched by the consumer (audio thread) only.
    tail: Padded<Cell<usize>>,
}

impl TriggerQueue {
    /// Producer side. Returns false (and drops the play) if the queue is full.
    #[inline]
    fn push(&self, sound: *const Sound) -> bool {
        let head = self.head.0.get();
        let slot = &self.slots[head % QUEUE_CAPACITY];
        if !slot.load(Ordering::Relaxed).is_null() {
            return false; // the audio thread has not taken this one yet: full
        }
        slot.store(sound.cast_mut(), Ordering::Relaxed);
        self.head.0.set(head.wrapping_add(1));
        true
    }

    /// Consumer side: hands every pending sound to `f`, oldest first.
    #[inline]
    pub(crate) fn drain(&self, mut f: impl FnMut(&Sound)) {
        let mut tail = self.tail.0.get();
        loop {
            let slot = &self.slots[tail % QUEUE_CAPACITY];
            let sound = slot.load(Ordering::Relaxed);
            if sound.is_null() {
                break;
            }
            slot.store(ptr::null_mut(), Ordering::Relaxed);
            // Pairs with the fence(Release) after each load: the samples are visible.
            fence(Ordering::Acquire);
            // SAFETY: sounds outlive the output unit (see Audio::drop).
            f(unsafe { &*sound });
            tail = tail.wrapping_add(1);
        }
        self.tail.0.set(tail);
    }
}

/// Everything the render callback touches; its address is the callback refcon.
pub(crate) struct Engine {
    pub(crate) queue: TriggerQueue,
    pub(crate) render: UnsafeCell<RenderState>,
}

pub struct Audio {
    pub(crate) audio_unit: AudioUnit,
    pub(crate) engine: NonNull<Engine>,
    /// Owns every decoded sound until after the output unit is disposed. Boxed so the
    /// `*const Sound` handed to the audio thread stays valid when the Vec grows.
    #[allow(clippy::vec_box)]
    sounds: Vec<Box<Sound>>,
    explicit: [[*const Sound; KEY_TABLE]; 2],
    explicit_other: [Vec<(i32, *const Sound)>; 2],
    fallback: [Vec<*const Sound>; 2],
    next_fallback: [Cell<usize>; 2],
    sample_rate: f64,
    initialized: bool,
    timebase: MachTimebaseInfo,
}

impl Audio {
    pub fn new() -> Self {
        let engine = Box::new(Engine {
            queue: TriggerQueue {
                slots: std::array::from_fn(|_| AtomicPtr::new(ptr::null_mut())),
                head: Padded(Cell::new(0)),
                tail: Padded(Cell::new(0)),
            },
            render: UnsafeCell::new(RenderState {
                voices: [IDLE; MAX_VOICES],
                next_voice: 0,
                rt_scheduled: false,
                sample_rate: 0.0,
            }),
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
            timebase,
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
        // Publishes the decoded samples to the audio thread (see module docs).
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

    /// Hands a sound to the audio thread. `event_time` (mach ticks) enables the latency log.
    #[inline]
    pub fn play(&self, key_code: i32, ty: SoundType, event_time: u64) {
        let t = ty as usize;
        let mut sound = self.explicit_sound(t, key_code);
        if sound.is_null() {
            let fallback = &self.fallback[t];
            if fallback.is_empty() {
                return;
            }
            // Simple round-robin through fallback sounds
            let idx = self.next_fallback[t].get() % fallback.len();
            self.next_fallback[t].set((idx + 1) % fallback.len());
            sound = fallback[idx];
        }
        // SAFETY: the engine outlives self; Audio is !Sync so this is the only producer.
        unsafe { self.engine.as_ref() }.queue.push(sound);

        // Log latency from key event to voice hand-off
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
        // Buffer latency adds ~128 frames / sampleRate seconds (~2.9ms at 44.1kHz)
        let buffer_ms = (128.0 / self.sample_rate) * 1000.0;
        // Same text as the C++ iostream output (%g = default stream precision), built
        // on the stack and written with a single write(2).
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
        self.sample_rate = file_sample_rate;
        // SAFETY: the output unit is not running (not initialized yet), so the
        // audio thread is not reading the render state.
        unsafe { (*self.engine.as_ref().render.get()).sample_rate = file_sample_rate };
        unsafe {
            let desc = AudioComponentDescription {
                component_type: fourcc(b"auou"),       // kAudioUnitType_Output
                component_sub_type: fourcc(b"ahal"),   // kAudioUnitSubType_HALOutput
                component_manufacturer: fourcc(b"appl"), // kAudioUnitManufacturer_Apple
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

            let mut device_id: u32 = 0;
            let mut size = size_of::<u32>() as u32;
            let mut addr = AudioObjectPropertyAddress {
                selector: fourcc(b"dOut"), // kAudioHardwarePropertyDefaultOutputDevice
                scope: SCOPE_GLOBAL_OBJECT,
                element: 0,
            };
            let status = AudioObjectGetPropertyData(
                AUDIO_SYSTEM_OBJECT,
                &addr,
                0,
                ptr::null(),
                &mut size,
                (&raw mut device_id).cast(),
            );
            if status != 0 {
                eprintln!("Error: Failed to get default audio output device (OSStatus {status})");
                AudioComponentInstanceDispose(self.audio_unit);
                self.audio_unit = ptr::null_mut();
                return false;
            }

            self.set_property(AU_CURRENT_DEVICE, SCOPE_GLOBAL, 0, &device_id);

            // 128 frames (~2.9ms at 44.1kHz) per device buffer.
            addr.selector = fourcc(b"fsiz"); // kAudioDevicePropertyBufferFrameSize
            addr.scope = SCOPE_GLOBAL_OBJECT;
            AudioObjectSetPropertyData(
                device_id,
                &addr,
                0,
                ptr::null(),
                size_of::<u32>() as u32,
                (&BUFFER_FRAMES as *const u32).cast(),
            );
            self.set_property(AU_MAX_FRAMES_PER_SLICE, SCOPE_GLOBAL, 0, &BUFFER_FRAMES);

            self.set_property(AU_ENABLE_IO, SCOPE_INPUT, 1, &0u32);
            self.set_property(AU_ENABLE_IO, SCOPE_OUTPUT, 0, &1u32);

            let format = AudioStreamBasicDescription {
                sample_rate: file_sample_rate,
                format_id: fourcc(b"lpcm"),
                // kAudioFormatFlagsNativeFloatPacked | kAudioFormatFlagIsNonInterleaved
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
            // The render callback can no longer run: free the engine, then the sounds.
            drop(Box::from_raw(self.engine.as_ptr()));
        }
    }
}

/// Applies a time-constraint policy to the calling (audio) thread.
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
    // SAFETY: only this callback touches the render state once the unit runs.
    let st = &mut *engine.render.get();

    // Configure real-time scheduling on the first callback: thread_policy_set applies
    // to the calling thread, so it has to happen here.
    if !st.rt_scheduled {
        setup_realtime_thread(frames, st.sample_rate);
        st.rt_scheduled = true;
    }

    engine.queue.drain(|s| {
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
    mix(&mut st.voices, out_l, out_r);
    0
}

/// Sums every active voice into the outputs and clips to [-1, 1]. Plain loops over
/// contiguous planar data: LLVM vectorizes them (NEON on Apple silicon).
#[inline(always)]
fn mix(voices: &mut [Voice; MAX_VOICES], out_l: &mut [f32], out_r: &mut [f32]) {
    out_l.fill(0.0);
    out_r.fill(0.0);
    let mut active = false;
    for v in voices.iter_mut() {
        if v.left.is_null() {
            continue;
        }
        active = true;
        let n = out_l.len().min(v.frames - v.pos);
        // SAFETY: pos + n <= frames, and both channels hold `frames` samples.
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
    // Silence needs no clipping.
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

// --- WAV decoding -----------------------------------------------------------
//
// Minimal RIFF/WAVE reader covering what dr_wav decodes for uncompressed files:
// integer PCM 8/16/24/32-bit, float 32/64-bit, plain or WAVE_FORMAT_EXTENSIBLE.
// Sample conversion uses dr_wav's exact formulas so the decoded floats are identical.

fn load_wav(path: &str) -> Option<(Sound, f64)> {
    let bytes = std::fs::read(path).ok()?;
    decode_wav(&bytes)
}

fn decode_wav(b: &[u8]) -> Option<(Sound, f64)> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);

    let mut fmt: Option<(u16, usize, u32, usize)> = None; // (tag, channels, rate, bits)
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
                    tag = u16_at(body + 24); // first two bytes of the SubFormat GUID
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
        (1, 1) => planar::<1>(data, channels, |s| s[0] as f32 * (2.0 / 255.0) - 1.0), // 0..255 -> -1..1
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

/// Interleaved samples of `B` bytes -> planar [left..., right...] (left only for mono).
/// Generic over the converter so it inlines into the loop.
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

// --- FFI ----------------------------------------------------------------------

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

/// Variable-length in C: `buffers` has `number_buffers` entries.
#[repr(C)]
pub(crate) struct AudioBufferList {
    pub(crate) number_buffers: u32,
    pub(crate) buffers: [AudioBuffer; 1],
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
