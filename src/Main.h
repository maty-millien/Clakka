#pragma once

#include <csignal>

#include "audio/Audio.h"
#include "input/Input.h"

class Main {
public:
    Main();
    void run();

private:
    static void signalHandler(int signum);

    Audio Audio;
    Input Input;
    static volatile sig_atomic_t running;
};
