#include <iostream>
#include <stdexcept>
#include <cstdlib>
#include <CoreFoundation/CoreFoundation.h>

#include "Main.h"
#include "theme/Theme.h"

volatile sig_atomic_t Main::running = true;

Main::Main()
    : Audio(),
      Input(Audio) {
    signal(SIGINT, Main::signalHandler);
}

void Main::run() {
    std::cout << "Kream engine starting..." << std::endl;

    // Load theme sounds - continue even if some sounds fail to load
    if (!Theme::loadThemes(Audio, "assets/themes")) {
        std::cerr << "Warning: Theme loading had issues, continuing with available sounds" << std::endl;
    }

    // Start input capture - this may throw if permissions are not granted
    try {
        Input.start();
    } catch (const std::runtime_error& e) {
        std::cerr << "Error: " << e.what() << std::endl;
        return;
    }

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
    try {
        Main main;
        main.run();
        return EXIT_SUCCESS;
    } catch (const std::exception& e) {
        std::cerr << "Fatal error: " << e.what() << std::endl;
        return EXIT_FAILURE;
    }
}
