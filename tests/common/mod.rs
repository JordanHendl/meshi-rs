use std::{fs::File, io::Read, os::unix::io::FromRawFd, path::PathBuf};
use image::RgbaImage;

/// Guard that enables Vulkan validation layers and captures messages written to
/// `stderr`. When dropped, it asserts that no validation errors or warnings were
/// emitted.
pub struct ValidationGuard {
    read_fd: i32,
    stderr_fd: i32,
}

impl ValidationGuard {
    pub fn new() -> Self {
        std::env::set_var("DASHI_VALIDATION", "1");
        unsafe {
            let stderr_fd = libc::dup(libc::STDERR_FILENO);
            assert!(stderr_fd >= 0, "dup stderr failed");
            let mut fds = [0; 2];
            assert_eq!(libc::pipe(fds.as_mut_ptr()), 0, "pipe failed");
            libc::dup2(fds[1], libc::STDERR_FILENO);
            libc::close(fds[1]);
            Self { read_fd: fds[0], stderr_fd }
        }
    }
}

impl Drop for ValidationGuard {
    fn drop(&mut self) {
        unsafe {
            libc::fflush(std::ptr::null_mut());
            libc::dup2(self.stderr_fd, libc::STDERR_FILENO);
            libc::close(self.stderr_fd);
            let mut file = File::from_raw_fd(self.read_fd);
            let mut output = String::new();
            let _ = file.read_to_string(&mut output);
            if output.contains("[ERROR]") || output.contains("[WARNING]") {
                panic!("Vulkan validation reported issues:\n{output}");
            }
        }
    }
}

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
