use dashi::utils::{Handle, Pool};
use glam::*;
use std::collections::{HashMap, HashSet};

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
#[derive(Clone, Copy, Default)]
pub enum CollisionShapeType {
    #[default]
    Sphere = 0,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CollisionShape {
    pub shape_type: CollisionShapeType,
    pub radius: f32,
}

impl Default for CollisionShape {
    fn default() -> Self {
        Self {
            shape_type: CollisionShapeType::Sphere,
            radius: 1.0,
        }
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct RigidBodyInfo {
    pub material: Handle<Material>,
    pub initial_position: Vec3,
    pub initial_velocity: Vec3,
    pub initial_rotation: glam::Quat,
    pub has_gravity: u32,
    pub collision_shape: CollisionShape,
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
    shape: CollisionShape,
    material: Handle<Material>,
    has_gravity: u32,
    forces: Vec<Vec3>,
}

impl RigidBody {
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

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ContactInfo {
    pub a: Handle<RigidBody>,
    pub b: Handle<RigidBody>,
    pub normal: Vec3,
    pub penetration: f32,
}

impl From<&RigidBodyInfo> for RigidBody {
    fn from(value: &RigidBodyInfo) -> Self {
        RigidBody {
            position: value.initial_position,
            velocity: Default::default(),
            rotation: value.initial_rotation,
            shape: value.collision_shape,
            material: value.material,
            has_gravity: value.has_gravity,
            forces: Vec::new(),
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
    contacts: Vec<ContactInfo>,
    default_material: Handle<Material>,
}

impl PhysicsSimulation {
    pub fn new(info: &SimulationInfo) -> Self {
        let mut s = Self {
            info: info.clone(),
            materials: Default::default(),
            rigid_bodies: Default::default(),
            contacts: Vec::new(),
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
                r.forces
                    .push(vec3(0.0, self.info.environment.gravity_mps, 0.0) * dt);
            }

            let total_force = r.forces.iter().fold(Vec3::ZERO, |acc, f| acc + *f);
            r.velocity += total_force;
            r.forces.clear();

            let adj_velocity = r.velocity * dt;
            let pos = r.position;
            r.position = pos + adj_velocity;

            r.dampen_velocity(mat, &dt);
        });

        // Collision detection and resolution using a simple spatial grid
        self.contacts.clear();
        let mut handles = Vec::new();
        self.rigid_bodies
            .for_each_occupied_handle_mut(|h| handles.push(h));

        // Determine a cell size based on the largest radius
        let mut max_radius = 0.0f32;
        for &h in &handles {
            let rb = self.rigid_bodies.get_ref(h).unwrap();
            max_radius = max_radius.max(rb.shape.radius);
        }
        let cell_size = if max_radius > 0.0 { max_radius * 2.0 } else { 1.0 };

        // Populate the grid
        let mut grid: HashMap<(i32, i32, i32), Vec<Handle<RigidBody>>> = HashMap::new();
        for &h in &handles {
            let rb = self.rigid_bodies.get_ref(h).unwrap();
            let cell = (
                (rb.position.x / cell_size).floor() as i32,
                (rb.position.y / cell_size).floor() as i32,
                (rb.position.z / cell_size).floor() as i32,
            );
            grid.entry(cell).or_default().push(h);
        }

        // Helper closure to process a potential pair
        let mut process_pair = |ha: Handle<RigidBody>, hb: Handle<RigidBody>| {
            let a_pos = self.rigid_bodies.get_ref(ha).unwrap().position;
            let b_pos = self.rigid_bodies.get_ref(hb).unwrap().position;
            let a_vel = self.rigid_bodies.get_ref(ha).unwrap().velocity;
            let b_vel = self.rigid_bodies.get_ref(hb).unwrap().velocity;
            let a_rad = self.rigid_bodies.get_ref(ha).unwrap().shape.radius;
            let b_rad = self.rigid_bodies.get_ref(hb).unwrap().shape.radius;

            let delta = b_pos - a_pos;
            let dist = delta.length();
            let penetration = a_rad + b_rad - dist;
            if penetration > 0.0 {
                let normal = if dist > 0.0 { delta / dist } else { Vec3::Z };
                let correction = normal * (penetration / 2.0);

                let rel_vel = b_vel - a_vel;
                let vel_along_normal = rel_vel.dot(normal);
                let mut a_vel_new = a_vel;
                let mut b_vel_new = b_vel;
                if vel_along_normal < 0.0 {
                    let impulse = normal * vel_along_normal;
                    a_vel_new += impulse;
                    b_vel_new -= impulse;
                }

                {
                    let a_mut = self.rigid_bodies.get_mut_ref(ha).unwrap();
                    a_mut.position = a_pos - correction;
                    a_mut.velocity = a_vel_new;
                }
                {
                    let b_mut = self.rigid_bodies.get_mut_ref(hb).unwrap();
                    b_mut.position = b_pos + correction;
                    b_mut.velocity = b_vel_new;
                }

                self.contacts.push(ContactInfo {
                    a: ha,
                    b: hb,
                    normal,
                    penetration,
                });
            }
        };

        let mut checked: HashSet<(u16, u16)> = HashSet::new();
        let offsets = [-1, 0, 1];
        for (cell, bodies) in grid.iter() {
            for i in 0..bodies.len() {
                let ha = bodies[i];

                // Check other bodies in the same cell
                for j in (i + 1)..bodies.len() {
                    let hb = bodies[j];
                    let key = if ha.slot < hb.slot {
                        (ha.slot, hb.slot)
                    } else {
                        (hb.slot, ha.slot)
                    };
                    if checked.insert(key) {
                        process_pair(ha, hb);
                    }
                }

                // Check neighboring cells
                for dx in offsets.iter() {
                    for dy in offsets.iter() {
                        for dz in offsets.iter() {
                            if *dx == 0 && *dy == 0 && *dz == 0 {
                                continue;
                            }
                            let neighbor = (cell.0 + *dx, cell.1 + *dy, cell.2 + *dz);
                            if let Some(neighbors) = grid.get(&neighbor) {
                                for &hb in neighbors {
                                    let key = if ha.slot < hb.slot {
                                        (ha.slot, hb.slot)
                                    } else {
                                        (hb.slot, ha.slot)
                                    };
                                    if checked.insert(key) {
                                        process_pair(ha, hb);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
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
            .forces
            .push(info.amt);
    }

    pub fn set_rigid_body_transform(
        &mut self,
        h: Handle<RigidBody>,
        info: &ActorStatus,
    ) -> bool {
        if !h.valid() {
            return false;
        }
        if let Some(rb) = self.rigid_bodies.get_mut_ref(h) {
            rb.position = info.position;
            rb.rotation = info.rotation;
            true
        } else {
            false
        }
    }

    pub fn set_rigid_body_collision_shape(
        &mut self,
        h: Handle<RigidBody>,
        shape: &CollisionShape,
    ) -> bool {
        if !h.valid() {
            return false;
        }
        if let Some(rb) = self.rigid_bodies.get_mut_ref(h) {
            rb.shape = *shape;
            true
        } else {
            false
        }
    }

    pub fn get_rigid_body_status(&self, h: Handle<RigidBody>) -> Option<ActorStatus> {
        if !h.valid() {
            return None;
        }
        self.rigid_bodies.get_ref(h).map(|rb| rb.into())
    }

    pub fn get_rigid_body_velocity(&self, h: Handle<RigidBody>) -> Option<Vec3> {
        if !h.valid() {
            return None;
        }
        self.rigid_bodies.get_ref(h).map(|rb| rb.velocity)
    }

    pub fn get_contacts(&self) -> &[ContactInfo] {
        &self.contacts
    }
}
