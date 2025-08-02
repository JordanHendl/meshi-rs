use meshi::physics::{MaterialInfo, PhysicsSimulation, RigidBodyInfo, SimulationInfo};

#[test]
fn physics_update_applies_gravity_and_damping() {
    let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
    let rb = sim.create_rigid_body(&RigidBodyInfo {
        has_gravity: 1,
        ..Default::default()
    });

    let dt = 1.0f32;
    sim.update(dt);

    let status = sim.get_rigid_body_status(rb);
    let velocity = sim.get_rigid_body_velocity(rb);

    let g = -9.8f32;
    let friction = MaterialInfo::default().dynamic_friction_m;
    let expected_position_y = g * dt * dt;
    let expected_velocity_y = (g - friction) * dt;

    assert!((status.position.y - expected_position_y).abs() < 1e-5);
    assert!((velocity.y - expected_velocity_y).abs() < 1e-5);
}
