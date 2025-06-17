#pragma once

#include <csignal>

#include "audio/Audio.hpp"
#include "theme/Theme.hpp"
#include "input/Input.hpp"

class App {
public:
    App();
    void run();

private:
    static void signalHandler(int signum);

    Audio Audio;
    Theme Theme;
    Input Input;
    static volatile sig_atomic_t running;
};
