#include <iostream>
#include <CoreGraphics/CoreGraphics.h>
#include <CoreFoundation/CoreFoundation.h>
#include "AudioEngine.h"

AudioEngine audioEngine;

CGEventRef keyboardCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *userInfo) {
    if (type == kCGEventKeyDown) {
        audioEngine.play();
    }
    return event;
}

int main(int argc, const char * argv[]) {
    if (!audioEngine.loadWav("key_press.wav")) {
        if (!audioEngine.loadWav("../res/key_press.wav")) {
            return 1;
        }
    }

    CFMachPortRef eventTap = CGEventTapCreate(
        kCGSessionEventTap, kCGHeadInsertEventTap, kCGEventTapOptionDefault,
        CGEventMaskBit(kCGEventKeyDown), keyboardCallback, NULL
    );

    if (!eventTap) {
        std::cerr << "Erreur: Impossible de créer l'Event Tap. Vérifiez les permissions d'Accessibilité." << std::endl;
        return 1;
    }

    CFRunLoopSourceRef runLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, eventTap, 0);
    CFRunLoopAddSource(CFRunLoopGetCurrent(), runLoopSource, kCFRunLoopCommonModes);
    CGEventTapEnable(eventTap, true);

    std::cout << "Moteur Kream démarré. Appuyez sur des touches..." << std::endl;
    CFRunLoopRun();

    CFRelease(eventTap);
    CFRelease(runLoopSource);

    return 0;
}
