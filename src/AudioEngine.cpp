#include "AudioEngine.h"
#include <iostream>
#include <CoreAudio/CoreAudio.h>
#include <mach/mach_time.h>
#include <Accelerate/Accelerate.h>
#include <cstring>
#include <pthread.h>
#include <mach/thread_policy.h>
#include <mach/thread_act.h>
#include <vector>

#define DR_WAV_IMPLEMENTATION
#include "dr_wav.h"

OSStatus AudioEngine::renderCallback(void *inRefCon, AudioUnitRenderActionFlags *ioActionFlags, const AudioTimeStamp *inTimeStamp, UInt32 inBusNumber, UInt32 inNumberFrames, AudioBufferList *ioData) {
    static bool rtScheduled = false;
    if (!rtScheduled) {
        AudioEngine* self = static_cast<AudioEngine*>(inRefCon);
        uint32_t period = static_cast<uint32_t>((inNumberFrames / self->sampleRate) * 1e9);
        uint32_t computation = period / 2;
        thread_port_t thread = pthread_mach_thread_np(pthread_self());
        thread_time_constraint_policy_data_t policy = { period, computation, period, 0 };
        thread_policy_set(thread, THREAD_TIME_CONSTRAINT_POLICY, (thread_policy_t)&policy, THREAD_TIME_CONSTRAINT_POLICY_COUNT);
        rtScheduled = true;
    }
    static mach_timebase_info_data_t timebaseInfo;
    static bool timebaseInitialized = false;
    if (!timebaseInitialized) {
        mach_timebase_info(&timebaseInfo);
        timebaseInitialized = true;
    }
    uint64_t now = mach_absolute_time();
    uint64_t hostTime = inTimeStamp->mHostTime;
    uint64_t nowNs = now * timebaseInfo.numer / timebaseInfo.denom;
    uint64_t hostNs = hostTime * timebaseInfo.numer / timebaseInfo.denom;
    int64_t deltaNs = nowNs > hostNs ? nowNs - hostNs : hostNs - nowNs;
    static int latencyPrintCount = 0;
    if (latencyPrintCount < 100) {
        std::cout << "Callback latency: " << (deltaNs / 1000.0) << " us" << std::endl;
        ++latencyPrintCount;
    }

    AudioEngine* self = static_cast<AudioEngine*>(inRefCon);

    float* outL = (float*)ioData->mBuffers[0].mData;
    float* outR = (float*)ioData->mBuffers[1].mData;

    uint64_t localPositions[MAX_VOICES];
    float*   localBuffers[MAX_VOICES];
    uint64_t localFrameCounts[MAX_VOICES];
    for (int v = 0; v < MAX_VOICES; ++v) {
        localPositions[v]    = self->playbackPositions[v].load(std::memory_order_relaxed);
        localBuffers[v]      = self->voiceBuffers[v];
        localFrameCounts[v]  = self->voiceBufferFrameCounts[v];
    }

    memset(outL, 0, sizeof(float) * inNumberFrames);
    memset(outR, 0, sizeof(float) * inNumberFrames);

    for (int v = 0; v < MAX_VOICES; ++v) {
        uint64_t pos      = localPositions[v];
        float*   buffer   = localBuffers[v];
        if (buffer != nullptr) {
            uint64_t total  = localFrameCounts[v];
            UInt32 frames   = inNumberFrames;
            if (pos >= total) continue;
            if (pos + frames > total) frames = static_cast<UInt32>(total - pos);
            if (self->channels == 1) {
                float* src = buffer + pos;
                vDSP_vadd(src, 1, outL, 1, outL, 1, frames);
                vDSP_vadd(src, 1, outR, 1, outR, 1, frames);
            } else {
                float* srcLeft = buffer + pos * self->channels;
                float* srcRight = buffer + pos * self->channels + 1;
                vDSP_vadd(srcLeft, self->channels, outL, 1, outL, 1, frames);
                vDSP_vadd(srcRight, self->channels, outR, 1, outR, 1, frames);
            }
            localPositions[v] = pos + frames;
        }
    }

    float clipMin = -1.0f, clipMax = 1.0f;
    vDSP_vclip(outL, 1, &clipMin, &clipMax, outL, 1, inNumberFrames);
    vDSP_vclip(outR, 1, &clipMin, &clipMax, outR, 1, inNumberFrames);

    for (int v = 0; v < MAX_VOICES; ++v) {
        self->playbackPositions[v].store(localPositions[v], std::memory_order_relaxed);
    }

    return noErr;
}

