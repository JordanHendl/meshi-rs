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

#[test]
fn many_spheres_generate_expected_contacts() {
    let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
    let mut bodies = Vec::new();
    let count = 100;
    for i in 0..count {
        let rb = sim.create_rigid_body(&RigidBodyInfo {
            initial_position: Vec3::new(i as f32 * 1.5, 0.0, 0.0),
            has_gravity: 0,
            ..Default::default()
        });
        bodies.push(rb);
    }

    sim.update(0.0);
    let contacts = sim.get_contacts();
    assert_eq!(contacts.len(), count - 1);
    for i in 0..(count - 1) {
        let a = bodies[i];
        let b = bodies[i + 1];
        assert!(contacts
            .iter()
            .any(|c| (c.a == a && c.b == b) || (c.a == b && c.b == a)));
    }
}
