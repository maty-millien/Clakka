#include <iostream>
#include <array>
#include <thread>
#include <chrono>
#include <stdexcept>
#include <CoreFoundation/CoreFoundation.h>
#include <mach/mach_time.h>

#include "Input.h"

namespace {
    // Modifier key masks for standard press/release behavior
    constexpr std::array<CGEventFlags, 5> kModifierMasks = {
        kCGEventFlagMaskShift,
        kCGEventFlagMaskControl,
        kCGEventFlagMaskAlternate,
        kCGEventFlagMaskCommand,
        kCGEventFlagMaskSecondaryFn
    };
}

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
        throw std::runtime_error("Failed to create event tap. Check Accessibility permissions.");
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

CGEventRef Input::keyboardCallback(CGEventTapProxy, CGEventType type, CGEventRef event, void *userInfo) {
    uint64_t eventTime = mach_absolute_time();
    Input* self = static_cast<Input*>(userInfo);

    static CGEventFlags lastFlags = CGEventGetFlags(event);
    int keyCode = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
    if (type == kCGEventFlagsChanged) {
        CGEventFlags flags = CGEventGetFlags(event);
        CGEventFlags changed = flags ^ lastFlags;

        // Handle standard modifier keys (Shift, Control, Alt, Cmd, Fn)
        for (CGEventFlags mask : kModifierMasks) {
            if (changed & mask) {
                self->audio.play(keyCode, (flags & mask) ? Audio::PRESS : Audio::RELEASE, eventTime);
            }
        }

        // CapsLock is a toggle key - it generates only one event per press (not separate
        // press/release events like other modifiers). We simulate both press and release
        // sounds with a short delay to provide complete audio feedback for the key action.
        if (changed & kCGEventFlagMaskAlphaShift) {
            self->audio.play(keyCode, Audio::PRESS, eventTime);
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
            self->audio.play(keyCode, Audio::RELEASE, mach_absolute_time());
        }

        lastFlags = flags;
    } else if (type == kCGEventKeyDown) {
        if (CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat) == 0) {
            self->audio.play(keyCode, Audio::PRESS, eventTime);
        }
    } else if (type == kCGEventKeyUp) {
        self->audio.play(keyCode, Audio::RELEASE, eventTime);
    }
    return event;
}
