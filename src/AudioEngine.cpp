#include "AudioEngine.h"
#include <iostream>
#include <algorithm>
#include <mutex>

#define DR_WAV_IMPLEMENTATION
#include "dr_wav.h"

OSStatus AudioEngine::renderCallback(void *inRefCon, AudioUnitRenderActionFlags *ioActionFlags, const AudioTimeStamp *inTimeStamp, UInt32 inBusNumber, UInt32 inNumberFrames, AudioBufferList *ioData) {
    AudioEngine* self = static_cast<AudioEngine*>(inRefCon);

    float* outL = (float*)ioData->mBuffers[0].mData;
    float* outR = (float*)ioData->mBuffers[1].mData;

    for (UInt32 i = 0; i < inNumberFrames; ++i) {
        float mixedSample = 0.0f;
        {
            std::lock_guard<std::mutex> lock(self->playbackMutex);
            for (auto &pos : self->playbackPositions) {
                if (pos < self->bufferFrameCount) {
                    mixedSample += self->audioBuffer[pos];
                    pos++;
                }
            }
            self->playbackPositions.erase(
                std::remove_if(self->playbackPositions.begin(), self->playbackPositions.end(),
                               [&](uint64_t pos){ return pos >= self->bufferFrameCount; }),
                self->playbackPositions.end());
        }
        outL[i] = mixedSample;
        outR[i] = mixedSample;
    }
    return noErr;
}

AudioEngine::AudioEngine() : audioUnit(nullptr), audioBuffer(nullptr), bufferFrameCount(0) {
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
    delete[] audioBuffer;
}

bool AudioEngine::loadWav(const char* filePath) {
    unsigned int channels;
    unsigned int sampleRate;
    drwav_uint64 totalFrameCount;

    audioBuffer = drwav_open_file_and_read_pcm_frames_f32(filePath, &channels, &sampleRate, &totalFrameCount, NULL);

    if (audioBuffer == nullptr) {
        std::cerr << "Erreur: Impossible de charger le fichier WAV " << filePath << std::endl;
        return false;
    }

    bufferFrameCount = totalFrameCount;
    std::cout << "Fichier " << filePath << " chargé. " << bufferFrameCount << " frames à " << sampleRate << " Hz." << std::endl;
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

void AudioEngine::play() {
    std::lock_guard<std::mutex> lock(playbackMutex);
    playbackPositions.push_back(0);
}
