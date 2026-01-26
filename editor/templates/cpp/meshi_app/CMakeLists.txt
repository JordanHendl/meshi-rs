cmake_minimum_required(VERSION 3.16)
project(meshi_app LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Point this at the Meshi C++ wrapper repository.
# Example: cmake -DMESHI_WRAPPER_DIR=/path/to/meshi ..
set(MESHI_WRAPPER_DIR "" CACHE PATH "Path to the Meshi C++ wrapper repo")

if (NOT MESHI_WRAPPER_DIR)
  message(FATAL_ERROR "MESHI_WRAPPER_DIR is not set. Provide the path to the Meshi C++ wrapper repo.")
endif()

add_subdirectory(${MESHI_WRAPPER_DIR} meshi_wrapper)

option(MESHI_USE_GITHUB_RELEASE_PLUGIN "Download the Meshi plugin from GitHub releases instead of using a local build." OFF)
set(MESHI_RS_DIR "${CMAKE_CURRENT_LIST_DIR}/../../.." CACHE PATH "Path to the meshi-rs repository (for local plugin builds).")

if (WIN32)
  set(MESHI_PLUGIN_FILENAME "meshi.dll")
elseif(APPLE)
  set(MESHI_PLUGIN_FILENAME "libmeshi.dylib")
else()
  set(MESHI_PLUGIN_FILENAME "libmeshi.so")
endif()

if (CMAKE_BUILD_TYPE STREQUAL "Release")
  set(MESHI_PLUGIN_BUILD_PROFILE "release")
else()
  set(MESHI_PLUGIN_BUILD_PROFILE "debug")
endif()

set(MESHI_PLUGIN_PATH
    "${MESHI_RS_DIR}/target/${MESHI_PLUGIN_BUILD_PROFILE}/${MESHI_PLUGIN_FILENAME}"
    CACHE FILEPATH
    "Path to the Meshi plugin library.")

if (MESHI_USE_GITHUB_RELEASE_PLUGIN)
  set(MESHI_PLUGIN_RELEASE_URL "" CACHE STRING "GitHub release URL for the prebuilt Meshi plugin library.")
  if (NOT MESHI_PLUGIN_RELEASE_URL)
    message(FATAL_ERROR "MESHI_PLUGIN_RELEASE_URL is required when MESHI_USE_GITHUB_RELEASE_PLUGIN=ON.")
  endif()

  set(MESHI_PLUGIN_DOWNLOAD_DIR "${CMAKE_BINARY_DIR}/meshi_plugin_release")
  file(MAKE_DIRECTORY "${MESHI_PLUGIN_DOWNLOAD_DIR}")
  set(MESHI_PLUGIN_PATH "${MESHI_PLUGIN_DOWNLOAD_DIR}/${MESHI_PLUGIN_FILENAME}")

  if (NOT EXISTS "${MESHI_PLUGIN_PATH}")
    message(STATUS "Downloading Meshi plugin from ${MESHI_PLUGIN_RELEASE_URL}")
    file(DOWNLOAD "${MESHI_PLUGIN_RELEASE_URL}" "${MESHI_PLUGIN_PATH}" SHOW_PROGRESS)
  endif()
endif()

message(STATUS "Meshi plugin path: ${MESHI_PLUGIN_PATH}")

add_executable(meshi_app main.cpp)

target_link_libraries(meshi_app PRIVATE meshi_wrapper)

target_compile_definitions(meshi_app PRIVATE MESHI_PLUGIN_PATH=\"${MESHI_PLUGIN_PATH}\")
