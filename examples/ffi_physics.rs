use meshi::*;
use meshi::physics::{ActorStatus, RigidBodyInfo};
use glam::{Vec3, Quat};
use std::ffi::CString;

fn main() {
    let app = CString::new("Example").unwrap();
    let loc = CString::new(".").unwrap();
    let info = MeshiEngineInfo { application_name: app.as_ptr(), application_location: loc.as_ptr() };
    let engine = unsafe { meshi_make_engine(&info) };
    let physics = unsafe { meshi_get_physics_system(engine) };
    let rb = unsafe { meshi_physx_create_rigid_body(physics, &RigidBodyInfo::default()) };
    let status = ActorStatus { position: Vec3::new(1.0, 0.0, 0.0), rotation: Quat::IDENTITY };
    unsafe { meshi_physx_set_rigid_body_transform(physics, &rb, &status); }
    let mut out = ActorStatus::default();
    unsafe { meshi_physx_get_rigid_body_status(physics, &rb, &mut out); }
    println!("Rigid body position: {} {} {}", out.position.x, out.position.y, out.position.z);
    unsafe { meshi_physx_release_rigid_body(physics, &rb); }
    unsafe { meshi_destroy_engine(engine); }
}
