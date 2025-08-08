# meshi

This crate builds a dynamic library that can be consumed via FFI and also provides examples and tests.

## Dependencies

To build and run the library, ensure the following Rust crates and their native dependencies are available:

- [dashi](https://github.com/JordanHendl/dashi)
- [koji](https://github.com/JordanHendl/koji)

## Build

Compile the dynamic library in release mode:

```bash
cargo build --release
```

The library artifact will be placed in `target/release`.

## Test

Run the test suite to verify functionality:

```bash
cargo test
```

Whenever altering FFI signatures or structs, update `include/meshi/meshi.h` and `meshi_types.h`, then run `cargo test` to verify synchronization.

## Examples

Sample code demonstrating the FFI interface lives in the `examples/` directory. Run an example with:

```bash
cargo run --example ffi_init
```

Replace `ffi_init` with `ffi_physics` to explore the physics example.

The `simple_render` example opens a windowed `RenderEngine`, loads a small
model and texture from the `database/` folder, and renders a few frames to the
display. Only `model.gltf` is included in the repository; add `albedo.png` and
`data.bin` beside it before running:

```bash
cargo run --example simple_render
```

Pass `graph` as an argument to use the experimental graph backend instead of
the default canvas backend:

```bash
cargo run --example simple_render -- graph
```

To adjust a directional light's color or intensity at runtime via FFI:

```rust
let info = DirectionalLightInfo {
    direction: Vec4::new(0.0, -1.0, 0.0, 0.0),
    color: Vec4::splat(1.0),
    intensity: 1.0,
};
let light = unsafe { meshi_gfx_create_directional_light(render, &info) };
let mut warm = info;
warm.color = Vec4::new(1.0, 0.5, 0.5, 1.0);
warm.intensity = 0.5;
unsafe { meshi_gfx_set_directional_light_info(render, light, &warm) };
```

## Resource database

The rendering database now supports asynchronous model loading. Call
`load_model_async(name)` to spawn a loader thread that parses the glTF file and
uploads vertex and index data to GPU buffers. The returned `JoinHandle`
resolves to a `MeshResource` that can be stored or rendered once complete.

Loaded models can be removed with `unload_model(name)`, which drops their GPU
buffers and frees associated memory. Additionally, `fetch_mesh` now accepts a
`wait: bool` flag. When `wait` is `true` and the mesh is not yet resident, the
database will load it synchronously before returning.

Model paths may include a `#` selector to reference a specific mesh or
primitive inside a glTF file. Use `file.gltf#mesh_name` or
`file.gltf#1/0` to select a mesh by name or index and optionally a
primitive index. When no selector is provided the database loads the first
primitive of the first mesh.

