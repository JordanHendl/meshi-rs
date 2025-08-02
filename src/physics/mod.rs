use dashi::utils::{Handle, Pool};
use glam::*;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EnvironmentInfo {
    gravity_mps: f32,
}

impl Default for EnvironmentInfo {
    fn default() -> Self {
        Self { gravity_mps: -9.8 }
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct SimulationInfo {
    pub environment: EnvironmentInfo,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MaterialInfo {
    pub dynamic_friction_m: f32,
}

impl Default for MaterialInfo {
    fn default() -> Self {
        Self {
            dynamic_friction_m: 5.0,
        }
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct ForceApplyInfo {
    amt: Vec3,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct RigidBodyInfo {
    pub material: Handle<Material>,
    pub initial_position: Vec3,
    pub initial_velocity: Vec3,
    pub initial_rotation: glam::Quat,
    pub has_gravity: u32,
}

#[repr(C)]
#[derive(Default)]
/// C representation keeps `Material` compatible with FFI. `MaterialInfo` is
/// already `repr(C)`, so this struct simply follows C layout without needing
/// additional packing.
pub struct Material {
    info: MaterialInfo,
}

impl From<&MaterialInfo> for Material {
    fn from(value: &MaterialInfo) -> Self {
        Self {
            info: value.clone(),
        }
    }
}
#[repr(C)]
#[derive(Default)]
/// `RigidBody` is shared across the FFI boundary. `Vec3` and `Quat` from
/// `glam` use 16-byte alignment, so the field order is arranged from largest
/// to smallest to avoid interior padding. The `material` handle precedes the
/// `has_gravity` flag so that all fields remain naturally aligned under
/// `repr(C)`.
pub struct RigidBody {
    position: Vec3,
    velocity: Vec3,
    rotation: Quat,
    material: Handle<Material>,
    has_gravity: u32,
}

impl RigidBody {
    pub fn apply_force(&mut self, force: Vec3) {
        self.velocity = self.velocity + force;
    }

    pub fn dampen_velocity(&mut self, mat: &Material, dt: &Vec3) {
        let dfc = mat.info.dynamic_friction_m;
        self.velocity = self.velocity - (vec3(dfc, dfc, dfc) * dt);
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct ActorStatus {
    pub position: Vec3,
    pub rotation: Quat,
}

impl From<&RigidBodyInfo> for RigidBody {
    fn from(value: &RigidBodyInfo) -> Self {
        RigidBody {
            position: value.initial_position,
            velocity: Default::default(),
            rotation: value.initial_rotation,
            material: value.material,
            has_gravity: value.has_gravity,
        }
    }
}
impl From<&RigidBody> for ActorStatus {
    fn from(value: &RigidBody) -> Self {
        Self {
            position: value.position,
            rotation: value.rotation,
        }
    }
}
pub struct PhysicsSimulation {
    info: SimulationInfo,
    materials: Pool<Material>,
    rigid_bodies: Pool<RigidBody>,
    default_material: Handle<Material>,
}

impl PhysicsSimulation {
    pub fn new(info: &SimulationInfo) -> Self {
        let mut s = Self {
            info: info.clone(),
            materials: Default::default(),
            rigid_bodies: Default::default(),
            default_material: Default::default(),
        };

        let default = s.materials.insert(Default::default()).unwrap();
        s.default_material = default;
        s
    }

    pub fn update(&mut self, dt: f32) {
        let dt = vec3(dt, dt, dt);
        self.rigid_bodies.for_each_occupied_mut(|r| {
            let mat = self.materials.get_ref(r.material).unwrap();
            if r.has_gravity == 1 {
                r.apply_force(vec3(0.0, self.info.environment.gravity_mps, 0.0) * dt);
            }

            let adj_velocity = r.velocity * dt;
            let pos = r.position;
            r.position = pos + adj_velocity;

            r.dampen_velocity(mat, &dt);
        });
    }

    pub fn create_material(&mut self, info: &MaterialInfo) -> Handle<Material> {
        self.materials.insert(info.into()).unwrap()
    }

    pub fn create_rigid_body(&mut self, info: &RigidBodyInfo) -> Handle<RigidBody> {
        let mut info = info.clone();
        if !info.material.valid() {
            info.material = self.default_material;
        }

        self.rigid_bodies.insert((&info).into()).unwrap()
    }

    pub fn release_material(&mut self, h: Handle<Material>) {
        self.materials.release(h);
    }

    pub fn release_rigid_body(&mut self, h: Handle<RigidBody>) {
        self.rigid_bodies.release(h);
    }

    pub fn apply_rigid_body_force(&mut self, h: Handle<RigidBody>, info: &ForceApplyInfo) {
        self.rigid_bodies
            .get_mut_ref(h)
            .unwrap()
            .apply_force(info.amt);
    }

    pub fn set_rigid_body_transform(&mut self, h: Handle<RigidBody>, info: &ActorStatus) {
        assert!(h.valid());
        if let Some(rb) = self.rigid_bodies.get_mut_ref(h) {
            rb.position = info.position;
            rb.rotation = info.rotation;
        }
    }

    pub fn get_rigid_body_status(&self, h: Handle<RigidBody>) -> ActorStatus {
        assert!(h.valid());
        self.rigid_bodies.get_ref(h).unwrap().into()
    }

    pub fn get_rigid_body_velocity(&self, h: Handle<RigidBody>) -> Vec3 {
        assert!(h.valid());
        self.rigid_bodies.get_ref(h).unwrap().velocity
    }
}