AudioEngine::AudioEngine() : audioUnit(nullptr), channels(0) {
    for (int i = 0; i < COUNT; ++i) {
        nextFallbackIndex[i] = 0;
    }
    for (int i = 0; i < MAX_VOICES; ++i) {
        playbackPositions[i].store(UINT64_MAX, std::memory_order_relaxed);
        voiceBuffers[i] = nullptr;
        voiceBufferFrameCounts[i] = 0;
    }
    nextVoiceIndex.store(0, std::memory_order_relaxed);

    AudioComponentDescription desc = {};
    desc.componentType = kAudioUnitType_Output;
    desc.componentSubType = kAudioUnitSubType_HALOutput;
    desc.componentFlags = 0;
    desc.componentFlagsMask = 0;
    desc.componentManufacturer = kAudioUnitManufacturer_Apple;

    AudioComponent outputComponent = AudioComponentFindNext(NULL, &desc);
    AudioComponentInstanceNew(outputComponent, &audioUnit);

    AudioDeviceID deviceID = 0;
    UInt32 size = sizeof(deviceID);
    AudioObjectPropertyAddress addr = { kAudioHardwarePropertyDefaultOutputDevice,
                                        kAudioObjectPropertyScopeGlobal,
                                        kAudioObjectPropertyElementMain };
    AudioObjectGetPropertyData(kAudioObjectSystemObject, &addr, 0, nullptr, &size, &deviceID);

    AudioObjectPropertyAddress rateAddr = { kAudioDevicePropertyAvailableNominalSampleRates, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain };
    UInt32 rateSize = 0;
    if (AudioObjectGetPropertyDataSize(deviceID, &rateAddr, 0, nullptr, &rateSize) == noErr && rateSize > 0) {
        int count = rateSize / sizeof(AudioValueRange);
        std::vector<AudioValueRange> ranges(count);
        AudioObjectGetPropertyData(deviceID, &rateAddr, 0, nullptr, &rateSize, ranges.data());
        sampleRate = ranges[0].mMaximum;
        for (auto &r : ranges) {
            if (r.mMaximum > sampleRate) sampleRate = r.mMaximum;
        }
        std::cout << "Using sample rate: " << sampleRate << " Hz" << std::endl;
    } else {
        sampleRate = 44100.0;
    }

    AudioUnitSetProperty(audioUnit,
                         kAudioOutputUnitProperty_CurrentDevice,
                         kAudioUnitScope_Global,
                         0,
                         &deviceID,
                         sizeof(deviceID));

    AudioValueRange bufferSizeRange;
    UInt32 rangeSize = sizeof(bufferSizeRange);
    addr.mSelector = kAudioDevicePropertyBufferFrameSizeRange;
    addr.mScope    = kAudioObjectPropertyScopeGlobal;
    if (AudioObjectGetPropertyData(deviceID, &addr, 0, nullptr, &rangeSize, &bufferSizeRange) == noErr) {
        UInt32 minBufferSize = static_cast<UInt32>(bufferSizeRange.mMinimum);
        std::cout << "Setting HAL buffer size to " << minBufferSize << " frames" << std::endl;
        addr.mSelector = kAudioDevicePropertyBufferFrameSize;
        AudioObjectSetPropertyData(deviceID, &addr, 0, nullptr, sizeof(minBufferSize), &minBufferSize);

        AudioUnitSetProperty(audioUnit,
                            kAudioUnitProperty_MaximumFramesPerSlice,
                            kAudioUnitScope_Global,
                            0,
                            &minBufferSize,
                            sizeof(minBufferSize));
    } else {
        std::cerr << "Warning: Could not get buffer size range, using default 64 frames" << std::endl;
        UInt32 defaultBufferSize = 64;
        addr.mSelector = kAudioDevicePropertyBufferFrameSize;
        AudioObjectSetPropertyData(deviceID, &addr, 0, nullptr, sizeof(defaultBufferSize), &defaultBufferSize);
        AudioUnitSetProperty(audioUnit,
                            kAudioUnitProperty_MaximumFramesPerSlice,
                            kAudioUnitScope_Global,
                            0,
                            &defaultBufferSize,
                            sizeof(defaultBufferSize));
    }

    UInt32 enableIO = 0;
    AudioUnitSetProperty(audioUnit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Input,  1, &enableIO, sizeof(enableIO));
    enableIO = 1;
    AudioUnitSetProperty(audioUnit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Output, 0, &enableIO, sizeof(enableIO));

    AudioStreamBasicDescription streamFormat = {};
    streamFormat.mSampleRate = sampleRate;
    streamFormat.mFormatID = kAudioFormatLinearPCM;
    streamFormat.mFormatFlags = kAudioFormatFlagsNativeFloatPacked | kAudioFormatFlagIsNonInterleaved;
    streamFormat.mBytesPerPacket = sizeof(float);
    streamFormat.mFramesPerPacket = 1;
    streamFormat.mBytesPerFrame = sizeof(float);
    streamFormat.mChannelsPerFrame = 2;
    streamFormat.mBitsPerChannel = 8 * sizeof(float);

    AudioUnitSetProperty(audioUnit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, 0, &streamFormat, sizeof(streamFormat));

    AURenderCallbackStruct callbackStruct = {};
    callbackStruct.inputProc = &AudioEngine::renderCallback;
    callbackStruct.inputProcRefCon = this;
    AudioUnitSetProperty(audioUnit, kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input, 0, &callbackStruct, sizeof(callbackStruct));

    AudioUnitInitialize(audioUnit);
    AudioOutputUnitStart(audioUnit);
}

