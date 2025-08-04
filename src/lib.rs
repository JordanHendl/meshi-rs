pub mod audio;
mod object;
pub mod physics;
pub mod render;
mod utils;
use audio::{AudioEngine, AudioEngineInfo, AudioSource, StreamingSource};
use dashi::utils::Handle;
use glam::{Mat4, Vec3};
use object::{FFIMeshObjectInfo, MeshObject};
use physics::{CollisionShape, CollisionShapeType, ContactInfo, ForceApplyInfo, PhysicsSimulation};
use render::{
    DirectionalLight, DirectionalLightInfo, RenderBackend, RenderEngine, RenderEngineInfo,
};
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
    audio: AudioEngine,
    frame_timer: Timer,
}

impl MeshiEngine {
    fn new(info: &MeshiEngineInfo) -> Option<Box<MeshiEngine>> {
        if info.application_name.is_null() || info.application_location.is_null() {
            return None;
        }
        let appname = unsafe { CStr::from_ptr(info.application_name) }
            .to_str()
            .ok()?;
        let mut appdir = unsafe { CStr::from_ptr(info.application_location) }
            .to_str()
            .ok()?;

        if appdir.is_empty() {
            appdir = ".";
        }

        info!("--INITIALIZING ENGINE--");
        info!("Application Name: '{}'", appname);
        info!("Application Dir: '{}'", appdir);

        Some(Box::new(MeshiEngine {
            render: RenderEngine::new(&RenderEngineInfo {
                application_path: appdir.to_string(),
                scene_info: None,
                headless: info.headless != 0,
                backend: RenderBackend::Canvas,
            })
            .expect("failed to initialize render engine"),
            physics: Box::new(PhysicsSimulation::new(&Default::default())),
            audio: AudioEngine::new(&AudioEngineInfo::default()),
            frame_timer: Timer::new(),
            name: appname.to_string(),
        }))
    }

    fn update(&mut self) -> f32 {
        self.frame_timer.stop();
        let dt = self.frame_timer.elapsed_duration();
        self.frame_timer.start();
        let dt_secs = dt.as_secs_f32();
        self.render.update(dt_secs);
        let _ = self.physics.update(dt_secs);
        self.audio.update(dt_secs);

        dt_secs
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

    if info.is_null() {
        return std::ptr::null_mut();
    }
    if let Some(mut e) = MeshiEngine::new(unsafe { &*info }) {
        e.frame_timer.start();
        Box::into_raw(e)
    } else {
        std::ptr::null_mut()
    }
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
        unsafe {
            // Take ownership and ensure the engine and all subsystems are fully dropped
            // (render, physics and audio) before returning.
            let _engine = Box::from_raw(engine);
            // `_engine` is dropped here when it goes out of scope.
        }
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
    if engine.is_null() {
        return;
    }
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
    if engine.is_null() {
        return std::ptr::null_mut();
    }
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
    if render.is_null() || info.is_null() {
        return Handle::default();
    }
    unsafe { &mut *render }.register_mesh_object(unsafe { &*info })
}

#[no_mangle]
pub extern "C" fn meshi_gfx_create_cube(render: *mut RenderEngine) -> Handle<MeshObject> {
    if render.is_null() {
        return Handle::default();
    }
    unsafe { &mut *render }.create_cube()
}

#[no_mangle]
pub extern "C" fn meshi_gfx_create_cube_ex(
    render: *mut RenderEngine,
    info: *const render::database::geometry_primitives::CubePrimitiveInfo,
) -> Handle<MeshObject> {
    if render.is_null() || info.is_null() {
        return Handle::default();
    }
    unsafe { &mut *render }.create_cube_ex(unsafe { &*info })
}

#[no_mangle]
pub extern "C" fn meshi_gfx_create_sphere(render: *mut RenderEngine) -> Handle<MeshObject> {
    if render.is_null() {
        return Handle::default();
    }
    unsafe { &mut *render }.create_sphere()
}

#[no_mangle]
pub extern "C" fn meshi_gfx_create_sphere_ex(
    render: *mut RenderEngine,
    info: *const render::database::geometry_primitives::SpherePrimitiveInfo,
) -> Handle<MeshObject> {
    if render.is_null() || info.is_null() {
        return Handle::default();
    }
    unsafe { &mut *render }.create_sphere_ex(unsafe { &*info })
}

#[no_mangle]
pub extern "C" fn meshi_gfx_create_triangle(render: *mut RenderEngine) -> Handle<MeshObject> {
    if render.is_null() {
        return Handle::default();
    }
    unsafe { &mut *render }.create_triangle()
}

#[no_mangle]
pub extern "C" fn meshi_gfx_release_mesh_object(
    render: *mut RenderEngine,
    h: *const Handle<MeshObject>,
) {
    if render.is_null() || h.is_null() {
        return;
    }
    unsafe { &mut *render }.release_mesh_object(unsafe { *h });
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
    if render.is_null() || info.is_null() {
        return Handle::default();
    }
    unsafe { &mut *render }.register_directional_light(unsafe { &*info })
}

#[no_mangle]
pub extern "C" fn meshi_gfx_release_directional_light(
    render: *mut RenderEngine,
    h: *const Handle<DirectionalLight>,
) {
    if render.is_null() || h.is_null() {
        return;
    }
    unsafe { &mut *render }.release_directional_light(unsafe { *h });
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
    if render.is_null() || transform.is_null() {
        return;
    }
    unsafe { &mut *render }.set_directional_light_transform(h, unsafe { &*transform });
}

/// Update the properties for a directional light.
///
/// # Safety
/// `render` and `info` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_directional_light_info(
    render: *mut RenderEngine,
    h: Handle<DirectionalLight>,
    info: *const DirectionalLightInfo,
) {
    if render.is_null() || info.is_null() {
        return;
    }
    unsafe { &mut *render }.set_directional_light_info(h, unsafe { &*info });
}

