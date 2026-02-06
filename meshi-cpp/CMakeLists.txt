cmake_minimum_required(VERSION 3.13)
project(meshi-cpp)

list(APPEND CMAKE_MODULE_PATH ${CMAKE_SOURCE_DIR}/cmake)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)

set(SAMPLE_DATABASE_SOURCE ${CMAKE_SOURCE_DIR}/../sample_database)
set(SAMPLE_DATABASE_DIR ${CMAKE_BINARY_DIR}/sample_database)
if(EXISTS ${SAMPLE_DATABASE_SOURCE})
  FILE(COPY ${SAMPLE_DATABASE_SOURCE} DESTINATION ${CMAKE_BINARY_DIR})
else()
  message(FATAL_ERROR "Sample database not found at ${SAMPLE_DATABASE_SOURCE}")
endif()
include(meshi_rs)

include(FetchContent)
FetchContent_Declare(
  glm
  GIT_REPOSITORY https://github.com/g-truc/glm.git
  GIT_TAG 7a878c2
)

FetchContent_MakeAvailable(glm)

set(SAMPLE_APP_DIR ${SAMPLE_DATABASE_DIR})
add_subdirectory(src)
