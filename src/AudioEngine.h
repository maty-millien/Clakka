#pragma once
#include <AudioToolbox/AudioToolbox.h>
#include <cstdint>
#include <vector>
#include <mutex>

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
    std::vector<uint64_t> playbackPositions;
    std::mutex playbackMutex;

    static OSStatus renderCallback(void *inRefCon,
                                   AudioUnitRenderActionFlags *ioActionFlags,
                                   const AudioTimeStamp *inTimeStamp,
                                   UInt32 inBusNumber,
                                   UInt32 inNumberFrames,
                                   AudioBufferList *ioData);
};
