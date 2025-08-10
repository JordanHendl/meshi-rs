use std::path::PathBuf;
use image::RgbaImage;

/// Save the provided images under `target/test_images` for manual inspection
/// and assert that their raw pixel data matches.
pub fn assert_images_eq(name: &str, actual: &RgbaImage, expected: &RgbaImage) {
    save_images(name, actual, expected);
    assert_eq!(actual.as_raw(), expected.as_raw());
}

fn save_images(name: &str, actual: &RgbaImage, expected: &RgbaImage) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test_images");
    std::fs::create_dir_all(&dir).expect("create test_images directory");
    let name = name.replace("::", "_");
    actual
        .save(dir.join(format!("{name}_actual.png")))
        .expect("save actual image");
    expected
        .save(dir.join(format!("{name}_expected.png")))
        .expect("save expected image");
}
