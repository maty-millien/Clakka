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
    // Real-time scheduling setup (once)
    static bool rtScheduled = false;
    if (!rtScheduled) {
        AudioEngine* self = static_cast<AudioEngine*>(inRefCon);
        // Period in nanoseconds
        uint32_t period = static_cast<uint32_t>((inNumberFrames / self->sampleRate) * 1e9);
        // Budget half of period
        uint32_t computation = period / 2;
        thread_port_t thread = pthread_mach_thread_np(pthread_self());
        thread_time_constraint_policy_data_t policy = { period, computation, period, 0 };
        thread_policy_set(thread, THREAD_TIME_CONSTRAINT_POLICY, (thread_policy_t)&policy, THREAD_TIME_CONSTRAINT_POLICY_COUNT);
        rtScheduled = true;
    }
    // Instrumentation for latency measurement
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
    // Print latency for the first 100 callbacks to minimize overhead
    static int latencyPrintCount = 0;
    if (latencyPrintCount < 100) {
        std::cout << "Callback latency: " << (deltaNs / 1000.0) << " us" << std::endl;
        ++latencyPrintCount;
    }

    AudioEngine* self = static_cast<AudioEngine*>(inRefCon);

    float* outL = (float*)ioData->mBuffers[0].mData;
    float* outR = (float*)ioData->mBuffers[1].mData;

    // Copy atomic playback positions and voice types into local arrays
    uint64_t localPositions[MAX_VOICES];
    SoundType localTypes[MAX_VOICES];
    for (int v = 0; v < MAX_VOICES; ++v) {
        localPositions[v] = self->playbackPositions[v].load(std::memory_order_relaxed);
        localTypes[v] = self->voiceSoundTypes[v];
    }

    // Zero output buffers
    memset(outL, 0, sizeof(float) * inNumberFrames);
    memset(outR, 0, sizeof(float) * inNumberFrames);

    // Vectorized mixing across voices using Accelerate
    for (int v = 0; v < MAX_VOICES; ++v) {
        uint64_t pos = localPositions[v];
        SoundType type = localTypes[v];
        if (type < COUNT && self->audioBuffers[type] != nullptr) {
            uint64_t total = self->bufferFrameCounts[type];
            UInt32 frames = inNumberFrames;
            if (pos >= total) continue;
            if (pos + frames > total) frames = static_cast<UInt32>(total - pos);
            float* buffer = self->audioBuffers[type];
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

    // Clamp outputs between -1 and 1
    float clipMin = -1.0f, clipMax = 1.0f;
    vDSP_vclip(outL, 1, &clipMin, &clipMax, outL, 1, inNumberFrames);
    vDSP_vclip(outR, 1, &clipMin, &clipMax, outR, 1, inNumberFrames);

    // Store updated positions back to atomics
    for (int v = 0; v < MAX_VOICES; ++v) {
        self->playbackPositions[v].store(localPositions[v], std::memory_order_relaxed);
    }

    return noErr;
}

AudioEngine::AudioEngine() : audioUnit(nullptr), channels(0) {
    for (int i = 0; i < COUNT; ++i) {
        audioBuffers[i] = nullptr;
        bufferFrameCounts[i] = 0;
    }
    for (int i = 0; i < MAX_VOICES; ++i) {
        playbackPositions[i].store(UINT64_MAX, std::memory_order_relaxed);
        voiceSoundTypes[i] = PRESS;
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

    // Configure the HAL output unit and underlying device for the smallest practical latency.

    // 1. Retrieve the default output device ID.
    AudioDeviceID deviceID = 0;
    UInt32 size = sizeof(deviceID);
    AudioObjectPropertyAddress addr = { kAudioHardwarePropertyDefaultOutputDevice,
                                        kAudioObjectPropertyScopeGlobal,
                                        kAudioObjectPropertyElementMain };
    AudioObjectGetPropertyData(kAudioObjectSystemObject, &addr, 0, nullptr, &size, &deviceID);

    // 1.1 Query available sample rates and pick the highest for lowest period
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

    // 2. Tell the AudioUnit to use this hardware device.
    AudioUnitSetProperty(audioUnit,
                         kAudioOutputUnitProperty_CurrentDevice,
                         kAudioUnitScope_Global,
                         0,
                         &deviceID,
                         sizeof(deviceID));

    // 3. Query and set the hardware buffer size to the minimum supported frames to reduce latency.
    AudioValueRange bufferSizeRange;
    UInt32 rangeSize = sizeof(bufferSizeRange);
    addr.mSelector = kAudioDevicePropertyBufferFrameSizeRange;
    addr.mScope    = kAudioObjectPropertyScopeGlobal;
    if (AudioObjectGetPropertyData(deviceID, &addr, 0, nullptr, &rangeSize, &bufferSizeRange) == noErr) {
        UInt32 minBufferSize = static_cast<UInt32>(bufferSizeRange.mMinimum);
        std::cout << "Setting HAL buffer size to " << minBufferSize << " frames" << std::endl;
        addr.mSelector = kAudioDevicePropertyBufferFrameSize;
        AudioObjectSetPropertyData(deviceID, &addr, 0, nullptr, sizeof(minBufferSize), &minBufferSize);

        // 4. Let the AudioUnit know about our desired maximum slice size so Core Audio can
        //    schedule smaller render cycles.
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

    // 5. Disable the input bus and ensure the output bus is enabled (typical for a playback-only unit).
    UInt32 enableIO = 0;
    AudioUnitSetProperty(audioUnit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Input,  1, &enableIO, sizeof(enableIO));
    enableIO = 1;
    AudioUnitSetProperty(audioUnit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Output, 0, &enableIO, sizeof(enableIO));

    // NOTE: We will call AudioUnitInitialize later, after setting the stream format and callback.

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

    // Now that all properties are configured, initialise and start the unit.
    AudioUnitInitialize(audioUnit);
    AudioOutputUnitStart(audioUnit);
}

AudioEngine::~AudioEngine() {
    AudioOutputUnitStop(audioUnit);
    AudioUnitUninitialize(audioUnit);
    AudioComponentInstanceDispose(audioUnit);
    for (int i = 0; i < COUNT; ++i) {
        if (audioBuffers[i]) {
            drwav_free(audioBuffers[i], NULL);
        }
    }
}

bool AudioEngine::loadWav(const char* filePath, SoundType type) {
    unsigned int sampleRate;
    drwav_uint64 totalFrameCount;

    if (audioBuffers[type]) {
        drwav_free(audioBuffers[type], NULL);
        audioBuffers[type] = nullptr;
    }

    audioBuffers[type] = drwav_open_file_and_read_pcm_frames_f32(filePath, &channels, &sampleRate, &totalFrameCount, NULL);

    if (audioBuffers[type] == nullptr) {
        std::cerr << "Erreur: Impossible de charger le fichier WAV " << filePath << std::endl;
        return false;
    }

    bufferFrameCounts[type] = totalFrameCount;
    std::cout << "Fichier " << filePath << " chargé. " << bufferFrameCounts[type] << " frames à " << sampleRate << " Hz." << std::endl;
    AudioOutputUnitStop(audioUnit);
    AudioUnitUninitialize(audioUnit);
    AudioStreamBasicDescription streamFormat = {};
    streamFormat.mSampleRate = static_cast<Float64>(sampleRate);
    streamFormat.mFormatID = kAudioFormatLinearPCM;
    streamFormat.mFormatFlags = kAudioFormatFlagsNativeFloatPacked | kAudioFormatFlagIsNonInterleaved;
    streamFormat.mBytesPerPacket = sizeof(float);
    streamFormat.mFramesPerPacket = 1;
    streamFormat.mBytesPerFrame = sizeof(float);
    streamFormat.mChannelsPerFrame = 2;
    streamFormat.mBitsPerChannel = 8 * sizeof(float);

    // Preserve low-latency setting across re-initialisation.
    UInt32 reloadFrameSize = 64;
    AudioUnitSetProperty(audioUnit, kAudioUnitProperty_MaximumFramesPerSlice, kAudioUnitScope_Global, 0, &reloadFrameSize, sizeof(reloadFrameSize));
    AudioUnitSetProperty(audioUnit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, 0, &streamFormat, sizeof(streamFormat));

    // Update sample rate and reinitialize audio unit
    this->sampleRate = sampleRate;
    AudioUnitInitialize(audioUnit);
    AudioOutputUnitStart(audioUnit);
    return true;
}

void AudioEngine::play(SoundType type) {
    int voiceIndex = nextVoiceIndex.fetch_add(1, std::memory_order_relaxed) % MAX_VOICES;
    voiceSoundTypes[voiceIndex] = type;
    playbackPositions[voiceIndex].store(0, std::memory_order_relaxed);
}
