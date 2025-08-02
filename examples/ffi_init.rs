use glam::Mat4;
use meshi::render::database::geometry_primitives::CubePrimitiveInfo;
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
    unsafe {
        meshi_destroy_engine(engine);
    }
}
