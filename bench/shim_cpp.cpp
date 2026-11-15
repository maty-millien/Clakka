// Bench shim for the original C++ sources (built from `git archive main src lib`).
// System and standard headers first, then expose private members of the engine
// classes to this translation unit only.
#include <AudioToolbox/AudioToolbox.h>
#include <CoreGraphics/CoreGraphics.h>
#include <atomic>
#include <cstdint>
#include <unordered_map>
#include <vector>
#include <string>
#include <csignal>

#define private public
#include "audio/Audio.h"
#include "input/Input.h"
#include "theme/Theme.h"
#undef private

#include "shim.h"

static Audio *g_audio;
static Input *g_input;

extern "C" void kb_init(void) {
    g_audio = new Audio();
    g_input = new Input(*g_audio);
}

extern "C" void kb_load_themes(void) { Theme::loadThemes(*g_audio, "assets/themes"); }

extern "C" void kb_stop_audio_unit(void) {
    if (g_audio->audioUnit) AudioOutputUnitStop(g_audio->audioUnit);
}

extern "C" void kb_play(int key_code, int type, uint64_t event_time) {
    g_audio->play(key_code, type == 0 ? Audio::PRESS : Audio::RELEASE, event_time);
}

extern "C" void kb_callback(uint32_t type, CGEventRef event) {
    Input::keyboardCallback(nullptr, (CGEventType)type, event, g_input);
}

namespace {
struct TwoBufferList {
    UInt32 mNumberBuffers;
    AudioBuffer mBuffers[2];
};
}

extern "C" void kb_render(uint32_t frames, float *left, float *right) {
    TwoBufferList list;
    list.mNumberBuffers = 2;
    list.mBuffers[0] = {1, (UInt32)(frames * sizeof(float)), left};
    list.mBuffers[1] = {1, (UInt32)(frames * sizeof(float)), right};
    AudioUnitRenderActionFlags flags = 0;
    Audio::renderCallback(g_audio, &flags, nullptr, 0, frames, (AudioBufferList *)&list);
}

extern "C" int kb_input_start(void) {
    try {
        g_input->start();
        return 0;
    } catch (...) {
        return -1;
    }
}

// play() stores position = 0 last (relaxed) into voice nextVoiceIndex % MAX_VOICES.
// With the output unit stopped nothing else writes positions, so position == 0 on
// the expected voice means the seq-th play() is visible to the audio thread.
extern "C" int kb_poll_published(uint64_t seq) {
    return g_audio->voices[seq % MAX_VOICES].position.load(std::memory_order_acquire) == 0;
}

extern "C" void kb_ack(uint64_t seq) {
    g_audio->voices[seq % MAX_VOICES].position.store(VOICE_IDLE, std::memory_order_relaxed);
}
