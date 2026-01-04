cmake_minimum_required(VERSION 3.25)

# Project definition and semantic versioning
project(kream_engine
    VERSION 1.0.0
    LANGUAGES CXX
)

# --- 1. Global Configuration ---

# Default to Release build if not specified.
# This prevents unoptimized debug builds from running in production by accident.
if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    message(STATUS "No build type selected, defaulting to Release")
    set(CMAKE_BUILD_TYPE "Release" CACHE STRING "Build type (Debug, Release, RelWithDebInfo, MinSizeRel)" FORCE)
endif()

# Enforce C++17 Standard
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF) # Disable compiler-specific extensions (e.g., -std=gnu++17)

# Check and enable IPO/LTO (Link Time Optimization) if supported
include(CheckIPOSupported)
check_ipo_supported(RESULT result OUTPUT output)
if(result)
    set(CMAKE_INTERPROCEDURAL_OPTIMIZATION ON)
else()
    message(WARNING "IPO is not supported: ${output}")
endif()

# --- 2. Target Definition ---
# Define the executable target immediately to attach properties to it later
add_executable(kream_engine)

# --- 3. Sources ---
# Explicitly list sources. NEVER use GLOB/GLOB_RECURSE.
# It ensures the build system detects file additions/removals deterministically.
target_sources(kream_engine PRIVATE
    src/Main.cpp
    src/audio/Audio.cpp
    src/theme/Theme.cpp
    src/input/Input.cpp
    # Add new source files here manually
)

# --- 4. Include Directories ---
target_include_directories(kream_engine PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/src
    ${CMAKE_CURRENT_SOURCE_DIR}/lib
)

# --- 5. Compiler Flags (Target-Centric) ---
# General warning flags (Quality Control)
target_compile_options(kream_engine PRIVATE
    -Wall
    -Wextra
    -Wpedantic
    $<$<CXX_COMPILER_ID:Clang>:-Werror> # Treat warnings as errors on Clang (strict mode)
)

# Release-specific optimizations
# applied only when CMAKE_BUILD_TYPE is Release
target_compile_options(kream_engine PRIVATE
    $<$<CONFIG:Release>:-O3>
    $<$<CONFIG:Release>:-march=native>    # Optimizes for the host CPU (Careful with distribution)
    $<$<CONFIG:Release>:-funroll-loops>
)

# Define NDEBUG in Release to disable asserts
target_compile_definitions(kream_engine PRIVATE
    $<$<CONFIG:Release>:NDEBUG>
)

# --- 6. Platform Dependencies (macOS) ---
if(APPLE)
    find_library(CORE_GRAPHICS CoreGraphics)
    find_library(AUDIO_TOOLBOX AudioToolbox)
    find_library(CORE_FOUNDATION CoreFoundation)
    find_library(CORE_AUDIO CoreAudio)
    find_library(ACCELERATE Accelerate)

    target_link_libraries(kream_engine PRIVATE
        ${CORE_GRAPHICS}
        ${AUDIO_TOOLBOX}
        ${CORE_FOUNDATION}
        ${CORE_AUDIO}
        ${ACCELERATE}
    )
endif()

# --- 7. Post-Build Actions ---
# Automatically copy assets to the build directory after a successful build
add_custom_command(TARGET kream_engine POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E make_directory ${CMAKE_BINARY_DIR}/assets/themes
    COMMAND ${CMAKE_COMMAND} -E copy_directory
        ${CMAKE_CURRENT_SOURCE_DIR}/assets/themes/Creamy
        ${CMAKE_BINARY_DIR}/assets/themes/Creamy
    COMMENT "Copying assets to build output directory..."
)

# --- 8. Utility Targets ---
# Optional 'run' target for convenience
add_custom_target(run
    DEPENDS kream_engine
    COMMAND kream_engine
    WORKING_DIRECTORY ${CMAKE_BINARY_DIR}
    COMMENT "Executing kream_engine..."
)
