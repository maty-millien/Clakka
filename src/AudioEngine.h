#pragma once
#include <AudioToolbox/AudioToolbox.h>
#include <cstdint>
#include <atomic>
#include <unordered_map>
#include <vector>

const int MAX_VOICES = 32;

class AudioEngine {
public:
    enum SoundType {
        PRESS,
        RELEASE,
        COUNT
    };

    AudioEngine();
    ~AudioEngine();

    // Load a sound for a specific key code and type (press/release)
    bool loadKeySound(int keyCode, SoundType type, const char* filePath);
    // Load a fallback sound for cycling when no explicit key sound is set
    bool loadFallbackSound(SoundType type, const char* filePath);
    // Play the sound associated with the given key code and type
    void play(int keyCode, SoundType type);

private:
    AudioUnit audioUnit;
    unsigned int channels;

    // Buffer struct representing loaded WAV data
    struct Buffer {
        float* data;
        uint64_t frameCount;
    };
    // Explicit per-key buffers: for keys with their own sounds
    std::unordered_map<int, Buffer> explicitBuffers[COUNT];
    // Fallback buffer pools per sound type to cycle through
    std::vector<Buffer> fallbackBuffers[COUNT];
    // Map from keyCode to assigned fallback buffer index
    std::unordered_map<int, size_t> keyFallbackMap;
    // Next index to use when assigning fallback buffers per type
    size_t nextFallbackIndex[COUNT];

    std::atomic<uint64_t> playbackPositions[MAX_VOICES];
    // Per-voice buffer pointer and frame count
    float* voiceBuffers[MAX_VOICES];
    uint64_t voiceBufferFrameCounts[MAX_VOICES];
    std::atomic<int> nextVoiceIndex;
    double sampleRate;

    static OSStatus renderCallback(void *inRefCon,
                                   AudioUnitRenderActionFlags *ioActionFlags,
                                   const AudioTimeStamp *inTimeStamp,
                                   UInt32 inBusNumber,
                                   UInt32 inNumberFrames,
                                   AudioBufferList *ioData);
};
