set(MESHI_RS_DIR ${CMAKE_SOURCE_DIR}/.. CACHE PATH "Path to the meshi-rs repository")
set(MESHI_RS_INCLUDE_DIR ${MESHI_RS_DIR}/capi/meshi-rs CACHE PATH "Path to meshi-rs C headers")

if (WIN32)
  set(MESHI_RS_PLUGIN_FILENAME "meshi.dll")
elseif(APPLE)
  set(MESHI_RS_PLUGIN_FILENAME "libmeshi.dylib")
else()
  set(MESHI_RS_PLUGIN_FILENAME "libmeshi.so")
endif()

if (CMAKE_BUILD_TYPE STREQUAL "Release")
  set(MESHI_RS_PLUGIN_PROFILE "release")
else()
  set(MESHI_RS_PLUGIN_PROFILE "debug")
endif()

set(MESHI_RS_PLUGIN_SOURCE
    "${MESHI_RS_DIR}/target/${MESHI_RS_PLUGIN_PROFILE}/${MESHI_RS_PLUGIN_FILENAME}"
    CACHE FILEPATH
    "Path to the meshi-rs plugin build artifact.")

option(MESHI_RS_COPY_PLUGIN "Copy meshi-rs plugin into the build output directory for local runs." ON)

add_library(meshi-rs INTERFACE)
target_include_directories(meshi-rs INTERFACE ${MESHI_RS_INCLUDE_DIR})

if (MESHI_RS_COPY_PLUGIN)
  set(MESHI_RS_PLUGIN_DESTINATION
      "${CMAKE_RUNTIME_OUTPUT_DIRECTORY}/${MESHI_RS_PLUGIN_FILENAME}")
  set(MESHI_RS_PLUGIN_PATH "${MESHI_RS_PLUGIN_DESTINATION}")
  add_custom_target(
      meshi_rs_plugin_copy ALL
      COMMAND ${CMAKE_COMMAND} -E copy_if_different
              "${MESHI_RS_PLUGIN_SOURCE}"
              "${MESHI_RS_PLUGIN_DESTINATION}"
      COMMENT "Copying meshi-rs plugin to ${MESHI_RS_PLUGIN_DESTINATION}"
  )
else()
  set(MESHI_RS_PLUGIN_PATH "${MESHI_RS_PLUGIN_SOURCE}")
endif()
