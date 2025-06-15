#include "ThemeManager.hpp"
#include <iostream>
#include <dirent.h>
#include <unordered_map>

ThemeManager::ThemeManager(AudioEngine& engine) : audioEngine(engine) {}

void ThemeManager::loadThemes(const std::string& themesPath) {
    std::unordered_map<std::string, int> keyCodeMap = {
        {"back", 51}, {"enter", 36}, {"tab", 48}, {"space", 49},
        {"shift", 56}, {"ctrl", 59}, {"caps", 57}
    };
    std::string themeDirs[] = {themesPath + "/V1"};
    for (const auto &themeDir : themeDirs) {
        DIR* dir = opendir(themeDir.c_str());
        if (!dir) {
            std::cerr << "Warning: Could not open theme directory: " << themeDir << std::endl;
            continue;
        }
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
                    return;
                }
            } else if (keyName.compare(0, 3, "key") == 0) {
                if (!audioEngine.loadFallbackSound(type, path.c_str())) {
                    closedir(dir);
                    return;
                }
            }
        }
        closedir(dir);
    }
}
