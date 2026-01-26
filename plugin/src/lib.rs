use glam::{Mat4, Vec3};
pub use meshi_audio::AudioEngine;
use meshi_audio::{
    AudioEngineInfo, AudioSource, Bus, FinishedCallback, PlaybackState, StreamingSource,
};
pub use meshi_ffi_structs::*;
pub use meshi_graphics::RenderEngine;
use meshi_graphics::{Camera, Light, RenderEngineInfo, RenderObject, RenderObjectInfo};
use meshi_physics::SimulationInfo;
pub use meshi_physics::PhysicsSimulation;
use meshi_physics::{CollisionShape, CollisionShapeType, ContactInfo, ForceApplyInfo};
use meshi_utils::timer::Timer;
use noren::{meta::DeviceModel, DBInfo};
use resource_pool::Handle;
use std::ffi::*;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

macro_rules! return_if_null {
    ($ret:expr, $($ptr:expr),+ $(,)?) => {
        if $( $ptr.is_null() )||* {
            return $ret;
        }
    };
}

pub const MESHI_PLUGIN_ABI_VERSION: u32 = 1;

#[repr(C)]
pub struct MeshiPluginApi {
    pub abi_version: u32,
    pub make_engine: extern "C" fn(*const MeshiEngineInfo) -> *mut MeshiEngine,
    pub make_engine_headless: extern "C" fn(*const c_char, *const c_char) -> *mut MeshiEngine,
    pub destroy_engine: extern "C" fn(*mut MeshiEngine),
    pub register_event_callback:
        extern "C" fn(*mut MeshiEngine, *mut c_void, extern "C" fn(*mut event::Event, *mut c_void)),
    pub update: extern "C" fn(*mut MeshiEngine) -> c_float,
    pub get_graphics_system: extern "C" fn(*mut MeshiEngine) -> *mut MeshiEngine,
    pub get_audio_system: extern "C" fn(*mut MeshiEngine) -> *mut MeshiEngine,
    pub get_physics_system: extern "C" fn(*mut MeshiEngine) -> *mut MeshiEngine,
    pub gfx_create_mesh_object:
        extern "C" fn(*mut MeshiEngine, *const MeshObjectInfo) -> Handle<RenderObject>,
    pub gfx_release_render_object: extern "C" fn(*mut MeshiEngine, *const Handle<RenderObject>),
    pub gfx_set_transform: extern "C" fn(*mut MeshiEngine, Handle<RenderObject>, *const Mat4),
    pub gfx_create_light: extern "C" fn(*mut MeshiEngine, *const LightInfo) -> Handle<Light>,
    pub gfx_release_light: extern "C" fn(*mut MeshiEngine, *const Handle<Light>),
    pub gfx_set_light_transform: extern "C" fn(*mut MeshiEngine, Handle<Light>, *const Mat4),
    pub gfx_set_light_info: extern "C" fn(*mut MeshiEngine, Handle<Light>, *const LightInfo),
    pub gfx_set_camera_transform: extern "C" fn(*mut MeshiEngine, *const Mat4),
    pub gfx_register_camera: extern "C" fn(*mut MeshiEngine, *const Mat4) -> Handle<Camera>,
    pub gfx_set_camera_projection: extern "C" fn(*mut MeshiEngine, *const Mat4),
    pub gfx_capture_mouse: extern "C" fn(*mut MeshiEngine, i32),
    pub audio_create_source:
        extern "C" fn(*mut MeshiEngine, *const c_char) -> Handle<AudioSource>,
    pub audio_destroy_source: extern "C" fn(*mut MeshiEngine, Handle<AudioSource>),
    pub audio_play: extern "C" fn(*mut MeshiEngine, Handle<AudioSource>),
    pub audio_pause: extern "C" fn(*mut MeshiEngine, Handle<AudioSource>),
    pub audio_stop: extern "C" fn(*mut MeshiEngine, Handle<AudioSource>),
    pub audio_get_state: extern "C" fn(*mut MeshiEngine, Handle<AudioSource>) -> PlaybackState,
    pub audio_set_looping: extern "C" fn(*mut MeshiEngine, Handle<AudioSource>, i32),
    pub audio_set_volume: extern "C" fn(*mut MeshiEngine, Handle<AudioSource>, c_float),
    pub audio_set_pitch: extern "C" fn(*mut MeshiEngine, Handle<AudioSource>, c_float),
    pub audio_set_source_transform:
        extern "C" fn(*mut MeshiEngine, Handle<AudioSource>, *const Mat4, Vec3),
    pub audio_set_listener_transform: extern "C" fn(*mut MeshiEngine, *const Mat4, Vec3),
    pub audio_create_stream:
        extern "C" fn(*mut MeshiEngine, *const c_char) -> Handle<StreamingSource>,
    pub audio_update_stream:
        extern "C" fn(*mut MeshiEngine, Handle<StreamingSource>, *mut u8, usize) -> usize,
    pub audio_set_bus_volume: extern "C" fn(*mut MeshiEngine, Handle<Bus>, c_float),
    pub audio_register_finished_callback:
        extern "C" fn(*mut MeshiEngine, *mut c_void, FinishedCallback),
    pub physx_set_gravity: extern "C" fn(*mut MeshiEngine, f32),
    pub physx_create_material:
        extern "C" fn(*mut MeshiEngine, *const meshi_physics::MaterialInfo)
            -> Handle<meshi_physics::Material>,
    pub physx_release_material:
        extern "C" fn(*mut MeshiEngine, *const Handle<meshi_physics::Material>),
    pub physx_create_rigid_body:
        extern "C" fn(*mut MeshiEngine, *const meshi_physics::RigidBodyInfo)
            -> Handle<meshi_physics::RigidBody>,
    pub physx_release_rigid_body:
        extern "C" fn(*mut MeshiEngine, *const Handle<meshi_physics::RigidBody>),
    pub physx_apply_force_to_rigid_body:
        extern "C" fn(*mut MeshiEngine, *const Handle<meshi_physics::RigidBody>, *const ForceApplyInfo),
    pub physx_set_rigid_body_transform:
        extern "C" fn(*mut MeshiEngine, *const Handle<meshi_physics::RigidBody>, *const meshi_physics::ActorStatus) -> i32,
    pub physx_get_rigid_body_status:
        extern "C" fn(*mut MeshiEngine, *const Handle<meshi_physics::RigidBody>, *mut meshi_physics::ActorStatus) -> i32,
    pub physx_get_rigid_body_velocity:
        extern "C" fn(*mut MeshiEngine, *const Handle<meshi_physics::RigidBody>) -> Vec3,
    pub physx_set_collision_shape:
        extern "C" fn(*mut MeshiEngine, *const Handle<meshi_physics::RigidBody>, *const CollisionShape) -> i32,
    pub physx_get_contacts:
        extern "C" fn(*mut MeshiEngine, *mut ContactInfo, usize) -> usize,
    pub physx_collision_shape_sphere: extern "C" fn(f32) -> CollisionShape,
    pub physx_collision_shape_box: extern "C" fn(Vec3) -> CollisionShape,
    pub physx_collision_shape_capsule: extern "C" fn(f32, f32) -> CollisionShape,
}

