#include <iostream>
#include <cstring>
#include <CoreAudio/CoreAudio.h>
#include <mach/mach_time.h>
#include <Accelerate/Accelerate.h>
#include <pthread.h>
#include <mach/thread_policy.h>
#include <mach/thread_act.h>

#include "Audio.h"

#define DR_WAV_IMPLEMENTATION
#include "dr_wav.h"

OSStatus Audio::renderCallback(void *inRefCon, AudioUnitRenderActionFlags *, const AudioTimeStamp *, UInt32, UInt32 inNumberFrames, AudioBufferList *ioData) {
    Audio* self = static_cast<Audio*>(inRefCon);

    // Configure real-time thread scheduling on first callback invocation.
    // This must happen on the audio thread (not during initAudioUnit) because
    // thread_policy_set applies to the calling thread.
    static bool rtScheduled = false;
    if (!rtScheduled) {
        self->setupRealtimeThread(inNumberFrames);
        rtScheduled = true;
    }

    float* outL = (float*)ioData->mBuffers[0].mData;
    float* outR = (float*)ioData->mBuffers[1].mData;

    // Cache atomic values locally to avoid repeated atomic loads in the tight mixing loop.
    // Each atomic load has memory ordering overhead, so we load once per voice and work
    // with the local copy throughout the render callback.
    uint64_t localPositions[MAX_VOICES];
    float*   localBuffers[MAX_VOICES];
    uint64_t localFrameCounts[MAX_VOICES];
    for (int v = 0; v < MAX_VOICES; ++v) {
        localPositions[v]    = self->voices[v].position.load(std::memory_order_relaxed);
        localBuffers[v]      = self->voices[v].buffer;
        localFrameCounts[v]  = self->voices[v].frameCount;
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
        self->voices[v].position.store(localPositions[v], std::memory_order_relaxed);
    }

    return noErr;
}

Audio::Audio() : audioUnit(nullptr), channels(0), initialized(false) {
    for (int i = 0; i < COUNT; ++i) {
        nextFallbackIndex[i] = 0;
    }
    for (int i = 0; i < MAX_VOICES; ++i) {
        voices[i].position.store(VOICE_IDLE, std::memory_order_relaxed);
        voices[i].buffer = nullptr;
        voices[i].frameCount = 0;
    }
    nextVoiceIndex.store(0, std::memory_order_relaxed);
}

Audio::~Audio() {
    if (audioUnit) {
        AudioOutputUnitStop(audioUnit);
        AudioUnitUninitialize(audioUnit);
        AudioComponentInstanceDispose(audioUnit);
    }
    for (int type = 0; type < COUNT; ++type) {
        for (auto &kv : explicitBuffers[type]) {
            drwav_free(kv.second.data, NULL);
        }
        for (auto &buf : fallbackBuffers[type]) {
            drwav_free(buf.data, NULL);
        }
    }
}

void Audio::setupRealtimeThread(UInt32 bufferFrames) {
    uint32_t period = static_cast<uint32_t>((bufferFrames / sampleRate) * 1e9);
    uint32_t computation = period / 2;
    thread_port_t thread = pthread_mach_thread_np(pthread_self());
    thread_time_constraint_policy_data_t policy = { period, computation, period, 0 };
    thread_policy_set(thread, THREAD_TIME_CONSTRAINT_POLICY, (thread_policy_t)&policy, THREAD_TIME_CONSTRAINT_POLICY_COUNT);
}

