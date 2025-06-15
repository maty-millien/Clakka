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

    bool loadKeySound(int keyCode, SoundType type, const char* filePath);
    bool loadFallbackSound(SoundType type, const char* filePath);
    void play(int keyCode, SoundType type);

private:
    AudioUnit audioUnit;
    unsigned int channels;

    struct Buffer {
        float* data;
        uint64_t frameCount;
    };
    std::unordered_map<int, Buffer> explicitBuffers[COUNT];
    std::vector<Buffer> fallbackBuffers[COUNT];
    std::unordered_map<int, size_t> keyFallbackMap;
    size_t nextFallbackIndex[COUNT];

    std::atomic<uint64_t> playbackPositions[MAX_VOICES];
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
