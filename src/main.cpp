#include <iostream>
#include <csignal>
#include <atomic>
#include <CoreGraphics/CoreGraphics.h>
#include <CoreFoundation/CoreFoundation.h>
#include "AudioEngine.h"

AudioEngine audioEngine;
std::atomic<bool> shouldExit(false);

void signalHandler(int signum) {
    std::cout << "Signal (" << signum << ") reçu. Arrêt..." << std::endl;
    shouldExit = true;
    CFRunLoopStop(CFRunLoopGetCurrent());
}

CGEventRef keyboardCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *userInfo) {
    if (type == kCGEventKeyDown) {
        audioEngine.play();
    }
    return event;
}

int main(int argc, const char * argv[]) {
    signal(SIGINT, signalHandler);
    signal(SIGTERM, signalHandler);

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

    std::cout << "Moteur Kream démarré. Appuyez sur des touches... (Ctrl+C pour quitter)" << std::endl;
    CFRunLoopRun();

    std::cout << "Arrêt du moteur Kream." << std::endl;

    CFRelease(eventTap);
    CFRelease(runLoopSource);

    return 0;
}
