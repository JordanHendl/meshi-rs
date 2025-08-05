use glam::Vec3;
use meshi::physics::{MaterialInfo, PhysicsSimulation, RigidBodyInfo, SimulationInfo};

#[test]
fn physics_update_applies_gravity_and_damping() {
    let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
    let rb = sim.create_rigid_body(&RigidBodyInfo {
        has_gravity: 1,
        ..Default::default()
    });

    let dt = 1.0f32;
    sim.update(dt).unwrap();

    let status = sim
        .get_rigid_body_status(rb)
        .expect("rigid body should be valid");
    let velocity = sim
        .get_rigid_body_velocity(rb)
        .expect("rigid body should be valid");

    let g = -9.8f32;
    let friction = MaterialInfo::default().dynamic_friction_m;
    let expected_position_y = g * dt * dt;
    let expected_velocity_y = (g - friction) * dt;

    assert!((status.position.y - expected_position_y).abs() < 1e-5);
    assert!((velocity.y - expected_velocity_y).abs() < 1e-5);
}

#[test]
fn static_friction_stops_motion() {
    let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
    let mat = sim.create_material(&MaterialInfo {
        dynamic_friction_m: 0.0,
        static_friction_m: 1.0,
        restitution: 0.0,
    });
    let rb = sim.create_rigid_body(&RigidBodyInfo {
        material: mat,
        initial_velocity: Vec3::splat(0.5),
        has_gravity: 0,
        ..Default::default()
    });
    sim.update(1.0).unwrap();
    let velocity = sim
        .get_rigid_body_velocity(rb)
        .expect("rigid body should be valid");
    assert!(velocity.length_squared() == 0.0);
}
