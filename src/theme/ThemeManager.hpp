#pragma once

#include "audio/AudioEngine.hpp"
#include <string>

class ThemeManager {
public:
    ThemeManager(AudioEngine& audioEngine);
    void loadThemes(const std::string& themesPath);

private:
    AudioEngine& audioEngine;
};
