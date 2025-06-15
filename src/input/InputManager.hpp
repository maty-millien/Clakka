#pragma once

#include "audio/AudioEngine.hpp"
#include <CoreGraphics/CoreGraphics.h>

class InputManager {
public:
    InputManager(AudioEngine& audioEngine);
    ~InputManager();

    void start();
    void stop();

private:
    static CGEventRef keyboardCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void* userInfo);

    AudioEngine& audioEngine;
    CFMachPortRef eventTap;
    CFRunLoopSourceRef runLoopSource;
};
