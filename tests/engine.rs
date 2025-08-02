use meshi::*;
use std::ffi::CString;

fn main() {
    let name = CString::new("test").unwrap();
    let loc = CString::new(".").unwrap();
    let info = MeshiEngineInfo {
        application_name: name.as_ptr(),
        application_location: loc.as_ptr(),
        headless: 1,
    };
    let engine = unsafe { meshi_make_engine(&info) };
    assert!(!engine.is_null());
    unsafe {
        meshi_update(engine);
    }
}
