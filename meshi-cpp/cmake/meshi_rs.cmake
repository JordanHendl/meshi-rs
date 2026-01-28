set(MESHI_RS_INCLUDE_DIR ${CMAKE_SOURCE_DIR}/../capi/meshi-rs CACHE PATH "Path to meshi-rs C headers")

add_library(meshi-rs INTERFACE)
target_include_directories(meshi-rs INTERFACE ${MESHI_RS_INCLUDE_DIR})
