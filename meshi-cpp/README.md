#[meshi]

This is the C++ front end of the meshi project.

This links against the plugin interface and provides helpers for a game engine.

## Build script

From any build directory, run:

```bash
/path/to/meshi-cpp/build.sh /path/to/meshi-cpp
```

If no source path is provided, the script defaults to the `meshi-cpp` directory that contains the script. The script configures CMake with Ninja and `CMAKE_EXPORT_COMPILE_COMMANDS=TRUE` in the current directory, then builds.
