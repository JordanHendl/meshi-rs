use resource_pool::{Handle, Pool};
use glam::*;
use std::collections::{HashMap, HashSet};

#[repr(C)]
#[derive(Clone, Copy)]
/// Environment parameters for the physics simulation.
///
/// Gravity defaults to Earth's gravity (`-9.8`). It can be customized by
/// constructing an [`EnvironmentInfo`] with a different value:
///
/// ```
/// use meshi_physics::{EnvironmentInfo, PhysicsSimulation, SimulationInfo};
///
/// let mut info = SimulationInfo::default();
/// info.environment = EnvironmentInfo::new(-3.7); // roughly moon gravity
/// let _sim = PhysicsSimulation::new(&info);
/// ```
pub struct EnvironmentInfo {
    /// Gravitational acceleration in meters per second squared.
    pub gravity_mps: f32,
}

impl EnvironmentInfo {
    /// Create a new [`EnvironmentInfo`] with the provided gravity value.
    pub fn new(gravity_mps: f32) -> Self {
        Self { gravity_mps }
    }
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
    pub static_friction_m: f32,
    pub restitution: f32,
}

impl Default for MaterialInfo {
    fn default() -> Self {
        Self {
            dynamic_friction_m: 5.0,
            static_friction_m: 5.0,
            restitution: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct ForceApplyInfo {
    pub amt: Vec3,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub enum CollisionShapeType {
    #[default]
    Sphere = 0,
    Box = 1,
    Capsule = 2,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CollisionShape {
    /// Full extents of the box shape. For spheres this value is ignored.
    pub dimensions: Vec3,
    /// Radius for sphere shapes. For boxes this value is ignored.
    pub radius: f32,
    /// Half height for capsule shapes. Ignored for other shapes.
    pub half_height: f32,
    pub shape_type: CollisionShapeType,
}

impl Default for CollisionShape {
    fn default() -> Self {
        Self {
            shape_type: CollisionShapeType::Sphere,
            radius: 1.0,
            half_height: 1.0,
            dimensions: Vec3::ONE,
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
        self.velocity -= vec3(dfc, dfc, dfc) * *dt;
        let threshold = mat.info.static_friction_m * dt.x;
        if self.velocity.length() < threshold {
            self.velocity = Vec3::ZERO;
        }
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

fn collide_sphere_box(
    sphere_pos: Vec3,
    radius: f32,
    box_pos: Vec3,
    box_half: Vec3,
) -> Option<(Vec3, f32)> {
    let diff = sphere_pos - box_pos;
    let closest = vec3(
        diff.x.clamp(-box_half.x, box_half.x),
        diff.y.clamp(-box_half.y, box_half.y),
        diff.z.clamp(-box_half.z, box_half.z),
    );
    let delta = diff - closest;
    let dist_sq = delta.length_squared();
    if dist_sq < radius * radius {
        let dist = dist_sq.sqrt();
        if dist > 0.0 {
            let normal = -(delta / dist);
            Some((normal, radius - dist))
        } else {
            let over_x = box_half.x - diff.x.abs();
            let over_y = box_half.y - diff.y.abs();
            let over_z = box_half.z - diff.z.abs();
            if over_x < over_y && over_x < over_z {
                let normal = vec3(if diff.x > 0.0 { -1.0 } else { 1.0 }, 0.0, 0.0);
                Some((normal, radius + over_x))
            } else if over_y < over_z {
                let normal = vec3(0.0, if diff.y > 0.0 { -1.0 } else { 1.0 }, 0.0);
                Some((normal, radius + over_y))
            } else {
                let normal = vec3(0.0, 0.0, if diff.z > 0.0 { -1.0 } else { 1.0 });
                Some((normal, radius + over_z))
            }
        }
    } else {
        None
    }
}

fn closest_point_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let t = (p - a).dot(ab) / ab.length_squared();
    a + ab * t.clamp(0.0, 1.0)
}

fn collide_capsule_sphere(
    cap_pos: Vec3,
    half_height: f32,
    radius: f32,
    sphere_pos: Vec3,
    sphere_radius: f32,
) -> Option<(Vec3, f32)> {
    let a = cap_pos + vec3(0.0, -half_height, 0.0);
    let b = cap_pos + vec3(0.0, half_height, 0.0);
    let closest = closest_point_on_segment(sphere_pos, a, b);
    let delta = sphere_pos - closest;
    let dist = delta.length();
    let penetration = radius + sphere_radius - dist;
    if penetration > 0.0 {
        let normal = if dist > 0.0 { delta / dist } else { Vec3::Y };
        Some((normal, penetration))
    } else {
        None
    }
}

fn collide_capsule_capsule(
    a_pos: Vec3,
    a_half: f32,
    a_radius: f32,
    b_pos: Vec3,
    b_half: f32,
    b_radius: f32,
) -> Option<(Vec3, f32)> {
    let a_min = a_pos.y - a_half;
    let a_max = a_pos.y + a_half;
    let b_min = b_pos.y - b_half;
    let b_max = b_pos.y + b_half;

    let (ya, yb) = if a_max < b_min {
        (a_max, b_min)
    } else if b_max < a_min {
        (a_min, b_max)
    } else {
        let y = (a_min.max(b_min) + a_max.min(b_max)) * 0.5;
        (y, y)
    };

    let pa = vec3(a_pos.x, ya, a_pos.z);
    let pb = vec3(b_pos.x, yb, b_pos.z);
    let delta = pb - pa;
    let dist = delta.length();
    let penetration = a_radius + b_radius - dist;
    if penetration > 0.0 {
        let normal = if dist > 0.0 { delta / dist } else { Vec3::Z };
        Some((normal, penetration))
    } else {
        None
    }
}

fn collide_capsule_box(
    cap_pos: Vec3,
    half_height: f32,
    radius: f32,
    box_pos: Vec3,
    box_half: Vec3,
) -> Option<(Vec3, f32)> {
    let seg_min = cap_pos.y - half_height;
    let seg_max = cap_pos.y + half_height;
    let box_min = box_pos - box_half;
    let box_max = box_pos + box_half;

    let closest_x = cap_pos.x.clamp(box_min.x, box_max.x);
    let closest_z = cap_pos.z.clamp(box_min.z, box_max.z);
    let closest_y = if seg_max < box_min.y {
        box_min.y
    } else if seg_min > box_max.y {
        box_max.y
    } else {
        cap_pos.y.clamp(box_min.y, box_max.y)
    };

    let axis_y = closest_y.clamp(seg_min, seg_max);
    let capsule_point = vec3(cap_pos.x, axis_y, cap_pos.z);
    let box_point = vec3(closest_x, closest_y, closest_z);
    let delta = capsule_point - box_point;
    let dist_sq = delta.length_squared();
    if dist_sq < radius * radius {
        let dist = dist_sq.sqrt();
        let normal = if dist > 0.0 { -(delta / dist) } else { Vec3::Y };
        Some((normal, radius - dist))
    } else {
        None
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsError {
    InvalidHandle,
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

    /// Set the global gravitational acceleration in meters per second squared.
    pub fn set_gravity(&mut self, gravity_mps: f32) {
        self.info.environment.gravity_mps = gravity_mps;
    }

    pub fn update(&mut self, dt: f32) -> Result<(), PhysicsError> {
        let dt_vec = vec3(dt, dt, dt);
        let mut had_invalid = false;

        self.rigid_bodies.for_each_occupied_mut(|r| {
            if let Some(mat) = self.materials.get_ref(r.material) {
                if r.has_gravity == 1 {
                    r.forces
                        .push(vec3(0.0, self.info.environment.gravity_mps, 0.0) * dt_vec);
                }

                let total_force = r.forces.iter().fold(Vec3::ZERO, |acc, f| acc + *f);
                r.velocity += total_force;
                r.forces.clear();

                let adj_velocity = r.velocity * dt_vec;
                let pos = r.position;
                r.position = pos + adj_velocity;

                r.dampen_velocity(mat, &dt_vec);
            } else {
                had_invalid = true;
            }
        });

        // Collision detection and resolution using a simple spatial grid
        self.contacts.clear();
        let mut handles = Vec::new();
        self.rigid_bodies
            .for_each_occupied_handle_mut(|h| handles.push(h));

        // Determine a cell size based on the largest radius
        let mut max_radius = 0.0f32;
        for &h in &handles {
            if let Some(rb) = self.rigid_bodies.get_ref(h) {
                let r = match rb.shape.shape_type {
                    CollisionShapeType::Sphere => rb.shape.radius,
                    CollisionShapeType::Box => rb.shape.dimensions.max_element() * 0.5,
                    CollisionShapeType::Capsule => rb.shape.radius + rb.shape.half_height,
                };
                max_radius = max_radius.max(r);
            } else {
                had_invalid = true;
            }
        }
        let cell_size = if max_radius > 0.0 {
            max_radius * 2.0
        } else {
            1.0
        };

        // Populate the grid
        let mut grid: HashMap<(i32, i32, i32), Vec<Handle<RigidBody>>> = HashMap::new();
        for &h in &handles {
            if let Some(rb) = self.rigid_bodies.get_ref(h) {
                let cell = (
                    (rb.position.x / cell_size).floor() as i32,
                    (rb.position.y / cell_size).floor() as i32,
                    (rb.position.z / cell_size).floor() as i32,
                );
                grid.entry(cell).or_default().push(h);
            } else {
                had_invalid = true;
            }
        }

        // Helper closure to process a potential pair
        let mut process_pair = |ha: Handle<RigidBody>, hb: Handle<RigidBody>| {
            let a_ref = self.rigid_bodies.get_ref(ha).unwrap();
            let b_ref = self.rigid_bodies.get_ref(hb).unwrap();
            let a_pos = a_ref.position;
            let b_pos = b_ref.position;
            let a_vel = a_ref.velocity;
            let b_vel = b_ref.velocity;
            let a_shape = a_ref.shape;
            let b_shape = b_ref.shape;
            let a_mat = self.materials.get_ref(a_ref.material).unwrap();
            let b_mat = self.materials.get_ref(b_ref.material).unwrap();

            let mut result: Option<(Vec3, f32)> = None;

            match (a_shape.shape_type, b_shape.shape_type) {
                (CollisionShapeType::Sphere, CollisionShapeType::Sphere) => {
                    let delta = b_pos - a_pos;
                    let dist = delta.length();
                    let penetration = a_shape.radius + b_shape.radius - dist;
                    if penetration > 0.0 {
                        let normal = if dist > 0.0 { delta / dist } else { Vec3::Z };
                        result = Some((normal, penetration));
                    }
                }
                (CollisionShapeType::Box, CollisionShapeType::Box) => {
                    let a_half = a_shape.dimensions * 0.5;
                    let b_half = b_shape.dimensions * 0.5;
                    let delta = b_pos - a_pos;
                    let overlap_x = a_half.x + b_half.x - delta.x.abs();
                    let overlap_y = a_half.y + b_half.y - delta.y.abs();
                    let overlap_z = a_half.z + b_half.z - delta.z.abs();
                    if overlap_x > 0.0 && overlap_y > 0.0 && overlap_z > 0.0 {
                        if overlap_x < overlap_y && overlap_x < overlap_z {
                            let normal = vec3(delta.x.signum(), 0.0, 0.0);
                            result = Some((normal, overlap_x));
                        } else if overlap_y < overlap_z {
                            let normal = vec3(0.0, delta.y.signum(), 0.0);
                            result = Some((normal, overlap_y));
                        } else {
                            let normal = vec3(0.0, 0.0, delta.z.signum());
                            result = Some((normal, overlap_z));
                        }
                    }
                }
                (CollisionShapeType::Sphere, CollisionShapeType::Box) => {
                    if let Some((normal, penetration)) =
                        collide_sphere_box(a_pos, a_shape.radius, b_pos, b_shape.dimensions * 0.5)
                    {
                        result = Some((normal, penetration));
                    }
                }
                (CollisionShapeType::Box, CollisionShapeType::Sphere) => {
                    if let Some((normal, penetration)) =
                        collide_sphere_box(b_pos, b_shape.radius, a_pos, a_shape.dimensions * 0.5)
                    {
                        result = Some((-normal, penetration));
                    }
                }
                (CollisionShapeType::Capsule, CollisionShapeType::Capsule) => {
                    if let Some((normal, penetration)) = collide_capsule_capsule(
                        a_pos,
                        a_shape.half_height,
                        a_shape.radius,
                        b_pos,
                        b_shape.half_height,
                        b_shape.radius,
                    ) {
                        result = Some((normal, penetration));
                    }
                }
                (CollisionShapeType::Capsule, CollisionShapeType::Sphere) => {
                    if let Some((normal, penetration)) = collide_capsule_sphere(
                        a_pos,
                        a_shape.half_height,
                        a_shape.radius,
                        b_pos,
                        b_shape.radius,
                    ) {
                        result = Some((normal, penetration));
                    }
                }
                (CollisionShapeType::Sphere, CollisionShapeType::Capsule) => {
                    if let Some((normal, penetration)) = collide_capsule_sphere(
                        b_pos,
                        b_shape.half_height,
                        b_shape.radius,
                        a_pos,
                        a_shape.radius,
                    ) {
                        result = Some((-normal, penetration));
                    }
                }
                (CollisionShapeType::Capsule, CollisionShapeType::Box) => {
                    if let Some((normal, penetration)) = collide_capsule_box(
                        a_pos,
                        a_shape.half_height,
                        a_shape.radius,
                        b_pos,
                        b_shape.dimensions * 0.5,
                    ) {
                        result = Some((normal, penetration));
                    }
                }
                (CollisionShapeType::Box, CollisionShapeType::Capsule) => {
                    if let Some((normal, penetration)) = collide_capsule_box(
                        b_pos,
                        b_shape.half_height,
                        b_shape.radius,
                        a_pos,
                        a_shape.dimensions * 0.5,
                    ) {
                        result = Some((-normal, penetration));
                    }
                }
            }

            if let Some((normal, penetration)) = result {
                let correction = normal * (penetration / 2.0);
                let rel_vel = b_vel - a_vel;
                let vel_along_normal = rel_vel.dot(normal);
                let mut a_vel_new = a_vel;
                let mut b_vel_new = b_vel;
                if vel_along_normal < 0.0 {
                    let restitution = (a_mat.info.restitution + b_mat.info.restitution) * 0.5;
                    let j = -vel_along_normal * (1.0 + restitution) * 0.5;
                    let impulse = normal * j;
                    a_vel_new -= impulse;
                    b_vel_new += impulse;
                }

                if let Some(a_mut) = self.rigid_bodies.get_mut_ref(ha) {
                    a_mut.position = a_pos - correction;
                    a_mut.velocity = a_vel_new;
                } else {
                    had_invalid = true;
                    return;
                }
                if let Some(b_mut) = self.rigid_bodies.get_mut_ref(hb) {
                    b_mut.position = b_pos + correction;
                    b_mut.velocity = b_vel_new;
                } else {
                    had_invalid = true;
                    return;
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

        if had_invalid {
            Err(PhysicsError::InvalidHandle)
        } else {
            Ok(())
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

    pub fn apply_rigid_body_force(
        &mut self,
        h: Handle<RigidBody>,
        info: &ForceApplyInfo,
    ) -> Result<(), PhysicsError> {
        if !h.valid() {
            return Err(PhysicsError::InvalidHandle);
        }
        if let Some(rb) = self.rigid_bodies.get_mut_ref(h) {
            rb.forces.push(info.amt);
            Ok(())
        } else {
            Err(PhysicsError::InvalidHandle)
        }
    }

    pub fn set_rigid_body_transform(&mut self, h: Handle<RigidBody>, info: &ActorStatus) -> bool {
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
