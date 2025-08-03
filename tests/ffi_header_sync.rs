use std::fs;

#[test]
fn ffi_header_sync() {
    let root = env!("CARGO_MANIFEST_DIR");
    let lib_rs =
        fs::read_to_string(format!("{}/src/lib.rs", root)).expect("failed to read src/lib.rs");
    let header = fs::read_to_string(format!("{}/include/meshi/meshi.h", root))
        .expect("failed to read include/meshi/meshi.h");

    let mut missing = Vec::new();
    for line in lib_rs.lines() {
        if let Some(start) = line.find("pub extern \"C\" fn") {
            let after = &line[start + "pub extern \"C\" fn".len()..];
            let name = after
                .split(|c: char| c == '(' || c.is_whitespace())
                .next()
                .unwrap();
            if !header.contains(name) {
                missing.push(name.to_string());
            }
        }
    }

    assert!(missing.is_empty(), "Missing in meshi.h: {:?}", missing);
}