pub static MESHI_PLUGIN_API: MeshiPluginApi = MeshiPluginApi {
    abi_version: MESHI_PLUGIN_ABI_VERSION,
    make_engine: meshi_make_engine,
    make_engine_headless: meshi_make_engine_headless,
    destroy_engine: meshi_destroy_engine,
    register_event_callback: meshi_register_event_callback,
    update: meshi_update,
    get_graphics_system: meshi_get_graphics_system,
    get_audio_system: meshi_get_audio_system,
    get_physics_system: meshi_get_physics_system,
    gfx_create_mesh_object: meshi_gfx_create_mesh_object,
    gfx_release_render_object: meshi_gfx_release_render_object,
    gfx_set_transform: meshi_gfx_set_transform,
    gfx_create_light: meshi_gfx_create_light,
    gfx_release_light: meshi_gfx_release_light,
    gfx_set_light_transform: meshi_gfx_set_light_transform,
    gfx_set_light_info: meshi_gfx_set_light_info,
    gfx_set_camera_transform: meshi_gfx_set_camera_transform,
    gfx_register_camera: meshi_gfx_register_camera,
    gfx_set_camera_projection: meshi_gfx_set_camera_projection,
    gfx_capture_mouse: meshi_gfx_capture_mouse,
    audio_create_source: meshi_audio_create_source,
    audio_destroy_source: meshi_audio_destroy_source,
    audio_play: meshi_audio_play,
    audio_pause: meshi_audio_pause,
    audio_stop: meshi_audio_stop,
    audio_get_state: meshi_audio_get_state,
    audio_set_looping: meshi_audio_set_looping,
    audio_set_volume: meshi_audio_set_volume,
    audio_set_pitch: meshi_audio_set_pitch,
    audio_set_source_transform: meshi_audio_set_source_transform,
    audio_set_listener_transform: meshi_audio_set_listener_transform,
    audio_create_stream: meshi_audio_create_stream,
    audio_update_stream: meshi_audio_update_stream,
    audio_set_bus_volume: meshi_audio_set_bus_volume,
    audio_register_finished_callback: meshi_audio_register_finished_callback,
    physx_set_gravity: meshi_physx_set_gravity,
    physx_create_material: meshi_physx_create_material,
    physx_release_material: meshi_physx_release_material,
    physx_create_rigid_body: meshi_physx_create_rigid_body,
    physx_release_rigid_body: meshi_physx_release_rigid_body,
    physx_apply_force_to_rigid_body: meshi_physx_apply_force_to_rigid_body,
    physx_set_rigid_body_transform: meshi_physx_set_rigid_body_transform,
    physx_get_rigid_body_status: meshi_physx_get_rigid_body_status,
    physx_get_rigid_body_velocity: meshi_physx_get_rigid_body_velocity,
    physx_set_collision_shape: meshi_physx_set_collision_shape,
    physx_get_contacts: meshi_physx_get_contacts,
    physx_collision_shape_sphere: meshi_physx_collision_shape_sphere,
    physx_collision_shape_box: meshi_physx_collision_shape_box,
    physx_collision_shape_capsule: meshi_physx_collision_shape_capsule,
};

