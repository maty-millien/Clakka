// C ABI implemented once per engine version (shim_cpp.cpp for the C++ sources,
// shim-rs/ for the Rust sources). bench.c only talks to the engine through it,
// so both versions are timed by exactly the same driver code.
#pragma once

#include <stdint.h>
#include <CoreGraphics/CoreGraphics.h>

#ifdef __cplusplus
extern "C" {
#endif

// Construct Audio and Input (tap not installed yet).
void kb_init(void);
// Theme::loadThemes(audio, "assets/themes"): decodes all WAVs, inits and starts the AudioUnit.
void kb_load_themes(void);
// Stop the output unit so the benchmark is the only caller of the render callback.
void kb_stop_audio_unit(void);
// Audio::play(keyCode, type (0 press, 1 release), eventTime). eventTime 0 disables the log.
void kb_play(int key_code, int type, uint64_t event_time);
// Call the event tap callback directly with a synthetic event.
void kb_callback(uint32_t type, CGEventRef event);
// Call the render callback for `frames` frames into two planar output buffers.
void kb_render(uint32_t frames, float *left, float *right);
// Install the real event tap on the current run loop. Returns 0 on success.
int kb_input_start(void);
// Non-zero once the seq-th play() since kb_init has been published to the audio thread.
int kb_poll_published(uint64_t seq);
// Consume the seq-th play() the way the audio thread would (render is stopped in tap mode).
void kb_ack(uint64_t seq);

#ifdef __cplusplus
}
#endif
