use std::path::Path;

use image::{DynamicImage, ImageError};

/// Decode an image from a file path.
///
/// The caller receives the fully decoded image so tests can inspect its
/// dimensions or pixel data without involving any GPU resources.
pub fn load_image_from_path<P: AsRef<Path>>(path: P) -> Result<DynamicImage, ImageError> {
    image::open(path)
}

/// Decode an image from an in-memory byte slice.
///
/// This is useful when images are embedded in containers such as glTF files.
pub fn load_image_from_bytes(bytes: &[u8]) -> Result<DynamicImage, ImageError> {
    image::load_from_memory(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgba, RgbaImage};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn load_from_path_and_bytes() {
        let dir = tempdir().unwrap();
        let img_path = dir.path().join("test.png");
        let img = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 4]));
        img.save(&img_path).unwrap();

        let from_path = load_image_from_path(&img_path).unwrap();
        assert_eq!(from_path.dimensions(), (2, 2));

        let bytes = fs::read(&img_path).unwrap();
        let from_bytes = load_image_from_bytes(&bytes).unwrap();
        assert_eq!(from_bytes.dimensions(), (2, 2));
    }
}