#[no_mangle]
pub extern "C" fn meshi_plugin_get_api() -> *const MeshiPluginApi {
    &MESHI_PLUGIN_API as *const MeshiPluginApi
}

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
    /// Optional extent to override the default canvas size.
    pub canvas_extent: *const u32,
    /// Enable debug overlays or tooling for engine subsystems (0 = disabled, 1 = enabled).
    pub debug_mode: i32,
}

/// Primary engine instance returned by [`meshi_make_engine`].
///
/// This struct owns the rendering and physics systems and should be
/// destroyed with [`meshi_destroy_engine`] when no longer needed.
#[repr(C)]
pub struct MeshiEngine {
    name: String,
    render: Box<RenderEngine>,
    physics: Box<PhysicsSimulation>,
    database: Box<noren::DB>,
    audio: AudioEngine,
    frame_timer: Timer,
    primary_camera: Option<Handle<Camera>>,
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
        info!("Headless Mode: '{}'", info.headless != 0);
        let mut render = Box::new(
            RenderEngine::new(&RenderEngineInfo {
                headless: info.headless != 0,
                canvas_extent: if info.canvas_extent.is_null() {
                    None
                } else {
                    Some(unsafe { [*info.canvas_extent, *info.canvas_extent.add(1)] })
                },
                skybox_cubemap_entry: Some(noren::defaults::DEFAULT_CUBEMAP_ENTRY.to_string()),
                debug_mode: info.debug_mode != 0,
                ..Default::default()
            })
            .expect("failed to initialize render engine"),
        );

        let mut database = Box::new(
            noren::DB::new(&DBInfo {
                base_dir: &appdir,
                layout_file: None,
                pooled_geometry_uploads: false,
            })
            .expect("failed to initialize database!"),
        );

