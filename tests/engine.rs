use glam::Mat4;
use meshi::render::{
    database::geometry_primitives::{
        ConePrimitiveInfo, CubePrimitiveInfo, CylinderPrimitiveInfo, PlanePrimitiveInfo,
        SpherePrimitiveInfo,
    },
    RenderBackend,
};
use meshi::*;
use std::ffi::CString;

fn main() {
    let name = CString::new("test").unwrap();
    let loc = CString::new(".").unwrap();
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
    unsafe {
        meshi_gfx_set_camera(render, &Mat4::IDENTITY);
        meshi_gfx_set_projection(render, &Mat4::IDENTITY);
        meshi_gfx_capture_mouse(render, 1);
        meshi_gfx_capture_mouse(render, 0);
    }
    let cube_info = CubePrimitiveInfo { size: 1.0 };
    unsafe { meshi_gfx_create_cube_ex(render, &cube_info) };
    let sphere_info = SpherePrimitiveInfo {
        radius: 1.0,
        segments: 8,
        rings: 8,
    };
    unsafe { meshi_gfx_create_sphere_ex(render, &sphere_info) };
    let cylinder_info = CylinderPrimitiveInfo {
        radius: 1.0,
        height: 2.0,
        segments: 8,
    };
    unsafe { meshi_gfx_create_cylinder_ex(render, &cylinder_info) };
    let plane_info = PlanePrimitiveInfo { size: 1.0 };
    unsafe { meshi_gfx_create_plane_ex(render, &plane_info) };
    let cone_info = ConePrimitiveInfo {
        radius: 1.0,
        height: 2.0,
        segments: 8,
    };
    unsafe { meshi_gfx_create_cone_ex(render, &cone_info) };
    unsafe { meshi_update(engine) };
}
