# Version minimale de CMake requise
cmake_minimum_required(VERSION 3.10)

# Nom du projet
project(KreamEngine VERSION 1.0)

# Définir le standard C++ (C++17 est un bon choix)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD_LIBRARIES "c++")
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -stdlib=libc++")

# Activer le support pour les frameworks macOS
set(CMAKE_FIND_FRAMEWORK "LAST")
set(CMAKE_OSX_DEPLOYMENT_TARGET "10.15" CACHE STRING "Minimum OS X deployment version")

# Trouver les frameworks macOS dont nous avons besoin
# CoreFoundation: Fonctions de base du système
# CoreGraphics: Pour l'écoute des événements clavier
# AudioToolbox: Pour l'audio basse latence (Core Audio)
find_library(CORE_FOUNDATION_FRAMEWORK CoreFoundation)
find_library(CORE_GRAPHICS_FRAMEWORK CoreGraphics)
find_library(AUDIO_TOOLBOX_FRAMEWORK AudioToolbox)

# Créer l'exécutable à partir de nos fichiers source
add_executable(kream_engine
    src/main.cpp
)

# Lier notre exécutable avec les frameworks macOS
target_link_libraries(kream_engine PRIVATE
    "${CORE_FOUNDATION_FRAMEWORK}"
    "${CORE_GRAPHICS_FRAMEWORK}"
    "${AUDIO_TOOLBOX_FRAMEWORK}"
    c++
)