        render.initialize_database(database.as_mut());
        let mut audio = AudioEngine::new(&AudioEngineInfo {
            debug_mode: info.debug_mode != 0,
            ..Default::default()
        });
        audio.initialize_database(database.as_mut());
        Some(Box::new(MeshiEngine {
            database,
            render,
            physics: Box::new(PhysicsSimulation::new(&SimulationInfo {
                debug_mode: info.debug_mode != 0,
                ..Default::default()
            })),
            audio,
            frame_timer: Timer::new(),
            name: appname.to_string(),
            primary_camera: None,
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

    fn shut_down(mut self) {
        self.render.shut_down();
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
        canvas_extent: std::ptr::null(),
        debug_mode: 0,
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
            let engine = Box::from_raw(engine);
            engine.shut_down();
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
    cb: extern "C" fn(*mut event::Event, *mut c_void),
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
pub extern "C" fn meshi_get_graphics_system(engine: *mut MeshiEngine) -> *mut MeshiEngine {
    if engine.is_null() {
        return std::ptr::null_mut();
    }
    return engine;
}

/// Register a new renderable mesh object.
///
/// # Safety
/// `render` must be a valid pointer obtained from [`meshi_get_graphics_system`]
/// and `info` must point to a valid [`FFIMeshObjectInfo`].
#[no_mangle]
pub extern "C" fn meshi_gfx_create_mesh_object(
    render: *mut MeshiEngine,
    info: *const MeshObjectInfo,
) -> Handle<RenderObject> {
    return_if_null!(Handle::default(), render, info);
    let engine: &mut MeshiEngine = unsafe { &mut (*render) };

    let info: &MeshObjectInfo = unsafe { &(*info) };
    let mesh = unsafe { CStr::from_ptr(info.mesh) }
        .to_str()
        .unwrap_or("mesh/default");

    let material = unsafe { CStr::from_ptr(info.material) }
        .to_str()
        .unwrap_or("material/default");

    let mesh = engine
        .database
        .fetch_gpu_mesh_with_material(mesh, material)
        .expect("Failed to load mesh");

    let model = DeviceModel {
        name: "".to_string(),
        meshes: vec![mesh],
        rig: Default::default(),
    };

    let h = engine
        .render
        .register_object(&RenderObjectInfo::Model(model))
        .expect("Unable to register object");
    meshi_gfx_set_transform(engine, h, &info.transform);

    h
}

#[no_mangle]
pub extern "C" fn meshi_gfx_release_render_object(
    render: *mut MeshiEngine,
    h: *const Handle<RenderObject>,
) {
    if render.is_null() || h.is_null() {
        return;
    }

    let engine: &mut MeshiEngine = unsafe { &mut (*render) };
    engine.render.release_object(unsafe { *h });
}

/// Update the transformation matrix for a renderable object.
///
/// # Safety
/// `render` must be obtained from [`meshi_get_graphics_system`] and
/// `transform` must point to a valid [`Mat4`]. If either pointer is null this
/// function returns without modifying the renderable.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_transform(
    render: *mut MeshiEngine,
    h: Handle<RenderObject>,
    transform: *const Mat4,
) {
    if render.is_null() || transform.is_null() {
        return;
    }
    if !h.valid() {
        info!(
            "Attempted to set transform for invalid mesh object handle (slot: {}, generation: {})",
            h.slot, h.generation
        );
        return;
    }

    let engine: &mut MeshiEngine = unsafe { &mut (*render) };
    engine
        .render
        .set_object_transform(h, unsafe { &*transform });
}

/// Create a directional light for the scene.
///
/// # Safety
/// `render` must be valid and `info` must not be null.
#[no_mangle]
pub extern "C" fn meshi_gfx_create_light(
    render: *mut MeshiEngine,
    info: *const LightInfo,
) -> Handle<Light> {
    if render.is_null() || info.is_null() {
        return Handle::default();
    }

    let engine: &mut MeshiEngine = unsafe { &mut (*render) };
    engine.render.register_light(unsafe { &*info })
}

#[no_mangle]
pub extern "C" fn meshi_gfx_release_light(render: *mut MeshiEngine, h: *const Handle<Light>) {
    if render.is_null() || h.is_null() {
        return;
    }

    let engine: &mut MeshiEngine = unsafe { &mut (*render) };
    engine.render.release_light(unsafe { *h });
}

/// Update the transform for a directional light.
///
/// # Safety
/// `render` and `transform` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_light_transform(
    render: *mut MeshiEngine,
    h: Handle<Light>,
    transform: *const Mat4,
) {
    if render.is_null() || transform.is_null() {
        return;
    }

    let engine: &mut MeshiEngine = unsafe { &mut (*render) };
    engine
        .render
        .set_light_transform(h, unsafe { &*transform });
}

/// Update the properties for a directional light.
///
/// # Safety
/// `render` and `info` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_light_info(
    render: *mut MeshiEngine,
    h: Handle<Light>,
    info: *const LightInfo,
) {
    if render.is_null() || info.is_null() {
        return;
    }

    let engine: &mut MeshiEngine = unsafe { &mut (*render) };
    engine.render.set_light_info(h, unsafe { &*info });
}

/// Set the world-to-camera transform used for rendering.
///
/// # Safety
/// `render` and `transform` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_camera_transform(render: *mut MeshiEngine, transform: *const Mat4) {
    if render.is_null() || transform.is_null() {
        return;
    }

    let engine: &mut MeshiEngine = unsafe { &mut (*render) };
    let Some(camera) = engine.primary_camera else {
        return;
    };
    engine
        .render
        .set_camera_transform(camera, unsafe { &*transform });
}

/// Set the projection matrix used for rendering.
///
/// # Safety
/// `render` and `transform` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_gfx_register_camera(
    render: *mut MeshiEngine,
    initial_transform: *const Mat4,
) -> Handle<Camera> {
    if render.is_null() || initial_transform.is_null() {
        return Handle::default();
    }

    let engine: &mut MeshiEngine = unsafe { &mut (*render) };
    let handle = engine
        .render
        .register_camera(unsafe { &*initial_transform });
    engine.primary_camera = Some(handle);
    handle
}

