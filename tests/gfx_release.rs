use meshi::render::{DirectionalLightInfo, RenderBackend};
use meshi::*;
use std::ffi::CString;

fn main() {
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
    let render = unsafe { meshi_get_graphics_system(engine) };

    // Mesh object release
    let cube1 = unsafe { meshi_gfx_create_cube(render) };
    unsafe { meshi_gfx_release_mesh_object(render, &cube1) };
    let cube2 = unsafe { meshi_gfx_create_cube(render) };
    assert_eq!(cube1.slot, cube2.slot);

    // Directional light release
    let light_info = DirectionalLightInfo::default();
    let light1 = unsafe { meshi_gfx_create_directional_light(render, &light_info) };
    unsafe { meshi_gfx_release_directional_light(render, &light1) };
    let light2 = unsafe { meshi_gfx_create_directional_light(render, &light_info) };
    assert_eq!(light1.slot, light2.slot);
}
