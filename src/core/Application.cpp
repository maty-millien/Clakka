#include "Application.hpp"
#include <iostream>
#include <CoreFoundation/CoreFoundation.h>

volatile sig_atomic_t Application::running = true;

Application::Application()
    : audioEngine(),
      themeManager(audioEngine),
      inputManager(audioEngine) {
    signal(SIGINT, Application::signalHandler);
}

void Application::run() {
    std::cout << "Kream engine starting..." << std::endl;

    themeManager.loadThemes("assets/themes");
    inputManager.start();

    std::cout << "Kream engine started. Press keys... (Ctrl+C to quit)" << std::endl;

    while(running) {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, true);
    }

    inputManager.stop();

    std::cout << "Kream engine stopped." << std::endl;
}

void Application::signalHandler(int signum) {
    if (signum == SIGINT) {
        running = false;
        CFRunLoopStop(CFRunLoopGetCurrent());
    }
}