/// Set the projection matrix used for rendering.
///
/// # Safety
/// `render` and `transform` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_gfx_set_camera_projection(
    render: *mut MeshiEngine,
    transform: *const Mat4,
) {
    if render.is_null() || transform.is_null() {
        return;
    }

    let engine: &mut MeshiEngine = unsafe { &mut (*render) };
    let Some(camera) = engine.primary_camera else {
        return;
    };
    engine
        .render
        .set_camera_projection(camera, unsafe { &*transform });
}

/// Enable or disable mouse capture for the renderer window.
///
/// # Safety
/// `render` must be a valid pointer.
#[no_mangle]
pub extern "C" fn meshi_gfx_capture_mouse(render: *mut MeshiEngine, value: i32) {
    if render.is_null() {
        return;
    }

    let engine: &mut MeshiEngine = unsafe { &mut (*render) };
    engine.render.set_capture_mouse(value != 0);
}

////////////////////////////////////////////
///////////////////AUDIO////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
/// Obtain a mutable pointer to the engine for audio operations.
///
/// # Safety
/// `engine` must be a valid engine pointer.
#[no_mangle]
pub extern "C" fn meshi_get_audio_system(engine: *mut MeshiEngine) -> *mut MeshiEngine {
    if engine.is_null() {
        return std::ptr::null_mut();
    }
    engine
}

/// Create an audio source from a file path.
#[no_mangle]
pub extern "C" fn meshi_audio_create_source(
    engine: *mut MeshiEngine,
    path: *const c_char,
) -> Handle<AudioSource> {
    if engine.is_null() || path.is_null() {
        return Handle::default();
    }
    let p = unsafe { CStr::from_ptr(path) }.to_str().unwrap_or("");
    unsafe { &mut (*engine).audio }.create_source(p)
}

/// Destroy an audio source and free its resources.
#[no_mangle]
pub extern "C" fn meshi_audio_destroy_source(engine: *mut MeshiEngine, h: Handle<AudioSource>) {
    if engine.is_null() {
        return;
    }
    unsafe { &mut (*engine).audio }.destroy_source(h);
}

