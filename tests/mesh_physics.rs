use glam::{Mat4, Quat, Vec3};
use meshi::physics::{ActorStatus, RigidBodyInfo};
use meshi::*;
use std::ffi::CString;

fn main() {
    if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
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

    // Graphics mesh object
    let render = unsafe { meshi_get_graphics_system(engine) };
    let cube = unsafe { meshi_gfx_create_cube(render) };
    let transform = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
    unsafe {
        meshi_gfx_set_renderable_transform(render, cube, &transform);
    }

    // Physics rigid body
    let physics = unsafe { meshi_get_physics_system(engine) };
    let rb = unsafe { meshi_physx_create_rigid_body(physics, &RigidBodyInfo::default()) };
    let new_status = ActorStatus {
        position: Vec3::new(4.0, 5.0, 6.0),
        rotation: Quat::IDENTITY,
    };
    unsafe {
        meshi_physx_set_rigid_body_transform(physics, &rb, &new_status);
    }
    let mut out = ActorStatus::default();
    unsafe {
        meshi_physx_get_rigid_body_status(physics, &rb, &mut out);
    }
    assert_eq!(out.position, new_status.position);
    assert_eq!(out.rotation, new_status.rotation);

    unsafe {
        meshi_physx_release_rigid_body(physics, &rb);
    }
    unsafe {
        meshi_destroy_engine(engine);
    }
}
