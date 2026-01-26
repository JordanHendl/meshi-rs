use std::{collections::BTreeSet, fs, path::Path};

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn parse_identifier_at_start(input: &str) -> Option<String> {
    let mut ident = String::new();
    for ch in input.chars() {
        if is_ident_char(ch) {
            ident.push(ch);
        } else {
            break;
        }
    }
    if ident.is_empty() {
        None
    } else {
        Some(ident)
    }
}

fn parse_last_identifier(input: &str) -> Option<String> {
    let mut chars = input.chars().rev();
    while let Some(ch) = chars.next() {
        if is_ident_char(ch) {
            let mut ident = String::new();
            ident.push(ch);
            for ch in chars.by_ref() {
                if is_ident_char(ch) {
                    ident.push(ch);
                } else {
                    break;
                }
            }
            return Some(ident.chars().rev().collect());
        }
    }
    None
}

fn exported_functions_from_rust(contents: &str) -> BTreeSet<String> {
    let mut exports = BTreeSet::new();
    for line in contents.lines() {
        if let Some(rest) = line.split("pub extern \"C\" fn").nth(1) {
            let rest = rest.trim_start();
            if let Some(name) = parse_identifier_at_start(rest) {
                exports.insert(name);
            }
        }
    }
    exports
}

fn declared_functions_from_header(contents: &str) -> BTreeSet<String> {
    let mut functions = BTreeSet::new();
    for statement in contents.split(';') {
        let statement = statement.trim();
        if statement.is_empty() {
            continue;
        }
        if statement.starts_with("typedef") || statement.starts_with('#') {
            continue;
        }
        if statement.contains('{') {
            continue;
        }
        if statement.contains("(*") {
            continue;
        }
        if !statement.contains('(') || !statement.contains(')') {
            continue;
        }
        let before_paren = match statement.split('(').next() {
            Some(value) => value,
            None => continue,
        };
        if let Some(name) = parse_last_identifier(before_paren) {
            functions.insert(name);
        }
    }
    functions
}

#[test]
fn capi_headers_match_rust_exports() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rust_path = manifest_dir.join("src").join("lib.rs");
    let rust_contents = fs::read_to_string(&rust_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", rust_path.display()));
    let rust_exports = exported_functions_from_rust(&rust_contents);

    let header_dir = manifest_dir.join("..").join("capi").join("meshi-rs");
    let mut header_exports = BTreeSet::new();
    for entry in fs::read_dir(&header_dir)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", header_dir.display()))
    {
        let entry = entry.expect("failed to read header entry");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("h") {
            continue;
        }
        let contents = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        header_exports.extend(declared_functions_from_header(&contents));
    }

    let missing_in_headers: Vec<_> = rust_exports
        .difference(&header_exports)
        .cloned()
        .collect();
    let extra_in_headers: Vec<_> = header_exports
        .difference(&rust_exports)
        .cloned()
        .collect();

    assert!(
        rust_exports.contains("meshi_plugin_get_api")
            && header_exports.contains("meshi_plugin_get_api"),
        "meshi_plugin_get_api must be exported by Rust and declared in headers"
    );

    assert!(
        missing_in_headers.is_empty() && extra_in_headers.is_empty(),
        "C headers do not match Rust exports.\nMissing in headers: {missing_in_headers:?}\nExtra in headers: {extra_in_headers:?}"
    );
}