/// Set the world-to-camera transform used for rendering.
///
/// # Safety
/// `render` and `transform` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_camera(render: *mut RenderEngine, transform: *const Mat4) {
    if render.is_null() || transform.is_null() {
        return;
    }
    unsafe { &mut *render }.set_camera(unsafe { &*transform });
}

/// Set the projection matrix used for rendering.
///
/// # Safety
/// `render` and `transform` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_projection(render: *mut RenderEngine, transform: *const Mat4) {
    if render.is_null() || transform.is_null() {
        return;
    }
    unsafe { &mut *render }.set_projection(unsafe { &*transform });
}

/// Enable or disable mouse capture for the renderer window.
///
/// # Safety
/// `render` must be a valid pointer.
#[no_mangle]
pub extern "C" fn meshi_gfx_capture_mouse(render: *mut RenderEngine, value: i32) {
    if render.is_null() {
        return;
    }
    unsafe { &mut *render }.set_capture_mouse(value != 0);
}

////////////////////////////////////////////
///////////////////AUDIO////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
/// Obtain a mutable pointer to the engine's audio system.
///
/// # Safety
/// `engine` must be a valid engine pointer.
#[no_mangle]
pub extern "C" fn meshi_get_audio_system(engine: *mut MeshiEngine) -> *mut AudioEngine {
    if engine.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { &mut (*engine).audio }
}

/// Create an audio source from a file path.
#[no_mangle]
pub extern "C" fn meshi_audio_create_source(
    audio: *mut AudioEngine,
    path: *const c_char,
) -> Handle<AudioSource> {
    if audio.is_null() || path.is_null() {
        return Handle::default();
    }
    let p = unsafe { CStr::from_ptr(path) }.to_str().unwrap_or("");
    unsafe { &mut *audio }.create_source(p)
}

/// Destroy an audio source and free its resources.
#[no_mangle]
pub extern "C" fn meshi_audio_destroy_source(audio: *mut AudioEngine, h: Handle<AudioSource>) {
    if audio.is_null() {
        return;
    }
    unsafe { &mut *audio }.destroy_source(h);
}

/// Begin playback for an audio source.
#[no_mangle]
pub extern "C" fn meshi_audio_play(audio: *mut AudioEngine, h: Handle<AudioSource>) {
    if audio.is_null() {
        return;
    }
    unsafe { &mut *audio }.play(h);
}

