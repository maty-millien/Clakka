/// Theme Sound Loader
///
/// Expected filename format for sound files:
///
/// Key-specific sounds: `{keyname}_{action}.wav`
///   - keyname: One of the mapped key names (back, enter, tab, space, shift, ctrl, caps)
///   - action: Either "press" or "release"
///   - Examples: `space_press.wav`, `return_release.wav`, `tab_press.wav`
///
/// Fallback sounds: `key{N}_{action}.wav`
///   - N: A number from 1-6 (e.g., key1, key2, ... key6)
///   - action: Either "press" or "release"
///   - These sounds are used for keys that don't have a specific mapping
///   - Examples: `key1_press.wav`, `key3_release.wav`

#include "Theme.h"
#include <iostream>
#include <dirent.h>
#include <unordered_map>

namespace Theme {

bool loadThemes(Audio& audio, const std::string& themesPath) {
    // Map of human-readable key names to macOS virtual key codes
    std::unordered_map<std::string, int> keyCodeMap = {
        {"back", 51}, {"enter", 36}, {"tab", 48}, {"space", 49},
        {"shift", 56}, {"ctrl", 59}, {"caps", 57}
    };

    std::string themeDirs[] = {themesPath + "/Creamy"};

    int loadedCount = 0;
    int failedCount = 0;

    for (const auto &themeDir : themeDirs) {
        DIR* dir = opendir(themeDir.c_str());
        if (!dir) {
            std::cerr << "Warning: Could not open theme directory: " << themeDir << std::endl;
            continue;
        }

        struct dirent* ent;
        while ((ent = readdir(dir)) != NULL) {
            // Skip non-regular files (directories, symlinks, etc.)
            if (ent->d_type != DT_REG) continue;

            std::string name(ent->d_name);

            // Skip files that aren't .wav files (need at least 5 chars: "x.wav")
            if (name.size() < 5 || name.substr(name.size() - 4) != ".wav") continue;

            std::string path = themeDir + "/" + name;

            // Parse filename: extract stem by removing .wav extension
            std::string stem = name.substr(0, name.size() - 4);

            // Find the last underscore to split into keyName and action
            // Example: "space_press" -> keyName="space", action="press"
            auto pos = stem.rfind('_');
            if (pos == std::string::npos) continue;

            std::string keyName = stem.substr(0, pos);
            std::string action = stem.substr(pos + 1);

            // Determine sound type based on action suffix
            Audio::SoundType type = (action == "press") ? Audio::PRESS : Audio::RELEASE;

            // Check if this is a key-specific sound (e.g., "space_press.wav")
            auto it = keyCodeMap.find(keyName);
            if (it != keyCodeMap.end()) {
                if (audio.loadKeySound(it->second, type, path.c_str())) {
                    loadedCount++;
                } else {
                    failedCount++;
                }
            }
            // Check if this is a fallback sound (e.g., "key1_press.wav")
            else if (keyName.compare(0, 3, "key") == 0) {
                if (audio.loadFallbackSound(type, path.c_str())) {
                    loadedCount++;
                } else {
                    failedCount++;
                }
            }
            // Files that don't match either pattern are silently ignored
        }
        closedir(dir);
    }

    // Report loading summary
    if (failedCount > 0) {
        std::cerr << "Warning: Failed to load " << failedCount << " sound(s), "
                  << loadedCount << " loaded successfully" << std::endl;
    } else if (loadedCount > 0) {
        std::cout << "Loaded " << loadedCount << " sound(s) successfully" << std::endl;
    } else {
        std::cerr << "Warning: No sounds were loaded" << std::endl;
    }

    return failedCount == 0 && loadedCount > 0;
}

} // namespace Theme
