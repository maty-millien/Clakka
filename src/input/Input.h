#pragma once

#include "audio/Audio.h"
#include <CoreGraphics/CoreGraphics.h>

class Input {
public:
    Input(Audio& audio);
    ~Input();

    void start();
    void stop();

private:
    static CGEventRef keyboardCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void* userInfo);

    Audio& audio;
    CFMachPortRef eventTap;
    CFRunLoopSourceRef runLoopSource;
};
