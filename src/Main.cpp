#include <iostream>
#include <CoreFoundation/CoreFoundation.h>

#include "Main.h"

volatile sig_atomic_t Main::running = true;

Main::Main()
    : Audio(),
      Theme(Audio),
      Input(Audio) {
    signal(SIGINT, Main::signalHandler);
}

void Main::run() {
    std::cout << "Kream engine starting..." << std::endl;

    Theme.loadThemes("assets/themes");
    Input.start();

    std::cout << "Kream engine started. Press keys... (Ctrl+C to quit)" << std::endl;

    while(running) CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, true);

    Input.stop();
    std::cout << "Kream engine stopped." << std::endl;
}

void Main::signalHandler(int signum) {
    if (signum == SIGINT) {
        running = false;
        CFRunLoopStop(CFRunLoopGetCurrent());
    }
}

int main() {
    Main main;
    main.run();
}
