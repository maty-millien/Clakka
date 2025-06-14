#include "AudioEngine.h"
#include <iostream>

#define DR_WAV_IMPLEMENTATION
#include "dr_wav.h"

OSStatus AudioEngine::renderCallback(void *inRefCon, AudioUnitRenderActionFlags *ioActionFlags, const AudioTimeStamp *inTimeStamp, UInt32 inBusNumber, UInt32 inNumberFrames, AudioBufferList *ioData) {
    AudioEngine* self = static_cast<AudioEngine*>(inRefCon);

    float* outL = (float*)ioData->mBuffers[0].mData;
    float* outR = (float*)ioData->mBuffers[1].mData;

    for (UInt32 i = 0; i < inNumberFrames; ++i) {
        outL[i] = 0.0f;
        outR[i] = 0.0f;
    }

    for (UInt32 i = 0; i < inNumberFrames; ++i) {
        float mixedSampleL = 0.0f;
        float mixedSampleR = 0.0f;

        for (int v = 0; v < MAX_VOICES; ++v) {
            uint64_t pos = self->playbackPositions[v].load(std::memory_order_relaxed);
            SoundType type = self->voiceSoundTypes[v];

            if (type < COUNT && self->audioBuffers[type] != nullptr && pos < self->bufferFrameCounts[type]) {
                if (self->channels == 1) {
                    float sample = self->audioBuffers[type][pos];
                    mixedSampleL += sample;
                    mixedSampleR += sample;
                } else if (self->channels >= 2) {
                    mixedSampleL += self->audioBuffers[type][pos * self->channels];
                    mixedSampleR += self->audioBuffers[type][pos * self->channels + 1];
                }
                self->playbackPositions[v].store(pos + 1, std::memory_order_relaxed);
            }
        }

        outL[i] = mixedSampleL < -1.0f ? -1.0f : (mixedSampleL > 1.0f ? 1.0f : mixedSampleL);
        outR[i] = mixedSampleR < -1.0f ? -1.0f : (mixedSampleR > 1.0f ? 1.0f : mixedSampleR);
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
    desc.componentSubType = kAudioUnitSubType_DefaultOutput;
    desc.componentFlags = 0;
    desc.componentFlagsMask = 0;
    desc.componentManufacturer = kAudioUnitManufacturer_Apple;

    AudioComponent outputComponent = AudioComponentFindNext(NULL, &desc);
    AudioComponentInstanceNew(outputComponent, &audioUnit);
    AudioUnitInitialize(audioUnit);

    AudioStreamBasicDescription streamFormat = {};
    streamFormat.mSampleRate = 44100.0;
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
    AudioUnitSetProperty(audioUnit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, 0, &streamFormat, sizeof(streamFormat));
    AudioUnitInitialize(audioUnit);
    AudioOutputUnitStart(audioUnit);
    return true;
}

void AudioEngine::play(SoundType type) {
    int voiceIndex = nextVoiceIndex.fetch_add(1, std::memory_order_relaxed) % MAX_VOICES;
    voiceSoundTypes[voiceIndex] = type;
    playbackPositions[voiceIndex].store(0, std::memory_order_relaxed);
}
