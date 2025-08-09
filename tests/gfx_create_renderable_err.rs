use dashi::utils::Handle;
use glam::Mat4;
use meshi::{render::RenderBackend, *};
use std::ffi::CString;

#[test]
fn invalid_info_returns_default_handle() {
    let name = CString::new("test").unwrap_or_default();
    let loc = CString::new(".").unwrap_or_default();
    let info = MeshiEngineInfo {
        application_name: name.as_ptr(),
        application_location: loc.as_ptr(),
        headless: 1,
        render_backend: RenderBackend::Canvas,
    };
    let engine = unsafe { meshi_make_engine(&info) };
    assert!(!engine.is_null());
    let render = unsafe { meshi_get_graphics_system(engine) };
    assert!(!render.is_null());

    let bad = FFIMeshObjectInfo {
        mesh: std::ptr::null(),
        material: std::ptr::null(),
        transform: Mat4::IDENTITY,
    };
    let handle = unsafe { meshi_gfx_create_renderable(render, &bad) };
    let default = Handle::<()>::default();
    assert_eq!(handle.slot, default.slot);
    assert_eq!(handle.generation, default.generation);

    unsafe { meshi_destroy_engine(engine) };
}
