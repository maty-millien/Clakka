# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Kream is a macOS keyboard audio feedback engine written in C++17. It plays custom sound effects for key presses and releases using real-time audio processing. The project requires macOS Accessibility permissions to intercept global keyboard events.

## Build Commands

```bash
make all      # Build in Release mode (default)
make run      # Build and run
make clean    # Remove build directory
make rebuild  # Clean and rebuild
```

For Debug builds: `BUILD_TYPE=Debug make all`

## Architecture

Three core components orchestrated by `Main`:

1. **Audio** (`src/audio/`) - Real-time polyphonic audio engine

   - Uses macOS AudioToolbox with HAL Output AudioUnit
   - 16-voice polyphonic mixer with round-robin allocation
   - Atomic operations for thread-safe voice management
   - SIMD mixing via Accelerate framework (vDSP)

2. **Input** (`src/input/`) - Keyboard event capture

   - CoreGraphics event tap for system-wide keyboard monitoring
   - Captures key down, key up, and modifier changes
   - Routes events to Audio component for playback

3. **Theme** (`src/theme/`) - Sound theme loader
   - Loads WAV files from `assets/themes/` directories
   - Key-specific sounds: `{keyname}_{action}.wav` (e.g., `space_press.wav`)
   - Fallback sounds: `key1_press.wav` through `key6_press.wav` for unmapped keys

## Dependencies

- **macOS frameworks**: CoreGraphics, AudioToolbox, CoreFoundation, CoreAudio, Accelerate
- **Third-party**: `lib/dr_wav.h` (single-header WAV decoder)

## Adding Source Files

New source files must be manually added to `CMakeLists.txt` in the `target_sources` section (GLOB is intentionally not used).
