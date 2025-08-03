use dashi::utils::Handle;
use meshi::physics::{ActorStatus, ForceApplyInfo, PhysicsSimulation, RigidBody, SimulationInfo};

#[test]
fn apply_force_with_invalid_handle_returns_err() {
    let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
    let invalid = Handle::<RigidBody>::default();
    let res = sim.apply_rigid_body_force(invalid, &ForceApplyInfo::default());
    assert!(res.is_err());
}

#[test]
fn set_transform_with_invalid_handle_returns_false() {
    let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
    let invalid = Handle::<RigidBody>::default();
    let status = ActorStatus::default();
    assert!(!sim.set_rigid_body_transform(invalid, &status));
}