/// Pause playback for an audio source.
#[no_mangle]
pub extern "C" fn meshi_audio_pause(audio: *mut AudioEngine, h: Handle<AudioSource>) {
    if audio.is_null() {
        return;
    }
    unsafe { &mut *audio }.pause(h);
}

/// Stop playback for an audio source.
#[no_mangle]
pub extern "C" fn meshi_audio_stop(audio: *mut AudioEngine, h: Handle<AudioSource>) {
    if audio.is_null() {
        return;
    }
    unsafe { &mut *audio }.stop(h);
}

/// Set whether an audio source loops when played.
#[no_mangle]
pub extern "C" fn meshi_audio_set_looping(
    audio: *mut AudioEngine,
    h: Handle<AudioSource>,
    looping: i32,
) {
    if audio.is_null() {
        return;
    }
    unsafe { &mut *audio }.set_looping(h, looping != 0);
}

/// Adjust the playback volume for an audio source.
#[no_mangle]
pub extern "C" fn meshi_audio_set_volume(
    audio: *mut AudioEngine,
    h: Handle<AudioSource>,
    volume: c_float,
) {
    if audio.is_null() {
        return;
    }
    unsafe { &mut *audio }.set_volume(h, volume as f32);
}

/// Adjust the playback pitch for an audio source.
#[no_mangle]
pub extern "C" fn meshi_audio_set_pitch(
    audio: *mut AudioEngine,
    h: Handle<AudioSource>,
    pitch: c_float,
) {
    if audio.is_null() {
        return;
    }
    unsafe { &mut *audio }.set_pitch(h, pitch as f32);
}

/// Create a streaming audio source from a file path.
#[no_mangle]
pub extern "C" fn meshi_audio_create_stream(
    audio: *mut AudioEngine,
    path: *const c_char,
) -> Handle<StreamingSource> {
    if audio.is_null() || path.is_null() {
        return Handle::default();
    }
    let p = unsafe { CStr::from_ptr(path) }.to_str().unwrap_or("");
    unsafe { &mut *audio }.create_stream(p)
}

/// Fill `out_samples` with data from the streaming source, returning the
/// number of bytes written.
#[no_mangle]
pub extern "C" fn meshi_audio_update_stream(
    audio: *mut AudioEngine,
    h: Handle<StreamingSource>,
    out_samples: *mut u8,
    max: usize,
) -> usize {
    if audio.is_null() || out_samples.is_null() {
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(out_samples, max) };
    unsafe { &mut *audio }.update_stream(h, slice)
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
    if engine.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { (*engine).physics.as_mut() as *mut PhysicsSimulation }
}

/// Set the gravitational acceleration for the physics simulation.
///
/// # Safety
/// `physics` must be a valid pointer. The gravity is expressed in meters per
/// second squared.
#[no_mangle]
pub extern "C" fn meshi_physx_set_gravity(physics: *mut PhysicsSimulation, gravity_mps: f32) {
    if physics.is_null() {
        return;
    }
    unsafe { &mut *physics }.set_gravity(gravity_mps);
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
    if physics.is_null() || info.is_null() {
        return Handle::default();
    }
    unsafe { &mut *physics }.create_material(unsafe { &*info })
}

/// Release a physics material handle.
#[no_mangle]
pub extern "C" fn meshi_physx_release_material(
    physics: *mut PhysicsSimulation,
    h: *const Handle<physics::Material>,
) {
    if physics.is_null() || h.is_null() {
        return;
    }
    unsafe { &mut *physics }.release_material(unsafe { *h });
}

/// Create a rigid body instance.
#[no_mangle]
pub extern "C" fn meshi_physx_create_rigid_body(
    physics: *mut PhysicsSimulation,
    info: *const physics::RigidBodyInfo,
) -> Handle<physics::RigidBody> {
    if physics.is_null() || info.is_null() {
        return Handle::default();
    }
    unsafe { &mut *physics }.create_rigid_body(unsafe { &*info })
}

/// Destroy a rigid body and free its resources.
#[no_mangle]
pub extern "C" fn meshi_physx_release_rigid_body(
    physics: *mut PhysicsSimulation,
    h: *const Handle<physics::RigidBody>,
) {
    if physics.is_null() || h.is_null() {
        return;
    }
    unsafe { &mut *physics }.release_rigid_body(unsafe { *h });
}

