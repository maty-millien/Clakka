cmake_minimum_required(VERSION 3.10)
project(kream_engine VERSION 1.0 LANGUAGES CXX)
if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()
set(CMAKE_INTERPROCEDURAL_OPTIMIZATION ON)

# Set C++ standard
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Compiler flags
add_compile_options(-Wall -O3 -ffast-math -march=native -DNDEBUG -flto -funroll-loops)

# Include directories
include_directories(
    ${PROJECT_SOURCE_DIR}/src
    ${PROJECT_SOURCE_DIR}/lib
)

# Source files
file(GLOB_RECURSE SOURCES
    "${PROJECT_SOURCE_DIR}/src/*.cpp"
)

# Executable target
add_executable(${PROJECT_NAME} ${SOURCES})

# Link macOS frameworks
target_link_libraries(${PROJECT_NAME}
    "-framework CoreGraphics"
    "-framework AudioToolbox"
    "-framework CoreFoundation"
    "-framework CoreAudio"
    "-framework Accelerate"
)

# Custom 'run' target: build, copy assets and run
add_custom_target(run
    DEPENDS ${PROJECT_NAME}
    COMMAND ${CMAKE_COMMAND} -E make_directory ${CMAKE_BINARY_DIR}/assets/themes
    COMMAND ${CMAKE_COMMAND} -E copy_directory ${PROJECT_SOURCE_DIR}/assets/themes/Creamy ${CMAKE_BINARY_DIR}/assets/themes/Creamy
    COMMAND ${CMAKE_COMMAND} -E chdir ${CMAKE_BINARY_DIR} ./${PROJECT_NAME}
)
