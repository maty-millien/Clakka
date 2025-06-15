#pragma once

#include "audio/AudioEngine.hpp"
#include "theme/ThemeManager.hpp"
#include "input/InputManager.hpp"
#include <csignal>

class Application {
public:
    Application();
    void run();

private:
    static void signalHandler(int signum);

    AudioEngine audioEngine;
    ThemeManager themeManager;
    InputManager inputManager;
    static volatile sig_atomic_t running;
};
