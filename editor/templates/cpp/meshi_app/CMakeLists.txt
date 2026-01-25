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

add_executable(meshi_app main.cpp)

target_link_libraries(meshi_app PRIVATE meshi_wrapper)
