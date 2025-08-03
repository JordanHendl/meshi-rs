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