/// Apply a force to a rigid body.
#[no_mangle]
pub extern "C" fn meshi_physx_apply_force_to_rigid_body(
    physics: *mut PhysicsSimulation,
    h: *const Handle<physics::RigidBody>,
    info: *const ForceApplyInfo,
) {
    if physics.is_null() || h.is_null() || info.is_null() {
        return;
    }
    let _ = unsafe { &mut *physics }.apply_rigid_body_force(unsafe { *h }, unsafe { &*info });
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
) -> i32 {
    if physics.is_null() || h.is_null() || info.is_null() {
        return 0;
    }
    if unsafe { &mut *physics }.set_rigid_body_transform(unsafe { *h }, unsafe { &*info }) {
        1
    } else {
        0
    }
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
) -> i32 {
    if physics.is_null() || h.is_null() || out_status.is_null() {
        return 0;
    }
    if let Some(status) = unsafe { &*physics }.get_rigid_body_status(unsafe { *h }) {
        unsafe { *out_status = status };
        1
    } else {
        0
    }
}

/// Retrieve the current velocity of a rigid body.
///
/// If any pointer is invalid or the handle does not reference a valid body,
/// a zero vector is returned.
///
/// # Safety
/// `physics` and `h` must be valid, non-null pointers.
#[no_mangle]
pub extern "C" fn meshi_physx_get_rigid_body_velocity(
    physics: *mut PhysicsSimulation,
    h: *const Handle<physics::RigidBody>,
) -> Vec3 {
    if physics.is_null() || h.is_null() {
        return Vec3::ZERO;
    }

    unsafe { &*physics }
        .get_rigid_body_velocity(unsafe { *h })
        .unwrap_or(Vec3::ZERO)
}

/// Set the collision shape for a rigid body.
///
/// # Safety
/// `physics`, `h`, and `shape` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_physx_set_collision_shape(
    physics: *mut PhysicsSimulation,
    h: *const Handle<physics::RigidBody>,
    shape: *const CollisionShape,
) -> i32 {
    if physics.is_null() || h.is_null() || shape.is_null() {
        return 0;
    }
    if unsafe { &mut *physics }.set_rigid_body_collision_shape(unsafe { *h }, unsafe { &*shape }) {
        1
    } else {
        0
    }
}

/// Retrieve collision contacts from the last simulation update.
/// Returns the number of contacts written to `out_contacts`.
///
/// # Safety
/// `physics` and `out_contacts` must be valid pointers and `out_contacts`
/// must have space for at least `max` elements.
#[no_mangle]
pub extern "C" fn meshi_physx_get_contacts(
    physics: *mut PhysicsSimulation,
    out_contacts: *mut ContactInfo,
    max: usize,
) -> usize {
    if physics.is_null() || out_contacts.is_null() {
        return 0;
    }
    let contacts = unsafe { &*physics }.get_contacts();
    let count = contacts.len().min(max);
    unsafe {
        std::ptr::copy_nonoverlapping(contacts.as_ptr(), out_contacts, count);
    }
    count
}

#[no_mangle]
pub extern "C" fn meshi_physx_collision_shape_sphere(radius: f32) -> CollisionShape {
    CollisionShape {
        dimensions: Vec3::ZERO,
        radius,
        shape_type: CollisionShapeType::Sphere,
    }
}

#[no_mangle]
pub extern "C" fn meshi_physx_collision_shape_box(dimensions: Vec3) -> CollisionShape {
    CollisionShape {
        dimensions,
        radius: 0.0,
        shape_type: CollisionShapeType::Box,
    }
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

        assert_eq!(
            meshi_physx_set_rigid_body_transform(&mut sim, &rb as *const _, &transform),
            1
        );
        let mut out = ActorStatus::default();
        assert_eq!(
            meshi_physx_get_rigid_body_status(&mut sim, &rb as *const _, &mut out),
            1
        );
        assert_eq!(out.position, transform.position);
        assert_eq!(out.rotation, transform.rotation);
    }
}
