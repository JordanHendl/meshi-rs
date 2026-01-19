# Meshi scripting (C/C++)

This directory provides a standalone CMake project for building Meshi scripts as
shared libraries and keeping the Meshi C API headers available to script
projects.

## Entry points (ABI)

Scripts are expected to export the following C ABI entrypoints:

```c
// Called once when the script is loaded/registered.
void meshi_script_register(struct MeshiEngine* engine);

// Called every frame; return the delta time (or unused).
void meshi_script_update(struct MeshiEngine* engine, float delta_time);
```

If you implement your script in C++, make sure to wrap these in `extern "C"`
so the exported symbols match the C ABI.

## Building scripts

This directory fetches Meshi via `FetchContent`, copies the Meshi C API headers
into the build include path, and exposes a helper function for shared library
scripts.

```cmake
add_meshi_script(my_script
  src/my_script.c
)
```

### Example script implementation

```c
#include <meshi/meshi.h>

void meshi_script_register(struct MeshiEngine* engine) {
  (void)engine;
}

void meshi_script_update(struct MeshiEngine* engine, float delta_time) {
  (void)engine;
  (void)delta_time;
}
```

### Configure + build

```bash
cmake -S gui/scripting -B build/meshi-scripting
cmake --build build/meshi-scripting
```

The resulting shared library will be in the build output directory (for example
`build/meshi-scripting/libmy_script.so` on Linux).