/// Begin playback for an audio source.
#[no_mangle]
pub extern "C" fn meshi_audio_play(engine: *mut MeshiEngine, h: Handle<AudioSource>) {
    if engine.is_null() {
        return;
    }
    unsafe { &mut (*engine).audio }.play(h);
}

/// Pause playback for an audio source.
#[no_mangle]
pub extern "C" fn meshi_audio_pause(engine: *mut MeshiEngine, h: Handle<AudioSource>) {
    if engine.is_null() {
        return;
    }
    unsafe { &mut (*engine).audio }.pause(h);
}

/// Stop playback for an audio source.
#[no_mangle]
pub extern "C" fn meshi_audio_stop(engine: *mut MeshiEngine, h: Handle<AudioSource>) {
    if engine.is_null() {
        return;
    }
    unsafe { &mut (*engine).audio }.stop(h);
}

/// Get the current playback state of an audio source.
#[no_mangle]
pub extern "C" fn meshi_audio_get_state(
    engine: *mut MeshiEngine,
    h: Handle<AudioSource>,
) -> PlaybackState {
    if engine.is_null() {
        return PlaybackState::Stopped;
    }
    unsafe { &mut (*engine).audio }
        .get_state(h)
        .unwrap_or(PlaybackState::Stopped)
}

/// Set whether an audio source loops when played.
#[no_mangle]
pub extern "C" fn meshi_audio_set_looping(
    engine: *mut MeshiEngine,
    h: Handle<AudioSource>,
    looping: i32,
) {
    if engine.is_null() {
        return;
    }
    unsafe { &mut (*engine).audio }.set_looping(h, looping != 0);
}

/// Adjust the playback volume for an audio source.
#[no_mangle]
pub extern "C" fn meshi_audio_set_volume(
    engine: *mut MeshiEngine,
    h: Handle<AudioSource>,
    volume: c_float,
) {
    if engine.is_null() {
        return;
    }
    unsafe { &mut (*engine).audio }.set_volume(h, volume as f32);
}

/// Adjust the playback pitch for an audio source.
#[no_mangle]
pub extern "C" fn meshi_audio_set_pitch(
    engine: *mut MeshiEngine,
    h: Handle<AudioSource>,
    pitch: c_float,
) {
    if engine.is_null() {
        return;
    }
    unsafe { &mut (*engine).audio }.set_pitch(h, pitch as f32);
}

/// Set the transform and velocity of an audio source.
///
/// # Safety
/// `engine` and `transform` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_audio_set_source_transform(
    engine: *mut MeshiEngine,
    h: Handle<AudioSource>,
    transform: *const Mat4,
    velocity: Vec3,
) {
    if engine.is_null() || transform.is_null() {
        return;
    }
    unsafe { &mut (*engine).audio }.set_source_transform(h, unsafe { &*transform }, velocity);
}

/// Set the listener transform and velocity for 3D audio calculations.
///
/// # Safety
/// `engine` and `transform` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_audio_set_listener_transform(
    engine: *mut MeshiEngine,
    transform: *const Mat4,
    velocity: Vec3,
) {
    if engine.is_null() || transform.is_null() {
        return;
    }
    unsafe { &mut (*engine).audio }.set_listener_transform(unsafe { &*transform }, velocity);
}

/// Create a streaming audio source from a file path.
#[no_mangle]
pub extern "C" fn meshi_audio_create_stream(
    engine: *mut MeshiEngine,
    path: *const c_char,
) -> Handle<StreamingSource> {
    if engine.is_null() || path.is_null() {
        return Handle::default();
    }
    let p = unsafe { CStr::from_ptr(path) }.to_str().unwrap_or("");
    unsafe { &mut (*engine).audio }.create_stream(p)
}

/// Fill `out_samples` with data from the streaming source, returning the
/// number of bytes written.
#[no_mangle]
pub extern "C" fn meshi_audio_update_stream(
    engine: *mut MeshiEngine,
    h: Handle<StreamingSource>,
    out_samples: *mut u8,
    max: usize,
) -> usize {
    if engine.is_null() || out_samples.is_null() {
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(out_samples, max) };
    unsafe { &mut (*engine).audio }.update_stream(h, slice)
}

