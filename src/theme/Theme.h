#pragma once

#include "audio/Audio.h"
#include <string>

namespace Theme {

/// Load sound themes from the specified themes directory.
/// Scans for WAV files and registers them with the Audio engine.
/// Returns true if all sounds loaded successfully, false if any failed.
/// Continues loading remaining sounds even if some fail.
bool loadThemes(Audio& audio, const std::string& themesPath);

} // namespace Theme
