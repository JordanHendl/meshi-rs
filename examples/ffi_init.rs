use glam::Mat4;
use meshi::*;
use std::ffi::CString;

fn main() {
    let app = CString::new("Example").unwrap();
    let loc = CString::new(".").unwrap();
    let engine = unsafe { meshi_make_engine_headless(app.as_ptr(), loc.as_ptr()) };
    let render = unsafe { meshi_get_graphics_system(engine) };
    let cube = unsafe { meshi_gfx_create_cube(render) };
    unsafe {
        meshi_gfx_set_renderable_transform(render, cube, &Mat4::IDENTITY);
    }
    unsafe {
        meshi_destroy_engine(engine);
    }
}