/// Set the volume for an audio bus.
#[no_mangle]
pub extern "C" fn meshi_audio_set_bus_volume(
    engine: *mut MeshiEngine,
    h: Handle<Bus>,
    volume: c_float,
) {
    if engine.is_null() {
        return;
    }
    unsafe { &mut (*engine).audio }.set_bus_volume(h, volume as f32);
}

/// Register a callback invoked when a source finishes playback.
#[no_mangle]
pub extern "C" fn meshi_audio_register_finished_callback(
    engine: *mut MeshiEngine,
    user_data: *mut c_void,
    cb: FinishedCallback,
) {
    if engine.is_null() {
        return;
    }
    unsafe { &mut (*engine).audio }.register_finished_callback(cb, user_data);
}

////////////////////////////////////////////
//////////////////PHYSICS///////////////////
////////////////////////////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
////////////////////////////////////////////
/// Access the engine for physics operations.
///
/// # Safety
/// `engine` must be a valid engine pointer.
#[no_mangle]
pub extern "C" fn meshi_get_physics_system(engine: *mut MeshiEngine) -> *mut MeshiEngine {
    if engine.is_null() {
        return std::ptr::null_mut();
    }
    engine
}

/// Set the gravitational acceleration for the physics simulation.
///
/// # Safety
/// `engine` must be a valid pointer. The gravity is expressed in meters per
/// second squared.
#[no_mangle]
pub extern "C" fn meshi_physx_set_gravity(engine: *mut MeshiEngine, gravity_mps: f32) {
    if engine.is_null() {
        return;
    }
    unsafe { &mut (*engine).physics }.set_gravity(gravity_mps);
}

/// Create a new material in the physics system.
///
/// # Safety
/// `engine` and `info` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_physx_create_material(
    engine: *mut MeshiEngine,
    info: *const meshi_physics::MaterialInfo,
) -> Handle<meshi_physics::Material> {
    if engine.is_null() || info.is_null() {
        return Handle::default();
    }
    unsafe { &mut (*engine).physics }.create_material(unsafe { &*info })
}

/// Release a physics material handle.
#[no_mangle]
pub extern "C" fn meshi_physx_release_material(
    engine: *mut MeshiEngine,
    h: *const Handle<meshi_physics::Material>,
) {
    if engine.is_null() || h.is_null() {
        return;
    }
    unsafe { &mut (*engine).physics }.release_material(unsafe { *h });
}

/// Create a rigid body instance.
#[no_mangle]
pub extern "C" fn meshi_physx_create_rigid_body(
    engine: *mut MeshiEngine,
    info: *const meshi_physics::RigidBodyInfo,
) -> Handle<meshi_physics::RigidBody> {
    if engine.is_null() || info.is_null() {
        return Handle::default();
    }
    unsafe { &mut (*engine).physics }.create_rigid_body(unsafe { &*info })
}

/// Destroy a rigid body and free its resources.
#[no_mangle]
pub extern "C" fn meshi_physx_release_rigid_body(
    engine: *mut MeshiEngine,
    h: *const Handle<meshi_physics::RigidBody>,
) {
    if engine.is_null() || h.is_null() {
        return;
    }
    unsafe { &mut (*engine).physics }.release_rigid_body(unsafe { *h });
}

/// Apply a force to a rigid body.
#[no_mangle]
pub extern "C" fn meshi_physx_apply_force_to_rigid_body(
    engine: *mut MeshiEngine,
    h: *const Handle<meshi_physics::RigidBody>,
    info: *const ForceApplyInfo,
) {
    if engine.is_null() || h.is_null() || info.is_null() {
        return;
    }
    let _ =
        unsafe { &mut (*engine).physics }.apply_rigid_body_force(unsafe { *h }, unsafe { &*info });
}

/// Set the position and rotation of a rigid body.
///
/// # Safety
/// `engine`, `h`, and `info` must be valid, non-null pointers.
#[no_mangle]
pub extern "C" fn meshi_physx_set_rigid_body_transform(
    engine: *mut MeshiEngine,
    h: *const Handle<meshi_physics::RigidBody>,
    info: *const meshi_physics::ActorStatus,
) -> i32 {
    if engine.is_null() || h.is_null() || info.is_null() {
        return 0;
    }
    if unsafe { &mut (*engine).physics }
        .set_rigid_body_transform(unsafe { *h }, unsafe { &*info })
    {
        1
    } else {
        0
    }
}

