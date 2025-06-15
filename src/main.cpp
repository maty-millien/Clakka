#include <iostream>
#include <csignal>
#include <atomic>
#include <CoreGraphics/CoreGraphics.h>
#include <CoreFoundation/CoreFoundation.h>
#include <thread>
#include <chrono>
#include "AudioEngine.h"
#include <dirent.h>
#include <unordered_map>
#include <string>

AudioEngine audioEngine;
std::atomic<bool> shouldExit(false);

void signalHandler(int signum) {
    std::cout << "Signal (" << signum << ") reçu. Arrêt..." << std::endl;
    shouldExit = true;
    CFRunLoopStop(CFRunLoopGetCurrent());
}

CGEventRef keyboardCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *userInfo) {
    static CGEventFlags lastFlags = CGEventGetFlags(event);
    int keyCode = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
    if (type == kCGEventFlagsChanged) {
        CGEventFlags flags = CGEventGetFlags(event);
        CGEventFlags changed = flags ^ lastFlags;
        if (changed & kCGEventFlagMaskShift) {
            if (flags & kCGEventFlagMaskShift)
                audioEngine.play(keyCode, AudioEngine::PRESS);
            else
                audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskControl) {
            if (flags & kCGEventFlagMaskControl)
                audioEngine.play(keyCode, AudioEngine::PRESS);
            else
                audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskAlternate) {
            if (flags & kCGEventFlagMaskAlternate)
                audioEngine.play(keyCode, AudioEngine::PRESS);
            else
                audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskCommand) {
            if (flags & kCGEventFlagMaskCommand)
                audioEngine.play(keyCode, AudioEngine::PRESS);
            else
                audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskAlphaShift) {
            audioEngine.play(keyCode, AudioEngine::PRESS);
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
            audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        if (changed & kCGEventFlagMaskSecondaryFn) {
            if (flags & kCGEventFlagMaskSecondaryFn)
                audioEngine.play(keyCode, AudioEngine::PRESS);
            else
                audioEngine.play(keyCode, AudioEngine::RELEASE);
        }
        lastFlags = flags;
    } else if (type == kCGEventKeyDown) {
        if (CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat) == 0) {
            audioEngine.play(keyCode, AudioEngine::PRESS);
        }
    } else if (type == kCGEventKeyUp) {
        audioEngine.play(keyCode, AudioEngine::RELEASE);
    }
    return event;
}

int main(int argc, const char * argv[]) {
    signal(SIGINT, signalHandler);
    signal(SIGTERM, signalHandler);

    // Load theme sounds (build-run copies WAVs into build/ so scan both current dir and themes/V1)
    std::unordered_map<std::string, int> keyCodeMap = {
        {"back", 51}, {"enter", 36}, {"tab", 48}, {"space", 49},
        {"shift", 56}, {"ctrl", 59}, {"caps", 57}
    };
    std::string themeDirs[] = {".", "themes/V1"};
    for (const auto &themeDir : themeDirs) {
        DIR* dir = opendir(themeDir.c_str());
        if (!dir) continue;
        struct dirent* ent;
        while ((ent = readdir(dir)) != NULL) {
            if (ent->d_type != DT_REG) continue;
            std::string name(ent->d_name);
            if (name.size() < 5 || name.substr(name.size() - 4) != ".wav") continue;
            std::string path = themeDir + "/" + name;
            std::string stem = name.substr(0, name.size() - 4);
            auto pos = stem.rfind('_');
            if (pos == std::string::npos) continue;
            std::string keyName = stem.substr(0, pos);
            std::string action = stem.substr(pos + 1);
            AudioEngine::SoundType type = (action == "press") ? AudioEngine::PRESS : AudioEngine::RELEASE;
            auto it = keyCodeMap.find(keyName);
            if (it != keyCodeMap.end()) {
                if (!audioEngine.loadKeySound(it->second, type, path.c_str())) {
                    closedir(dir);
                    return 1;
                }
            } else if (keyName.compare(0, 3, "key") == 0) {
                if (!audioEngine.loadFallbackSound(type, path.c_str())) {
                    closedir(dir);
                    return 1;
                }
            }
        }
        closedir(dir);
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
