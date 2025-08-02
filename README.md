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

## Examples

Sample code demonstrating the FFI interface lives in the `examples/` directory. Run an example with:

```bash
cargo run --example ffi_init
```

Replace `ffi_init` with `ffi_physics` to explore the physics example.