AudioEngine::~AudioEngine() {
    AudioOutputUnitStop(audioUnit);
    AudioUnitUninitialize(audioUnit);
    AudioComponentInstanceDispose(audioUnit);
    for (int type = 0; type < COUNT; ++type) {
        for (auto &kv : explicitBuffers[type]) {
            drwav_free(kv.second.data, NULL);
        }
        for (auto &buf : fallbackBuffers[type]) {
            drwav_free(buf.data, NULL);
        }
    }
}

bool AudioEngine::loadKeySound(int keyCode, AudioEngine::SoundType type, const char* filePath) {
    unsigned int fileSampleRate;
    drwav_uint64 totalFrameCount;
    float* data = drwav_open_file_and_read_pcm_frames_f32(filePath, &channels, &fileSampleRate, &totalFrameCount, NULL);
    if (data == nullptr) {
        std::cerr << "Error: Unable to load key sound " << filePath << std::endl;
        return false;
    }
    explicitBuffers[type][keyCode] = AudioEngine::Buffer{ data, totalFrameCount };
    return true;
}

bool AudioEngine::loadFallbackSound(AudioEngine::SoundType type, const char* filePath) {
    unsigned int fileSampleRate;
    drwav_uint64 totalFrameCount;
    float* data = drwav_open_file_and_read_pcm_frames_f32(filePath, &channels, &fileSampleRate, &totalFrameCount, NULL);
    if (data == nullptr) {
        std::cerr << "Error: Unable to load fallback sound " << filePath << std::endl;
        return false;
    }
    fallbackBuffers[type].push_back(AudioEngine::Buffer{ data, totalFrameCount });
    return true;
}

void AudioEngine::play(int keyCode, AudioEngine::SoundType type) {
    AudioEngine::Buffer buf;
    auto it = explicitBuffers[type].find(keyCode);
    if (it != explicitBuffers[type].end()) {
        buf = it->second;
    } else if (!fallbackBuffers[type].empty()) {
        size_t idx;
        auto kit = keyFallbackMap.find(keyCode);
        if (kit == keyFallbackMap.end()) {
            idx = nextFallbackIndex[type] % fallbackBuffers[type].size();
            keyFallbackMap[keyCode] = idx;
            nextFallbackIndex[type] = (idx + 1) % fallbackBuffers[type].size();
        } else {
            idx = kit->second;
        }
        buf = fallbackBuffers[type][idx];
    } else {
        return;
    }
    int voiceIndex = nextVoiceIndex.fetch_add(1, std::memory_order_relaxed) % MAX_VOICES;
    voiceBuffers[voiceIndex] = buf.data;
    voiceBufferFrameCounts[voiceIndex] = buf.frameCount;
    playbackPositions[voiceIndex].store(0, std::memory_order_relaxed);
}
