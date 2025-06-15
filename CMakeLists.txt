cmake_minimum_required(VERSION 3.10)

project(KreamEngine VERSION 1.0)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD_LIBRARIES "c++")
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -stdlib=libc++")

set(CMAKE_FIND_FRAMEWORK "LAST")
set(CMAKE_OSX_DEPLOYMENT_TARGET "10.15" CACHE STRING "Minimum OS X deployment version")

find_library(CORE_FOUNDATION_FRAMEWORK CoreFoundation)
find_library(CORE_GRAPHICS_FRAMEWORK CoreGraphics)
find_library(AUDIO_TOOLBOX_FRAMEWORK AudioToolbox)

add_executable(kream_engine
    src/main.cpp
)

target_link_libraries(kream_engine PRIVATE
    "${CORE_FOUNDATION_FRAMEWORK}"
    "${CORE_GRAPHICS_FRAMEWORK}"
    "${AUDIO_TOOLBOX_FRAMEWORK}"
    c++
)
