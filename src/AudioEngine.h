#pragma once
#include <AudioToolbox/AudioToolbox.h>
#include <cstdint>
#include <atomic>

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

    bool loadWav(const char* filePath, SoundType type);
    void play(SoundType type);

private:
    AudioUnit audioUnit;
    float* audioBuffers[COUNT];
    uint64_t bufferFrameCounts[COUNT];
    unsigned int channels;

    std::atomic<uint64_t> playbackPositions[MAX_VOICES];
    SoundType voiceSoundTypes[MAX_VOICES];
    std::atomic<int> nextVoiceIndex;

    static OSStatus renderCallback(void *inRefCon,
                                   AudioUnitRenderActionFlags *ioActionFlags,
                                   const AudioTimeStamp *inTimeStamp,
                                   UInt32 inBusNumber,
                                   UInt32 inNumberFrames,
                                   AudioBufferList *ioData);
};
