#include <iostream>
#include <csignal>
#include <atomic>
#include <CoreGraphics/CoreGraphics.h>
#include <CoreFoundation/CoreFoundation.h>
#include <thread>
#include <chrono>
#include "AudioEngine.h"


AudioEngine audioEngine;
std::atomic<bool> shouldExit(false);

void signalHandler(int signum) {
    std::cout << "Signal (" << signum << ") reçu. Arrêt..." << std::endl;
    shouldExit = true;
    CFRunLoopStop(CFRunLoopGetCurrent());
}

CGEventRef keyboardCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *userInfo) {
    static CGEventFlags lastFlags = CGEventGetFlags(event);
    if (type == kCGEventFlagsChanged) {
        CGEventFlags flags = CGEventGetFlags(event);
        CGEventFlags changed = flags ^ lastFlags;
        if (changed & kCGEventFlagMaskShift) {
            if (flags & kCGEventFlagMaskShift)
                audioEngine.play(AudioEngine::PRESS);
            else
                audioEngine.play(AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskControl) {
            if (flags & kCGEventFlagMaskControl)
                audioEngine.play(AudioEngine::PRESS);
            else
                audioEngine.play(AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskAlternate) {
            if (flags & kCGEventFlagMaskAlternate)
                audioEngine.play(AudioEngine::PRESS);
            else
                audioEngine.play(AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskCommand) {
            if (flags & kCGEventFlagMaskCommand)
                audioEngine.play(AudioEngine::PRESS);
            else
                audioEngine.play(AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskAlphaShift) {
            // Caps Lock is a toggle: play both press and release as a normal key press
            audioEngine.play(AudioEngine::PRESS);
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
            audioEngine.play(AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskSecondaryFn) {
            if (flags & kCGEventFlagMaskSecondaryFn)
                audioEngine.play(AudioEngine::PRESS);
            else
                audioEngine.play(AudioEngine::RELEASE);
        }
        lastFlags = flags;
    } else if (type == kCGEventKeyDown) {
        if (CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat) == 0) {
            audioEngine.play(AudioEngine::PRESS);
        }
    } else if (type == kCGEventKeyUp) {
        audioEngine.play(AudioEngine::RELEASE);
    }
    return event;
}

int main(int argc, const char * argv[]) {
    signal(SIGINT, signalHandler);
    signal(SIGTERM, signalHandler);

    if (!audioEngine.loadWav("../res/key_press.wav", AudioEngine::PRESS)) {
        return 1;
    }
    if (!audioEngine.loadWav("../res/key_release.wav", AudioEngine::RELEASE)) {
        return 1;
    }

    CFMachPortRef eventTap = CGEventTapCreate(
        kCGSessionEventTap, kCGHeadInsertEventTap, kCGEventTapOptionDefault,
        CGEventMaskBit(kCGEventKeyDown) | CGEventMaskBit(kCGEventKeyUp) | CGEventMaskBit(kCGEventFlagsChanged), keyboardCallback, NULL
    );

    if (!eventTap) {
        std::cerr << "Erreur: Impossible de créer l'Event Tap. Vérifiez les permissions d'Accessibilité." << std::endl;
        return 1;
    }

    CFRunLoopSourceRef runLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, eventTap, 0);
    CFRunLoopAddSource(CFRunLoopGetCurrent(), runLoopSource, kCFRunLoopCommonModes);
    CGEventTapEnable(eventTap, true);

    std::cout << "Moteur Kream démarré. Appuyez sur des touches... (Ctrl+C pour quitter)" << std::endl;
    CFRunLoopRun();

    std::cout << "Arrêt du moteur Kream." << std::endl;

    CFRelease(eventTap);
    CFRelease(runLoopSource);

    return 0;
}
