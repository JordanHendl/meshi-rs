use glam::{Mat4, Vec4};
use meshi::render::{database::geometry_primitives::CubePrimitiveInfo, DirectionalLightInfo};
use meshi::*;
use std::ffi::CString;

fn main() {
    let app = CString::new("Example").unwrap();
    let loc = CString::new(".").unwrap();
    let engine = unsafe { meshi_make_engine_headless(app.as_ptr(), loc.as_ptr()) };
    let render = unsafe { meshi_get_graphics_system(engine) };
    let info = CubePrimitiveInfo { size: 2.0 };
    let cube = unsafe { meshi_gfx_create_cube_ex(render, &info) };
    unsafe { meshi_gfx_set_renderable_transform(render, cube, &Mat4::IDENTITY) };

    let light_info = DirectionalLightInfo {
        direction: Vec4::new(0.0, -1.0, 0.0, 0.0),
        color: Vec4::splat(1.0),
        intensity: 1.0,
    };
    let light = unsafe { meshi_gfx_create_directional_light(render, &light_info) };
    unsafe { meshi_update(engine) };
    let mut warm = light_info;
    warm.color = Vec4::new(1.0, 0.5, 0.5, 1.0);
    warm.intensity = 0.5;
    unsafe { meshi_gfx_set_directional_light_info(render, light, &warm) };
    unsafe { meshi_update(engine) };
    unsafe {
        meshi_destroy_engine(engine);
    }
}
