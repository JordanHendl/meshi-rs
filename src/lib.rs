mod object;
pub mod physics;
pub mod render;
mod utils;
use dashi::utils::Handle;
use glam::Mat4;
use object::{FFIMeshObjectInfo, MeshObject};
use physics::{ForceApplyInfo, PhysicsSimulation};
use render::{DirectionalLight, DirectionalLightInfo, RenderEngine, RenderEngineInfo};
use std::ffi::*;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use utils::timer::Timer;

#[repr(C)]
/// Information used to create a [`MeshiEngine`].
///
/// Both strings must be valid null terminated C strings.
pub struct MeshiEngineInfo {
    /// Name of the application using the engine.
    pub application_name: *const c_char,
    /// Directory containing the application resources.
    pub application_location: *const c_char,
    /// Whether to run without creating a window (0 = windowed, 1 = headless).
    pub headless: i32,
}

/// Primary engine instance returned by [`meshi_make_engine`].
///
/// This struct owns the rendering and physics systems and should be
/// destroyed with [`meshi_destroy_engine`] when no longer needed.
#[repr(C)]
pub struct MeshiEngine {
    name: String,
    render: RenderEngine,
    physics: Box<PhysicsSimulation>,
    frame_timer: Timer,
}

impl MeshiEngine {
    fn new(info: &MeshiEngineInfo) -> Box<MeshiEngine> {
        assert!(!info.application_name.is_null());
        assert!(!info.application_location.is_null());
        let appname = unsafe { CStr::from_ptr(info.application_name) }
            .to_str()
            .unwrap();
        let mut appdir = unsafe { CStr::from_ptr(info.application_location) }
            .to_str()
            .unwrap();

        if appdir.is_empty() {
            appdir = ".";
        }

        info!("--INITIALIZING ENGINE--");
        info!("Application Name: '{}'", appname);
        info!("Application Dir: '{}'", appdir);

        Box::new(MeshiEngine {
            render: RenderEngine::new(&RenderEngineInfo {
                application_path: appdir.to_string(),
                scene_info: None,
                headless: info.headless != 0,
            }),
            physics: Box::new(PhysicsSimulation::new(&Default::default())),
            frame_timer: Timer::new(),
            name: appname.to_string(),
        })
    }

    fn update(&mut self) -> f32 {
        self.frame_timer.stop();
        let dt = self.frame_timer.elapsed_micro_f32();
        self.frame_timer.start();
        self.render.update(dt);
        self.physics.update(dt);

        dt
    }
}

#[repr(C)]
pub struct Renderable {}

/// Create a new engine instance.
///
/// # Safety
/// `info` must be a valid pointer to a [`MeshiEngineInfo`] with valid C strings.
#[no_mangle]
pub extern "C" fn meshi_make_engine(info: *const MeshiEngineInfo) -> *mut MeshiEngine {
    // a builder for `FmtSubscriber`.
    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(Level::INFO)
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let mut e = MeshiEngine::new(unsafe { &*info });
    e.frame_timer.start();
    return Box::into_raw(e);
}

/// Convenience wrapper to create a headless engine without modifying
/// [`MeshiEngineInfo`].
///
/// # Safety
/// `application_name` and `application_location` must be valid C strings.
#[no_mangle]
pub extern "C" fn meshi_make_engine_headless(
    application_name: *const c_char,
    application_location: *const c_char,
) -> *mut MeshiEngine {
    let info = MeshiEngineInfo {
        application_name,
        application_location,
        headless: 1,
    };
    meshi_make_engine(&info)
}

/// Destroy an engine previously created with [`meshi_make_engine`].
///
/// # Safety
/// `engine` must point to a valid [`MeshiEngine`] returned from
/// [`meshi_make_engine`] and must not be used after calling this function.
#[no_mangle]
pub extern "C" fn meshi_destroy_engine(engine: *mut MeshiEngine) {
    if !engine.is_null() {
        unsafe { drop(Box::from_raw(engine)) };
    }
}
/// Register a callback to receive window events from the renderer.
///
/// # Safety
/// `engine` must be a valid pointer returned by [`meshi_make_engine`].
#[no_mangle]
pub extern "C" fn meshi_register_event_callback(
    engine: *mut MeshiEngine,
    user_data: *mut c_void,
    cb: extern "C" fn(*mut render::event::Event, *mut c_void),
) {
    unsafe { &mut *engine }.render.set_event_cb(cb, user_data);
}

