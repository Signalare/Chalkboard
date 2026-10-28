# SPDX-License-Identifier: MIT OR Apache-2.0

cmake_minimum_required(VERSION 3.20)
project(chalkboard-render VERSION 0.0.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

add_compile_options(-Wall -Wextra -Werror -Wpedantic)

# Platform-independent renderer core (RND-005).
add_library(cb_render_core src/core/version.c)
target_include_directories(cb_render_core PUBLIC include)
target_compile_definitions(cb_render_core PRIVATE CB_RENDER_VERSION="${PROJECT_VERSION}")

# DRM/KMS backend: the on-device executable.
add_executable(chalkboard-render src/backends/drm/main.c)
target_link_libraries(chalkboard-render PRIVATE cb_render_core)

enable_testing()
add_executable(test_version tests/test_version.c)
target_link_libraries(test_version PRIVATE cb_render_core)
add_test(NAME version COMMAND test_version)
