#pragma once
#include <AudioToolbox/AudioToolbox.h>
#include <cstdint>
#include <atomic>

const int MAX_VOICES = 32;

class AudioEngine {
public:
    AudioEngine();
    ~AudioEngine();

    bool loadWav(const char* filePath);
    void play();

private:
    AudioUnit audioUnit;
    float* audioBuffer;
    uint64_t bufferFrameCount;
    unsigned int channels;

    std::atomic<uint64_t> playbackPositions[MAX_VOICES];
    std::atomic<int> nextVoiceIndex;

    static OSStatus renderCallback(void *inRefCon,
                                   AudioUnitRenderActionFlags *ioActionFlags,
                                   const AudioTimeStamp *inTimeStamp,
                                   UInt32 inBusNumber,
                                   UInt32 inNumberFrames,
                                   AudioBufferList *ioData);
};
