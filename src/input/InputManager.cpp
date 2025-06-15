#include "InputManager.hpp"
#include <iostream>
#include <thread>
#include <chrono>
#include <CoreFoundation/CoreFoundation.h>

InputManager::InputManager(AudioEngine& engine) : audioEngine(engine), eventTap(nullptr), runLoopSource(nullptr) {}

InputManager::~InputManager() {
    stop();
}

void InputManager::start() {
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

void InputManager::stop() {
    if (eventTap) {
        CGEventTapEnable(eventTap, false);
        CFRunLoopRemoveSource(CFRunLoopGetCurrent(), runLoopSource, kCFRunLoopCommonModes);
        CFRelease(runLoopSource);
        CFRelease(eventTap);
        eventTap = nullptr;
        runLoopSource = nullptr;
    }
}

CGEventRef InputManager::keyboardCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *userInfo) {
    InputManager* self = static_cast<InputManager*>(userInfo);

    static CGEventFlags lastFlags = CGEventGetFlags(event);
    int keyCode = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
    if (type == kCGEventFlagsChanged) {
        CGEventFlags flags = CGEventGetFlags(event);
        CGEventFlags changed = flags ^ lastFlags;
        if (changed & kCGEventFlagMaskShift) {
            if (flags & kCGEventFlagMaskShift)
                self->audioEngine.play(keyCode, AudioEngine::PRESS);
            else
                self->audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskControl) {
            if (flags & kCGEventFlagMaskControl)
                self->audioEngine.play(keyCode, AudioEngine::PRESS);
            else
                self->audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskAlternate) {
            if (flags & kCGEventFlagMaskAlternate)
                self->audioEngine.play(keyCode, AudioEngine::PRESS);
            else
                self->audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskCommand) {
            if (flags & kCGEventFlagMaskCommand)
                self->audioEngine.play(keyCode, AudioEngine::PRESS);
            else
                self->audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskAlphaShift) {
            self->audioEngine.play(keyCode, AudioEngine::PRESS);
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
            self->audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskSecondaryFn) {
            if (flags & kCGEventFlagMaskSecondaryFn)
                self->audioEngine.play(keyCode, AudioEngine::PRESS);
            else
                self->audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        lastFlags = flags;
    } else if (type == kCGEventKeyDown) {
        if (CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat) == 0) {
            self->audioEngine.play(keyCode, AudioEngine::PRESS);
        }
    } else if (type == kCGEventKeyUp) {
        self->audioEngine.play(keyCode, AudioEngine::RELEASE);
    }
    return event;
}
