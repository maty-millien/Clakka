#include <iostream>
#include <thread>
#include <chrono>
#include <CoreFoundation/CoreFoundation.h>

#include "Input.hpp"

Input::Input(Audio& audio) : audio(audio), eventTap(nullptr), runLoopSource(nullptr) {}

Input::~Input() {
    stop();
}

void Input::start() {
    eventTap = CGEventTapCreate(
        kCGSessionEventTap, kCGHeadInsertEventTap, kCGEventTapOptionDefault,
        CGEventMaskBit(kCGEventKeyDown) | CGEventMaskBit(kCGEventKeyUp) | CGEventMaskBit(kCGEventFlagsChanged),
        keyboardCallback, this
    );

    if (!eventTap) {
        std::cerr << "Error: Failed to create event tap. Check Accessibility permissions." << std::endl;
        exit(1);
    }

    runLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, eventTap, 0);
    CFRunLoopAddSource(CFRunLoopGetCurrent(), runLoopSource, kCFRunLoopCommonModes);
    CGEventTapEnable(eventTap, true);
}

void Input::stop() {
    if (eventTap) {
        CGEventTapEnable(eventTap, false);
        CFRunLoopRemoveSource(CFRunLoopGetCurrent(), runLoopSource, kCFRunLoopCommonModes);
        CFRelease(runLoopSource);
        CFRelease(eventTap);
        eventTap = nullptr;
        runLoopSource = nullptr;
    }
}

CGEventRef Input::keyboardCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *userInfo) {
    Input* self = static_cast<Input*>(userInfo);

    static CGEventFlags lastFlags = CGEventGetFlags(event);
    int keyCode = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
    if (type == kCGEventFlagsChanged) {
        CGEventFlags flags = CGEventGetFlags(event);
        CGEventFlags changed = flags ^ lastFlags;
        if (changed & kCGEventFlagMaskShift) {
            if (flags & kCGEventFlagMaskShift)
                self->audio.play(keyCode, Audio::PRESS);
            else
                self->audio.play(keyCode, Audio::RELEASE);
        }
        if (changed & kCGEventFlagMaskControl) {
            if (flags & kCGEventFlagMaskControl)
                self->audio.play(keyCode, Audio::PRESS);
            else
                self->audio.play(keyCode, Audio::RELEASE);
        }
        if (changed & kCGEventFlagMaskAlternate) {
            if (flags & kCGEventFlagMaskAlternate)
                self->audio.play(keyCode, Audio::PRESS);
            else
                self->audio.play(keyCode, Audio::RELEASE);
        }
        if (changed & kCGEventFlagMaskCommand) {
            if (flags & kCGEventFlagMaskCommand)
                self->audio.play(keyCode, Audio::PRESS);
            else
                self->audio.play(keyCode, Audio::RELEASE);
        }
        if (changed & kCGEventFlagMaskAlphaShift) {
            self->audio.play(keyCode, Audio::PRESS);
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
            self->audio.play(keyCode, Audio::RELEASE);
        }
        if (changed & kCGEventFlagMaskSecondaryFn) {
            if (flags & kCGEventFlagMaskSecondaryFn)
                self->audio.play(keyCode, Audio::PRESS);
            else
                self->audio.play(keyCode, Audio::RELEASE);
        }
        lastFlags = flags;
    } else if (type == kCGEventKeyDown) {
        if (CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat) == 0) {
            self->audio.play(keyCode, Audio::PRESS);
        }
    } else if (type == kCGEventKeyUp) {
        self->audio.play(keyCode, Audio::RELEASE);
    }
    return event;
}
