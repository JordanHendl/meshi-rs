use glam::Vec3;
use meshi::physics::{PhysicsSimulation, RigidBodyInfo, SimulationInfo};

#[test]
fn spheres_generate_contact() {
    let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
    let rb1 = sim.create_rigid_body(&RigidBodyInfo {
        initial_position: Vec3::ZERO,
        has_gravity: 0,
        ..Default::default()
    });
    let rb2 = sim.create_rigid_body(&RigidBodyInfo {
        initial_position: Vec3::new(0.5, 0.0, 0.0),
        has_gravity: 0,
        ..Default::default()
    });

    sim.update(0.0);
    let contacts = sim.get_contacts();
    assert!(contacts
        .iter()
        .any(|c| (c.a == rb1 && c.b == rb2) || (c.a == rb2 && c.b == rb1)));
}