bool Audio::initAudioUnit(double fileSampleRate) {
    sampleRate = fileSampleRate;
    AudioComponentDescription desc = {};
    desc.componentType = kAudioUnitType_Output;
    desc.componentSubType = kAudioUnitSubType_HALOutput;
    desc.componentFlags = 0;
    desc.componentFlagsMask = 0;
    desc.componentManufacturer = kAudioUnitManufacturer_Apple;

    AudioComponent outputComponent = AudioComponentFindNext(nullptr, &desc);
    if (!outputComponent) {
        std::cerr << "Error: Failed to find HAL output audio component" << std::endl;
        return false;
    }

    OSStatus status = AudioComponentInstanceNew(outputComponent, &audioUnit);
    if (status != noErr) {
        std::cerr << "Error: Failed to create audio unit instance (OSStatus " << status << ")" << std::endl;
        return false;
    }

    AudioDeviceID deviceID = 0;
    UInt32 size = sizeof(deviceID);
    AudioObjectPropertyAddress addr = { kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain };
    status = AudioObjectGetPropertyData(kAudioObjectSystemObject, &addr, 0, nullptr, &size, &deviceID);
    if (status != noErr) {
        std::cerr << "Error: Failed to get default audio output device (OSStatus " << status << ")" << std::endl;
        AudioComponentInstanceDispose(audioUnit);
        audioUnit = nullptr;
        return false;
    }

    AudioUnitSetProperty(audioUnit,
                         kAudioOutputUnitProperty_CurrentDevice,
                         kAudioUnitScope_Global,
                         0,
                         &deviceID,
                         sizeof(deviceID));

    // Use a reasonable default buffer size. A keyboard audio engine doesn't need
    // ultra-low latency optimization - 128 frames (~2.7ms at 48kHz) is sufficient.
    static constexpr UInt32 DEFAULT_BUFFER_SIZE = 128;
    addr.mSelector = kAudioDevicePropertyBufferFrameSize;
    addr.mScope = kAudioObjectPropertyScopeGlobal;
    AudioObjectSetPropertyData(deviceID, &addr, 0, nullptr, sizeof(DEFAULT_BUFFER_SIZE), &DEFAULT_BUFFER_SIZE);
    AudioUnitSetProperty(audioUnit,
                        kAudioUnitProperty_MaximumFramesPerSlice,
                        kAudioUnitScope_Global,
                        0,
                        &DEFAULT_BUFFER_SIZE,
                        sizeof(DEFAULT_BUFFER_SIZE));

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

    AudioUnitSetProperty(audioUnit,
                         kAudioUnitProperty_StreamFormat,
                         kAudioUnitScope_Input,
                         0,
                         &streamFormat,
                         sizeof(streamFormat));

    AURenderCallbackStruct callbackStruct = {};
    callbackStruct.inputProc = &Audio::renderCallback;
    callbackStruct.inputProcRefCon = this;
    AudioUnitSetProperty(audioUnit,
                         kAudioUnitProperty_SetRenderCallback,
                         kAudioUnitScope_Input,
                         0,
                         &callbackStruct,
                         sizeof(callbackStruct));

    status = AudioUnitInitialize(audioUnit);
    if (status != noErr) {
        std::cerr << "Error: Failed to initialize audio unit (OSStatus " << status << ")" << std::endl;
        AudioComponentInstanceDispose(audioUnit);
        audioUnit = nullptr;
        return false;
    }

    status = AudioOutputUnitStart(audioUnit);
    if (status != noErr) {
        std::cerr << "Error: Failed to start audio unit (OSStatus " << status << ")" << std::endl;
        AudioUnitUninitialize(audioUnit);
        AudioComponentInstanceDispose(audioUnit);
        audioUnit = nullptr;
        return false;
    }

    initialized = true;
    return true;
}

bool Audio::loadKeySound(int keyCode, Audio::SoundType type, const char* filePath) {
    unsigned int fileSampleRate;
    drwav_uint64 totalFrameCount;
    float* data = drwav_open_file_and_read_pcm_frames_f32(filePath, &channels, &fileSampleRate, &totalFrameCount, NULL);
    if (data == nullptr) {
        std::cerr << "Error: Unable to load key sound " << filePath << std::endl;
        return false;
    }
    explicitBuffers[type][keyCode] = Audio::Buffer{ data, totalFrameCount };
    if (!initialized) {
        if (!initAudioUnit(fileSampleRate)) {
            return false;
        }
    }
    return true;
}

bool Audio::loadFallbackSound(Audio::SoundType type, const char* filePath) {
    unsigned int fileSampleRate;
    drwav_uint64 totalFrameCount;
    float* data = drwav_open_file_and_read_pcm_frames_f32(filePath, &channels, &fileSampleRate, &totalFrameCount, NULL);
    if (data == nullptr) {
        std::cerr << "Error: Unable to load fallback sound " << filePath << std::endl;
        return false;
    }
    fallbackBuffers[type].push_back(Audio::Buffer{ data, totalFrameCount });
    if (!initialized) {
        if (!initAudioUnit(fileSampleRate)) {
            return false;
        }
    }
    return true;
}

void Audio::play(int keyCode, Audio::SoundType type) {
    Audio::Buffer buf;
    auto it = explicitBuffers[type].find(keyCode);
    if (it != explicitBuffers[type].end()) {
        buf = it->second;
    } else if (!fallbackBuffers[type].empty()) {
        // Simple round-robin through fallback sounds
        size_t idx = nextFallbackIndex[type] % fallbackBuffers[type].size();
        nextFallbackIndex[type] = (idx + 1) % fallbackBuffers[type].size();
        buf = fallbackBuffers[type][idx];
    } else {
        return;
    }
    int voiceIndex = nextVoiceIndex.fetch_add(1, std::memory_order_relaxed) % MAX_VOICES;
    voices[voiceIndex].buffer = buf.data;
    voices[voiceIndex].frameCount = buf.frameCount;
    voices[voiceIndex].position.store(0, std::memory_order_relaxed);
}
