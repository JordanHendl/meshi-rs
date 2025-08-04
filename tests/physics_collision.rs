use glam::Vec3;
use meshi::physics::{
    CollisionShape, CollisionShapeType, PhysicsSimulation, RigidBodyInfo, SimulationInfo,
};

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

    sim.update(0.0).unwrap();
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

    sim.update(0.0).unwrap();
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

#[test]
fn boxes_generate_contact() {
    let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
    let box_shape = CollisionShape {
        shape_type: CollisionShapeType::Box,
        dimensions: Vec3::splat(1.0),
        radius: 0.0,
        half_height: 0.0,
    };
    let rb1 = sim.create_rigid_body(&RigidBodyInfo {
        initial_position: Vec3::ZERO,
        has_gravity: 0,
        collision_shape: box_shape,
        ..Default::default()
    });
    let rb2 = sim.create_rigid_body(&RigidBodyInfo {
        initial_position: Vec3::new(0.9, 0.0, 0.0),
        has_gravity: 0,
        collision_shape: box_shape,
        ..Default::default()
    });

    sim.update(0.0);
    let contacts = sim.get_contacts();
    assert!(contacts
        .iter()
        .any(|c| (c.a == rb1 && c.b == rb2) || (c.a == rb2 && c.b == rb1)));
}

#[test]
fn box_and_sphere_generate_contact() {
    let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
    let box_shape = CollisionShape {
        shape_type: CollisionShapeType::Box,
        dimensions: Vec3::splat(1.0),
        radius: 0.0,
        half_height: 0.0,
    };
    let box_rb = sim.create_rigid_body(&RigidBodyInfo {
        initial_position: Vec3::ZERO,
        has_gravity: 0,
        collision_shape: box_shape,
        ..Default::default()
    });
    let sphere_rb = sim.create_rigid_body(&RigidBodyInfo {
        initial_position: Vec3::new(1.4, 0.0, 0.0),
        has_gravity: 0,
        ..Default::default()
    });

    sim.update(0.0);
    let contacts = sim.get_contacts();
    assert!(contacts
        .iter()
        .any(|c| { (c.a == box_rb && c.b == sphere_rb) || (c.a == sphere_rb && c.b == box_rb) }));
}

#[test]
fn capsules_generate_contact() {
    let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
    let capsule_shape = CollisionShape {
        shape_type: CollisionShapeType::Capsule,
        radius: 0.5,
        half_height: 1.0,
        dimensions: Vec3::ZERO,
    };
    let rb1 = sim.create_rigid_body(&RigidBodyInfo {
        initial_position: Vec3::ZERO,
        has_gravity: 0,
        collision_shape: capsule_shape,
        ..Default::default()
    });
    let rb2 = sim.create_rigid_body(&RigidBodyInfo {
        initial_position: Vec3::new(0.5, 0.0, 0.0),
        has_gravity: 0,
        collision_shape: capsule_shape,
        ..Default::default()
    });

    sim.update(0.0).unwrap();
    let contacts = sim.get_contacts();
    assert!(contacts
        .iter()
        .any(|c| (c.a == rb1 && c.b == rb2) || (c.a == rb2 && c.b == rb1)));
}

#[test]
fn capsule_and_sphere_generate_contact() {
    let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
    let capsule_shape = CollisionShape {
        shape_type: CollisionShapeType::Capsule,
        radius: 0.5,
        half_height: 1.0,
        dimensions: Vec3::ZERO,
    };
    let capsule_rb = sim.create_rigid_body(&RigidBodyInfo {
        initial_position: Vec3::ZERO,
        has_gravity: 0,
        collision_shape: capsule_shape,
        ..Default::default()
    });
    let sphere_rb = sim.create_rigid_body(&RigidBodyInfo {
        initial_position: Vec3::new(1.3, 0.0, 0.0),
        has_gravity: 0,
        ..Default::default()
    });

    sim.update(0.0).unwrap();
    let contacts = sim.get_contacts();
    assert!(contacts.iter().any(|c| {
        (c.a == capsule_rb && c.b == sphere_rb) || (c.a == sphere_rb && c.b == capsule_rb)
    }));
}
