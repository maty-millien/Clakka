#pragma once

#include <iostream>
#include <csignal>
#include <atomic>
#include <thread>
#include <chrono>
#include <dirent.h>
#include <unordered_map>
#include <string>
#include <CoreAudio/CoreAudio.h>
#include <mach/mach_time.h>
#include <Accelerate/Accelerate.h>
#include <cstring>
#include <pthread.h>
#include <mach/thread_policy.h>
#include <mach/thread_act.h>
#include <vector>
#include <AudioToolbox/AudioToolbox.h>
#include <cstdint>
#include <CoreGraphics/CoreGraphics.h>
#include <CoreFoundation/CoreFoundation.h>

#include "AudioEngine/AudioEngine.hpp"