/// Retrieve the current position and rotation of a rigid body.
///
/// # Safety
/// `engine`, `h`, and `out_status` must all be valid pointers. The function
/// returns immediately and leaves `out_status` untouched if any pointer is
/// null.
#[no_mangle]
pub extern "C" fn meshi_physx_get_rigid_body_status(
    engine: *mut MeshiEngine,
    h: *const Handle<meshi_physics::RigidBody>,
    out_status: *mut meshi_physics::ActorStatus,
) -> i32 {
    if engine.is_null() || h.is_null() || out_status.is_null() {
        return 0;
    }
    if let Some(status) = unsafe { &(*engine).physics }.get_rigid_body_status(unsafe { *h }) {
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
/// `engine` and `h` must be valid, non-null pointers.
#[no_mangle]
pub extern "C" fn meshi_physx_get_rigid_body_velocity(
    engine: *mut MeshiEngine,
    h: *const Handle<meshi_physics::RigidBody>,
) -> Vec3 {
    if engine.is_null() || h.is_null() {
        return Vec3::ZERO;
    }

    unsafe { &(*engine).physics }
        .get_rigid_body_velocity(unsafe { *h })
        .unwrap_or(Vec3::ZERO)
}

/// Set the collision shape for a rigid body.
///
/// # Safety
/// `engine`, `h`, and `shape` must be valid pointers.
#[no_mangle]
pub extern "C" fn meshi_physx_set_collision_shape(
    engine: *mut MeshiEngine,
    h: *const Handle<meshi_physics::RigidBody>,
    shape: *const CollisionShape,
) -> i32 {
    if engine.is_null() || h.is_null() || shape.is_null() {
        return 0;
    }
    if unsafe { &mut (*engine).physics }
        .set_rigid_body_collision_shape(unsafe { *h }, unsafe { &*shape })
    {
        1
    } else {
        0
    }
}

/// Retrieve collision contacts from the last simulation update.
/// Returns the number of contacts written to `out_contacts`.
///
/// # Safety
/// `engine` and `out_contacts` must be valid pointers and `out_contacts`
/// must have space for at least `max` elements.
#[no_mangle]
pub extern "C" fn meshi_physx_get_contacts(
    engine: *mut MeshiEngine,
    out_contacts: *mut ContactInfo,
    max: usize,
) -> usize {
    if engine.is_null() || out_contacts.is_null() {
        return 0;
    }
    let contacts = unsafe { &(*engine).physics }.get_contacts();
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
        half_height: 0.0,
        shape_type: CollisionShapeType::Sphere,
    }
}

#[no_mangle]
pub extern "C" fn meshi_physx_collision_shape_box(dimensions: Vec3) -> CollisionShape {
    CollisionShape {
        dimensions,
        radius: 0.0,
        half_height: 0.0,
        shape_type: CollisionShapeType::Box,
    }
}

#[no_mangle]
pub extern "C" fn meshi_physx_collision_shape_capsule(
    half_height: f32,
    radius: f32,
) -> CollisionShape {
    CollisionShape {
        dimensions: Vec3::ZERO,
        radius,
        half_height,
        shape_type: CollisionShapeType::Capsule,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec3};
    use meshi_physics::{ActorStatus, PhysicsSimulation, RigidBodyInfo, SimulationInfo};

    #[test]
    fn rigid_body_transform_roundtrip() {
        let mut sim = PhysicsSimulation::new(&SimulationInfo::default());
        let rb = sim.create_rigid_body(&RigidBodyInfo::default());
        let transform = ActorStatus {
            position: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::IDENTITY,
        };

        assert!(sim.set_rigid_body_transform(rb, &transform));
        let out = sim
            .get_rigid_body_status(rb)
            .expect("missing rigid body status");
        assert_eq!(out.position, transform.position);
        assert_eq!(out.rotation, transform.rotation);
    }
}
