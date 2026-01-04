#pragma once

#include <AudioToolbox/AudioToolbox.h>
#include <vector>
#include <unordered_map>
#include <atomic>
#include <cstdint>

static constexpr int MAX_VOICES = 16;
static constexpr uint64_t VOICE_IDLE = UINT64_MAX;

class Audio {
public:
    enum SoundType {
        PRESS,
        RELEASE,
        COUNT
    };

    Audio();
    ~Audio();

    bool loadKeySound(int keyCode, SoundType type, const char* filePath);
    bool loadFallbackSound(SoundType type, const char* filePath);
    void play(int keyCode, SoundType type, uint64_t eventTime = 0);

private:
    AudioUnit audioUnit;
    unsigned int channels;

    struct Buffer {
        float* data;
        uint64_t frameCount;
    };
    std::unordered_map<int, Buffer> explicitBuffers[COUNT];
    std::vector<Buffer> fallbackBuffers[COUNT];
    size_t nextFallbackIndex[COUNT];

    struct Voice {
        std::atomic<uint64_t> position;
        float* buffer;
        uint64_t frameCount;
    };
    Voice voices[MAX_VOICES];
    std::atomic<int> nextVoiceIndex;
    double sampleRate;
    bool initialized;

    bool initAudioUnit(double fileSampleRate);
    void setupRealtimeThread(UInt32 bufferFrames);

    static OSStatus renderCallback(void *inRefCon,
                                   AudioUnitRenderActionFlags *ioActionFlags,
                                   const AudioTimeStamp *inTimeStamp,
                                   UInt32 inBusNumber,
                                   UInt32 inNumberFrames,
                                   AudioBufferList *ioData);
};