/// Advance the simulation by one frame and render the result.
///
/// # Safety
/// The caller must pass a valid pointer returned by [`meshi_make_engine`].
/// Providing a null pointer causes this function to return `0.0` without
/// performing any update.
#[no_mangle]
pub extern "C" fn meshi_update(engine: *mut MeshiEngine) -> c_float {
    if engine.is_null() {
        return 0.0;
    }
    unsafe { &mut *engine }.update() as c_float
}

////////////////////////////////////////////
//////////////////GRAPHICS//////////////////
////////////////////////////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
/// Obtain a mutable pointer to the engine's renderer.
///
/// # Safety
/// `engine` must be a valid engine pointer.
#[no_mangle]
pub extern "C" fn meshi_get_graphics_system(engine: *mut MeshiEngine) -> *mut RenderEngine {
    unsafe { &mut (*engine).render }
}

/// Register a new renderable mesh object.
///
/// # Safety
/// `render` must be a valid pointer obtained from [`meshi_get_graphics_system`]
/// and `info` must point to a valid [`FFIMeshObjectInfo`].
#[no_mangle]
pub extern "C" fn meshi_gfx_create_renderable(
    render: *mut RenderEngine,
    info: *const FFIMeshObjectInfo,
) -> Handle<MeshObject> {
    unsafe { &mut *render }.register_mesh_object(unsafe { &*info })
}

#[no_mangle]
pub extern "C" fn meshi_gfx_create_cube(render: *mut RenderEngine) -> Handle<MeshObject> {
    unsafe { &mut *render }.create_cube()
}

#[no_mangle]
pub extern "C" fn meshi_gfx_create_sphere(render: *mut RenderEngine) -> Handle<MeshObject> {
    unsafe { &mut *render }.create_sphere()
}

#[no_mangle]
pub extern "C" fn meshi_gfx_create_triangle(render: *mut RenderEngine) -> Handle<MeshObject> {
    unsafe { &mut *render }.create_triangle()
}

/// Update the transformation matrix for a renderable object.
///
/// # Safety
/// `render` must be obtained from [`meshi_get_graphics_system`] and
/// `transform` must point to a valid [`Mat4`]. If either pointer is null this
/// function returns without modifying the renderable.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_renderable_transform(
    render: *mut RenderEngine,
    h: Handle<MeshObject>,
    transform: *const Mat4,
) {
    if render.is_null() || transform.is_null() {
        return;
    }
    unsafe { &mut *render }.set_mesh_object_transform(h, unsafe { &*transform });
}

/// Create a directional light for the scene.
///
/// # Safety
/// `render` must be valid and `info` must not be null.
#[no_mangle]
pub extern "C" fn meshi_gfx_create_directional_light(
    render: *mut RenderEngine,
    info: *const DirectionalLightInfo,
) -> Handle<DirectionalLight> {
    unsafe { &mut *render }.register_directional_light(unsafe { &*info })
}

/// Update the transform for a directional light.
///
/// # Safety
/// `render` and `transform` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_directional_light_transform(
    render: *mut RenderEngine,
    h: Handle<DirectionalLight>,
    transform: *const Mat4,
) {
    unsafe { &mut *render }.set_directional_light_transform(h, unsafe { &*transform });
}

/// Set the world-to-camera transform used for rendering.
///
/// # Safety
/// `render` and `transform` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_camera(render: *mut RenderEngine, transform: *const Mat4) {
    unsafe { &mut *render }.set_camera(unsafe { &*transform });
}

/// Set the projection matrix used for rendering.
///
/// # Safety
/// `render` and `transform` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_projection(render: *mut RenderEngine, transform: *const Mat4) {
    unsafe { &mut *render }.set_projection(unsafe { &*transform });
}

