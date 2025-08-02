use meshi::*;
use std::ffi::CString;

fn main() {
    if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
        // Headless environment; skip test.
        return;
    }
    let name = CString::new("test").unwrap();
    let loc = CString::new(".").unwrap();
    let info = MeshiEngineInfo {
        application_name: name.as_ptr(),
        application_location: loc.as_ptr(),
        headless: 0,
    };
    let engine = unsafe { meshi_make_engine(&info) };
    assert!(!engine.is_null());
    unsafe {
        meshi_destroy_engine(engine);
    }
}
