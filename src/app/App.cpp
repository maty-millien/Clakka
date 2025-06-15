#include "App.hpp"
#include <iostream>
#include <CoreFoundation/CoreFoundation.h>

volatile sig_atomic_t App::running = true;

App::App()
    : Audio(),
      Theme(Audio),
      Input(Audio) {
    signal(SIGINT, App::signalHandler);
}

void App::run() {
    std::cout << "Kream engine starting..." << std::endl;

    Theme.loadThemes("assets/themes");
    Input.start();

    std::cout << "Kream engine started. Press keys... (Ctrl+C to quit)" << std::endl;

    while(running) {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, true);
    }

    Input.stop();

    std::cout << "Kream engine stopped." << std::endl;
}

void App::signalHandler(int signum) {
    if (signum == SIGINT) {
        running = false;
        CFRunLoopStop(CFRunLoopGetCurrent());
    }
}

int main(int argc, const char * argv[]) {
    App app;
    app.run();
    return 0;
}
