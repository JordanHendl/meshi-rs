use dashi::utils::Handle;
use glam::Mat4;
use meshi::render::RenderBackend;
use meshi::*;
use std::ffi::CString;

#[test]
fn set_transform_with_invalid_handle_does_nothing() {
    let name = CString::new("test").unwrap_or_default();
    let loc = CString::new(".").unwrap_or_default();
    let info = MeshiEngineInfo {
        application_name: name.as_ptr(),
        application_location: loc.as_ptr(),
        headless: 1,
        render_backend: RenderBackend::Canvas,
        canvas_extent: std::ptr::null(),
    };
    let engine = unsafe { meshi_make_engine(&info) };
    assert!(!engine.is_null());
    let render = unsafe { meshi_get_graphics_system(engine) };
    assert!(!render.is_null());

    let invalid = Handle::default();
    let transform = Mat4::IDENTITY;
    unsafe { meshi_gfx_set_renderable_transform(render, invalid, &transform) };

    let cube = unsafe { meshi_gfx_create_cube(render) };
    unsafe { meshi_gfx_set_renderable_transform(render, cube, &transform) };

    unsafe {
        meshi_gfx_release_mesh_object(render, &cube);
        meshi_destroy_engine(engine);
    }
}
