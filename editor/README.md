# Meshi Editor (Rust)

This editor crate is the Rust host application. It is responsible for:

- Bootstrapping the Meshi engine and GUI rendering backend.
- Building and submitting GUI frames using Meshi's in-engine GUI library.
- Hosting editor tools (scene graph, inspector, asset browser, etc.).
- Generating and launching **user runtime code** that is written in C++ and links
  against the Meshi C++ wrapper library from GitHub.

## Architecture overview

### Rust editor host

The editor itself is a Rust application that uses the Meshi GUI backend provided
by the `meshi_graphics` crate. The editor should own the frame loop and build
GUI frames each tick.

Suggested module layout:

- `editor/src/main.rs`: entry point, engine bootstrap, main loop, dispatch.
- `editor/src/ui/`: editor UI composition (menus, panels, layout).
- `editor/src/scene/`: editor scene graph and selection state.
- `editor/src/assets/`: asset browser and import pipeline.
- `editor/src/project/`: project discovery, configuration, workspace state.
- `editor/src/runtime/`: glue for launching C++ user runtime builds.

### C++ user runtime

User-authored runtime/game code is written in C++ and built against the Meshi
C++ wrapper library:

- GitHub: https://github.com/JordanHendl/meshi

The editor should generate a user project template that links to the Meshi
wrapper and calls into the Meshi plugin entry points. The Rust editor should
invoke the build/run pipeline for this C++ project when the user presses “Play”.

## Immediate tasks

1. **Wire Meshi GUI**
   - Add `meshi_graphics` as a dependency and create a GUI frame each tick.
   - Submit the frame to the engine in the render loop.

2. **Editor UI layout**
   - Implement menu bar, viewport panel, hierarchy panel, inspector panel,
     asset browser, and log/console panel.

3. **C++ runtime template**
   - Provide a C++ starter project that uses the Meshi wrapper library.
   - Wire the editor to build/run this project as the user runtime.

4. **Plugin bridge**
   - Ensure the plugin C API exposes required hooks for GUI submission and input.
   - Wrap those hooks in the C++ wrapper and Rust editor host.

## C++ runtime template

A minimal template is provided under `editor/templates/cpp/meshi_app`. This
project is intentionally small and meant to be copied or generated into a user
workspace.
