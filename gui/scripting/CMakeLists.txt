cmake_minimum_required(VERSION 3.20)
project(meshi_scripting LANGUAGES C CXX)

include(FetchContent)

FetchContent_Declare(
  meshi
  GIT_REPOSITORY https://github.com/JordanHendl/meshi
  GIT_TAG main
)

FetchContent_MakeAvailable(meshi)

set(MESHI_CAPI_DIR "${meshi_SOURCE_DIR}/capi/meshi-rs")
set(MESHI_SCRIPTING_INCLUDE_ROOT "${CMAKE_CURRENT_BINARY_DIR}/include")
set(MESHI_SCRIPTING_INCLUDE_DIR "${MESHI_SCRIPTING_INCLUDE_ROOT}/meshi")

file(MAKE_DIRECTORY "${MESHI_SCRIPTING_INCLUDE_DIR}")
file(
  COPY
    "${MESHI_CAPI_DIR}/meshi.h"
    "${MESHI_CAPI_DIR}/meshi_types.h"
  DESTINATION
    "${MESHI_SCRIPTING_INCLUDE_DIR}"
)

function(add_meshi_script target)
  add_library(${target} SHARED ${ARGN})
  target_include_directories(${target} PRIVATE "${MESHI_SCRIPTING_INCLUDE_ROOT}")
  set_target_properties(${target} PROPERTIES POSITION_INDEPENDENT_CODE ON)
endfunction()
