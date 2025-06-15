#pragma once

#include "audio/Audio.hpp"
#include <string>

class Theme {
public:
    Theme(Audio& audio);
    void loadThemes(const std::string& themesPath);

private:
    Audio& audio;
};