/// Enable or disable mouse capture for the renderer window.
///
/// # Safety
/// `render` must be a valid pointer.
#[no_mangle]
pub extern "C" fn meshi_gfx_capture_mouse(render: *mut RenderEngine, value: i32) {
    if value == 0 {
        unsafe { &mut *render }.set_capture_mouse(false);
    } else {
        unsafe { &mut *render }.set_capture_mouse(true);
    }
}

////////////////////////////////////////////
//////////////////PHYSICS///////////////////
////////////////////////////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
/// Access the internal physics simulation.
///
/// # Safety
/// `engine` must be a valid engine pointer.
#[no_mangle]
pub extern "C" fn meshi_get_physics_system(engine: *mut MeshiEngine) -> *mut PhysicsSimulation {
    unsafe { (*engine).physics.as_mut() as *mut PhysicsSimulation }
}

/// Create a new material in the physics system.
///
/// # Safety
/// `physics` and `info` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_physx_create_material(
    physics: *mut PhysicsSimulation,
    info: *const physics::MaterialInfo,
) -> Handle<physics::Material> {
    unsafe { &mut *physics }.create_material(unsafe { &*info })
}

/// Release a physics material handle.
#[no_mangle]
pub extern "C" fn meshi_physx_release_material(
    physics: *mut PhysicsSimulation,
    h: *const Handle<physics::Material>,
) {
    unsafe { &mut *physics }.release_material(unsafe { *h });
}

/// Create a rigid body instance.
#[no_mangle]
pub extern "C" fn meshi_physx_create_rigid_body(
    physics: *mut PhysicsSimulation,
    info: *const physics::RigidBodyInfo,
) -> Handle<physics::RigidBody> {
    unsafe { &mut *physics }.create_rigid_body(unsafe { &*info })
}

/// Destroy a rigid body and free its resources.
#[no_mangle]
pub extern "C" fn meshi_physx_release_rigid_body(
    physics: *mut PhysicsSimulation,
    h: *const Handle<physics::RigidBody>,
) {
    unsafe { &mut *physics }.release_rigid_body(unsafe { *h });
}

/// Apply a force to a rigid body.
#[no_mangle]
pub extern "C" fn meshi_physx_apply_force_to_rigid_body(
    physics: *mut PhysicsSimulation,
    h: *const Handle<physics::RigidBody>,
    info: *const ForceApplyInfo,
) {
    unsafe { &mut *physics }.apply_rigid_body_force(unsafe { *h }, unsafe { &*info });
}

/// Set the position and rotation of a rigid body.
///
/// # Safety
/// `physics`, `h`, and `info` must be valid, non-null pointers.
#[no_mangle]
pub extern "C" fn meshi_physx_set_rigid_body_transform(
    physics: *mut PhysicsSimulation,
    h: *const Handle<physics::RigidBody>,
    info: *const physics::ActorStatus,
) {
    unsafe { &mut *physics }.set_rigid_body_transform(unsafe { *h }, unsafe { &*info });
}

/// Retrieve the current position and rotation of a rigid body.
///
/// # Safety
/// `physics`, `h`, and `out_status` must all be valid pointers. The function
/// returns immediately and leaves `out_status` untouched if any pointer is
/// null.
#[no_mangle]
pub extern "C" fn meshi_physx_get_rigid_body_status(
    physics: *mut PhysicsSimulation,
    h: *const Handle<physics::RigidBody>,
    out_status: *mut physics::ActorStatus,
) {
    if physics.is_null() || h.is_null() || out_status.is_null() {
        return;
    }
    let status = unsafe { &*physics }.get_rigid_body_status(unsafe { *h });
    unsafe { *out_status = status };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::{ActorStatus, PhysicsSimulation, RigidBodyInfo, SimulationInfo};
    use glam::{Quat, Vec3};

    #[test]
    fn rigid_body_transform_roundtrip() {
        let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
        let rb = sim.create_rigid_body(&RigidBodyInfo::default());
        let transform = ActorStatus {
            position: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::IDENTITY,
        };

        meshi_physx_set_rigid_body_transform(&mut sim, &rb as *const _, &transform);
        let mut out = ActorStatus::default();
        meshi_physx_get_rigid_body_status(&mut sim, &rb as *const _, &mut out);
        assert_eq!(out.position, transform.position);
        assert_eq!(out.rotation, transform.rotation);
    }
}
