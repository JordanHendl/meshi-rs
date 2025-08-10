use std::{ffi::c_void, fmt};

use dashi::{
    utils::{Handle, Pool},
    *,
};
use database::{Database, Error as DatabaseError, MeshResource};
use glam::{Mat4, Vec3, Vec4};
use image::RgbaImage;
use tracing::{info, warn};

use crate::object::{
    Error as MeshObjectError, FFIMeshObjectInfo, MeshObject, MeshObjectInfo, MeshTarget,
};
use crate::render::database::geometry_primitives::{
    self, ConePrimitiveInfo, CubePrimitiveInfo, CylinderPrimitiveInfo, PlanePrimitiveInfo,
    SpherePrimitiveInfo,
};
use crate::streaming::StreamingManager;
mod canvas;
pub mod config;
pub mod database;
pub mod event;
mod graph;

#[derive(Debug)]
pub enum RenderError {
    DeviceSelection,
    ContextCreation,
    Database(DatabaseError),
    Gpu(dashi::GPUError),
    GraphConfig(std::io::Error),
    GraphParse(serde_json::Error),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::DeviceSelection => write!(f, "failed to select device"),
            RenderError::ContextCreation => write!(f, "failed to create GPU context"),
            RenderError::Database(err) => write!(f, "database error: {err}"),
            RenderError::Gpu(err) => write!(f, "gpu error: {err:?}"),
            RenderError::GraphConfig(err) => {
                write!(f, "failed to read graph config: {err}")
            }
            RenderError::GraphParse(err) => {
                write!(f, "failed to parse graph config: {err}")
            }
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RenderError::Database(err) => Some(err),
            RenderError::GraphConfig(err) => Some(err),
            RenderError::GraphParse(err) => Some(err),
            _ => None,
        }
    }
}

impl From<DatabaseError> for RenderError {
    fn from(value: DatabaseError) -> Self {
        RenderError::Database(value)
    }
}

impl From<dashi::GPUError> for RenderError {
    fn from(value: dashi::GPUError) -> Self {
        RenderError::Gpu(value)
    }
}

impl From<std::io::Error> for RenderError {
    fn from(value: std::io::Error) -> Self {
        RenderError::GraphConfig(value)
    }
}

impl From<serde_json::Error> for RenderError {
    fn from(value: serde_json::Error) -> Self {
        RenderError::GraphParse(value)
    }
}

pub struct SceneInfo<'a> {
    pub models: &'a [&'a str],
    pub images: &'a [&'a str],
}

#[derive(Default, Debug)]
pub struct SceneLoadErrors {
    pub models: Vec<String>,
    pub images: Vec<String>,
}

#[derive(Default, Clone, Copy)]
#[repr(C)]
pub struct DirectionalLightInfo {
    pub direction: Vec4,
    pub color: Vec4,
    pub intensity: f32,
}

#[derive(Default)]
pub struct DirectionalLight {
    pub transform: Mat4,
    pub info: DirectionalLightInfo,
}
pub struct CameraInfo<'a> {
    pub pass: &'a str,
    pub transform: Mat4,
    pub projection: Mat4,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub enum RenderBackend {
    #[default]
    Canvas,
    Graph,
}

#[derive(Default)]
pub struct RenderEngineInfo<'a> {
    pub application_path: String,
    pub scene_info: Option<SceneInfo<'a>>,
    pub headless: bool,
    pub backend: RenderBackend,
    pub canvas_extent: Option<[u32; 2]>,
}

struct EventCallbackInfo {
    event_cb: extern "C" fn(*mut event::Event, *mut c_void),
    user_data: *mut c_void,
}

#[allow(dead_code)]
pub struct RenderEngine {
    ctx: Option<Box<gpu::Context>>,
    event_loop: Option<winit::event_loop::EventLoop<()>>,
    database: Database,
    event_cb: Option<EventCallbackInfo>,
    mesh_objects: Pool<MeshObject>,
    directional_lights: Pool<DirectionalLight>,
    camera: Mat4,
    projection: Mat4,
    backend: Backend,
    scene_load_errors: SceneLoadErrors,
    streaming: Option<StreamingManager>,
}

enum Backend {
    Canvas(canvas::CanvasRenderer),
    Graph(graph::GraphRenderer),
}

#[allow(dead_code)]
impl RenderEngine {
    pub fn new(info: &RenderEngineInfo) -> Result<Self, RenderError> {
        let device_selector = DeviceSelector::new().map_err(|_| RenderError::DeviceSelection)?;
        let device = device_selector
            .select(DeviceFilter::default().add_required_type(DeviceType::Dedicated))
            .unwrap_or_default();

        info!("Initializing Render Engine with device {}", device);

        let cfg = config::RenderEngineConfig {
            scene_cfg_path: Some(format!("{}/koji.json", info.application_path)),
            database_path: Some(format!("{}/database", info.application_path)),
        };

        let backend = match info.backend {
            RenderBackend::Canvas => {
                info!("Using canvas backend [HEADLESS={}]", info.headless);
                Backend::Canvas(canvas::CanvasRenderer::new(
                    info.canvas_extent,
                    info.headless,
                ))
            }
            RenderBackend::Graph => {
                info!("Using graph backend [HEADLESS={}]", info.headless);
                Backend::Graph(graph::GraphRenderer::new(
                    cfg.scene_cfg_path.clone(),
                    info.headless,
                )?)
            }
        };

        // The GPU context that holds all the data.
        let mut ctx = if info.headless {
            info!("Initializing Headless Rendering Context");
            Box::new(
                gpu::Context::headless(&ContextInfo { device })
                    .map_err(|_| RenderError::ContextCreation)?,
            )
        } else {
            info!("Initializing Rendering Context");
            Box::new(
                gpu::Context::new(&ContextInfo { device })
                    .map_err(|_| RenderError::ContextCreation)?,
            )
        };

        let database = Database::new(cfg.database_path.as_ref().unwrap(), &mut ctx)?;

        let event_loop = if cfg!(test) || info.headless {
            None
        } else {
            Some(winit::event_loop::EventLoop::new())
        };

        let s = Self {
            ctx: Some(ctx),
            event_loop,
            database,
            event_cb: None,
            mesh_objects: Default::default(),
            directional_lights: Default::default(),
            camera: Mat4::IDENTITY,
            projection: Mat4::IDENTITY,
            backend,
            scene_load_errors: SceneLoadErrors::default(),
            streaming: None,
        };

        Ok(s)
    }

    pub fn register_directional_light(
        &mut self,
        info: &DirectionalLightInfo,
    ) -> Handle<DirectionalLight> {
        let light = DirectionalLight {
            transform: Mat4::IDENTITY,
            info: *info,
        };
        self.directional_lights.insert(light).unwrap()
    }

    pub fn set_directional_light_transform(
        &mut self,
        handle: Handle<DirectionalLight>,
        transform: &Mat4,
    ) {
        if let Some(light) = self.directional_lights.get_mut_ref(handle) {
            light.transform = *transform;
        }
    }

    pub fn set_directional_light_info(
        &mut self,
        handle: Handle<DirectionalLight>,
        info: &DirectionalLightInfo,
    ) {
        if let Some(light) = self.directional_lights.get_mut_ref(handle) {
            light.info = *info;
        }
    }

    pub fn release_directional_light(&mut self, handle: Handle<DirectionalLight>) {
        self.directional_lights.release(handle);
    }

    pub fn register_mesh_object(
        &mut self,
        info: &FFIMeshObjectInfo,
    ) -> Result<Handle<MeshObject>, MeshObjectError> {
        let info = MeshObjectInfo::try_from(info)?;
        let object = info.make_object(&mut self.database)?;
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        Ok(handle)
    }

    pub fn release_mesh_object(&mut self, handle: Handle<MeshObject>) {
        self.mesh_objects.release(handle);
    }

    pub fn register_mesh_with_renderer(&mut self, handle: Handle<MeshObject>) {
        if let Some(ctx) = self.ctx.as_mut() {
            if let Some(obj) = self.mesh_objects.get_mut_ref(handle) {
                let res = match &mut self.backend {
                    Backend::Canvas(r) => r.register_mesh(ctx, obj),
                    Backend::Graph(r) => r.register_mesh(ctx, obj),
                };
                if let Ok(idx) = res {
                    obj.renderer_handle = Some(idx);
                }
            }
        }
    }

    pub fn mesh_renderer_handle(&self, handle: Handle<MeshObject>) -> Option<usize> {
        self.mesh_objects
            .get_ref(handle)
            .and_then(|obj| obj.renderer_handle)
    }

    fn update_mesh_with_renderer(&mut self, handle: Handle<MeshObject>) {
        println!("3");
        if let Some(ctx) = self.ctx.as_mut() {
            if let Some(obj) = self.mesh_objects.get_ref(handle) {
                if let Some(idx) = obj.renderer_handle {
                    match &mut self.backend {
                        Backend::Canvas(r) => r.update_mesh(ctx, idx, obj),
                        Backend::Graph(r) => r.update_mesh(ctx, idx, obj),
                    }
                }
            }
        }
    }

    pub fn create_cube(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_CUBE",
            material: "MESHI_CUBE",
            transform: Mat4::IDENTITY,
        };
        let object = match info.make_object(&mut self.database) {
            Ok(obj) => obj,
            Err(e) => {
                warn!(
                    "failed to create mesh object '{}': {}; falling back to default material",
                    info.mesh, e
                );
                let mesh = self
                    .database
                    .fetch_mesh(info.mesh, true)
                    .expect("failed to fetch mesh");
                let material = self
                    .database
                    .fetch_material("DEFAULT", None)
                    .expect("failed to fetch default material");
                MeshObject {
                    targets: vec![MeshTarget {
                        mesh: mesh.clone(),
                        material,
                    }],
                    mesh,
                    transform: info.transform,
                    renderer_handle: None,
                }
            }
        };
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        handle
    }

    pub fn create_cube_ex(&mut self, info: &CubePrimitiveInfo) -> Handle<MeshObject> {
        let prim = geometry_primitives::make_cube(info);
        let mesh = MeshResource::from_primitive("CUBE", prim);
        let material = self
            .database
            .fetch_material("DEFAULT", None)
            .expect("failed to fetch default material");
        let target = MeshTarget {
            mesh: mesh.clone(),
            material,
        };
        let object = MeshObject {
            targets: vec![target],
            mesh,
            transform: Mat4::IDENTITY,
            renderer_handle: None,
        };
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        handle
    }

    pub fn create_sphere(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_SPHERE",
            material: "MESHI_SPHERE",
            transform: Mat4::IDENTITY,
        };
        let object = match info.make_object(&mut self.database) {
            Ok(obj) => obj,
            Err(e) => {
                warn!(
                    "failed to create mesh object '{}': {}; falling back to default material",
                    info.mesh, e
                );
                let mesh = self
                    .database
                    .fetch_mesh(info.mesh, true)
                    .expect("failed to fetch mesh");
                let material = self
                    .database
                    .fetch_material("DEFAULT", None)
                    .expect("failed to fetch default material");
                MeshObject {
                    targets: vec![MeshTarget {
                        mesh: mesh.clone(),
                        material,
                    }],
                    mesh,
                    transform: info.transform,
                    renderer_handle: None,
                }
            }
        };
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        handle
    }

    pub fn create_sphere_ex(&mut self, info: &SpherePrimitiveInfo) -> Handle<MeshObject> {
        let prim = geometry_primitives::make_sphere(info);
        let mesh = MeshResource::from_primitive("SPHERE", prim);
        let material = self
            .database
            .fetch_material("DEFAULT", None)
            .expect("failed to fetch default material");
        let target = MeshTarget {
            mesh: mesh.clone(),
            material,
        };
        let object = MeshObject {
            targets: vec![target],
            mesh,
            transform: Mat4::IDENTITY,
            renderer_handle: None,
        };
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        handle
    }

    pub fn create_cylinder(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_CYLINDER",
            material: "MESHI_CYLINDER",
            transform: Mat4::IDENTITY,
        };
        let mut object = info
            .make_object(&mut self.database)
            .expect("failed to create mesh object");
        object.renderer_handle = None;
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        handle
    }

    pub fn create_cylinder_ex(&mut self, info: &CylinderPrimitiveInfo) -> Handle<MeshObject> {
        let prim = geometry_primitives::make_cylinder(info);
        let mesh = MeshResource::from_primitive("CYLINDER", prim);
        let material = self
            .database
            .fetch_material("DEFAULT", None)
            .expect("failed to fetch default material");
        let target = MeshTarget {
            mesh: mesh.clone(),
            material,
        };
        let object = MeshObject {
            targets: vec![target],
            mesh,
            transform: Mat4::IDENTITY,
            renderer_handle: None,
        };
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        handle
    }

    pub fn create_plane(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_PLANE",
            material: "MESHI_PLANE",
            transform: Mat4::IDENTITY,
        };
        let mut object = info
            .make_object(&mut self.database)
            .expect("failed to create mesh object");
        object.renderer_handle = None;
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        handle
    }

    pub fn create_plane_ex(&mut self, info: &PlanePrimitiveInfo) -> Handle<MeshObject> {
        let prim = geometry_primitives::make_plane(info);
        let mesh = MeshResource::from_primitive("PLANE", prim);
        let material = self
            .database
            .fetch_material("DEFAULT", None)
            .expect("failed to fetch default material");
        let target = MeshTarget {
            mesh: mesh.clone(),
            material,
        };
        let object = MeshObject {
            targets: vec![target],
            mesh,
            transform: Mat4::IDENTITY,
            renderer_handle: None,
        };
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        handle
    }

    pub fn create_cone(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_CONE",
            material: "MESHI_CONE",
            transform: Mat4::IDENTITY,
        };
        let mut object = info
            .make_object(&mut self.database)
            .expect("failed to create mesh object");
        object.renderer_handle = None;
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        handle
    }

    pub fn create_cone_ex(&mut self, info: &ConePrimitiveInfo) -> Handle<MeshObject> {
        let prim = geometry_primitives::make_cone(info);
        let mesh = MeshResource::from_primitive("CONE", prim);
        let material = self
            .database
            .fetch_material("DEFAULT", None)
            .expect("failed to fetch default material");
        let target = MeshTarget {
            mesh: mesh.clone(),
            material,
        };
        let object = MeshObject {
            targets: vec![target],
            mesh,
            transform: Mat4::IDENTITY,
            renderer_handle: None,
        };
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        handle
    }

    pub fn create_triangle(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_TRIANGLE",
            material: "MESHI_TRIANGLE",
            transform: Mat4::IDENTITY,
        };
        let object = match info.make_object(&mut self.database) {
            Ok(obj) => obj,
            Err(e) => {
                warn!(
                    "failed to create mesh object '{}': {}; falling back to default material",
                    info.mesh, e
                );
                let mesh = self
                    .database
                    .fetch_mesh(info.mesh, true)
                    .expect("failed to fetch mesh");
                let material = self
                    .database
                    .fetch_material("DEFAULT", None)
                    .expect("failed to fetch default material");
                MeshObject {
                    targets: vec![MeshTarget {
                        mesh: mesh.clone(),
                        material,
                    }],
                    mesh,
                    transform: info.transform,
                    renderer_handle: None,
                }
            }
        };
        let handle = self.mesh_objects.insert(object).unwrap();
        self.register_mesh_with_renderer(handle);
        handle
    }

    pub fn set_mesh_object_transform(
        &mut self,
        handle: Handle<MeshObject>,
        transform: &glam::Mat4,
    ) {
        if !handle.valid() {
            info!(
                "Attempted to set transform for invalid mesh object handle (slot: {}, generation: {})",
                handle.slot,
                handle.generation
            );
            return;
        }
        println!("1");
        match self.mesh_objects.get_mut_ref(handle) {
            Some(obj) => {
                obj.transform = *transform;
                for target in &obj.targets {
                    info!("Submitting transform for mesh '{}'", target.mesh.name);
                }
            }
            None => {
                info!(
                    "Attempted to set transform for invalid mesh object handle (slot: {}, generation: {})",
                    handle.slot,
                    handle.generation
                );
            }
        }

        println!("2");
        // After transform update, refresh GPU mesh
        self.update_mesh_with_renderer(handle);
    }

    pub fn update(&mut self, _delta_time: f32) {
        use winit::event_loop::ControlFlow;
        use winit::platform::run_return::EventLoopExtRunReturn;

        if self.event_cb.is_some() {
            let cb = self.event_cb.as_mut().unwrap();
            let mut triggered = false;

            if let Some(event_loop) = &mut self.event_loop {
                event_loop.run_return(|event, _target, control_flow| {
                    *control_flow = ControlFlow::Exit;
                    if let Some(mut e) = event::from_winit_event(&event) {
                        triggered = true;
                        let c = cb.event_cb;
                        c(&mut e, cb.user_data);
                    }
                });
            }

            if !triggered {
                let mut synthetic: event::Event = unsafe { std::mem::zeroed() };
                let c = cb.event_cb;
                c(&mut synthetic, cb.user_data);
            }
        }

        if let Some(mut mgr) = self.streaming.take() {
            let player_pos = self.camera_position();
            let db_ptr = &mut self.database as *mut Database;
            unsafe {
                mgr.update(player_pos, &mut *db_ptr, self);
            }
            self.streaming = Some(mgr);
        }

        if let Some(ctx) = self.ctx.as_mut() {
            match &mut self.backend {
                Backend::Canvas(r) => {
                    if let Err(e) = r.render(ctx) {
                        warn!("render error: {}", e);
                    }
                }
                Backend::Graph(r) => {
                    if let Err(e) = r.render(ctx) {
                        warn!("render error: {}", e);
                    }
                }
            }
        }
    }

    pub fn render_to_image(&mut self, extent: [u32; 2]) -> Result<RgbaImage, RenderError> {
        let ctx = self.ctx.as_mut().ok_or(RenderError::ContextCreation)?;
        match &mut self.backend {
            Backend::Canvas(r) => r.render_to_image(ctx, extent),
            Backend::Graph(r) => r.render_to_image(ctx, extent),
        }
    }

    pub fn set_projection(&mut self, proj: &Mat4) {
        self.projection = *proj;
    }

    pub fn set_capture_mouse(&mut self, capture: bool) {
        let _ = capture; // window management handled by renderer
    }
    pub fn set_camera(&mut self, camera: &Mat4) {
        self.camera = *camera;
    }

    pub fn camera_position(&self) -> Vec3 {
        self.camera.w_axis.truncate()
    }

    pub fn set_streaming_manager(&mut self, mgr: StreamingManager) {
        self.streaming = Some(mgr);
    }
    pub fn set_event_cb(
        &mut self,
        event_cb: extern "C" fn(*mut event::Event, *mut c_void),
        user_data: *mut c_void,
    ) {
        self.event_cb = Some(EventCallbackInfo {
            event_cb,
            user_data,
        });
    }

    /// Load the resources referenced by `SceneInfo` into the renderer's
    /// database. Models and images that fail to load are logged and recorded
    /// so the caller can query them later, but they do not abort rendering.
    pub fn set_scene(&mut self, info: &SceneInfo) -> Result<(), database::Error> {
        self.scene_load_errors = SceneLoadErrors::default();

        for m in info.models {
            if let Err(e) = self.database.load_model(m) {
                warn!("Failed to load model {}: {}", m, e);
                self.scene_load_errors.models.push((*m).to_string());
            }
        }

        for i in info.images {
            if let Err(e) = self.database.load_image(i, None) {
                warn!("Failed to load image {}: {}", i, e);
                self.scene_load_errors.images.push((*i).to_string());
            }
        }

        if !self.scene_load_errors.models.is_empty() || !self.scene_load_errors.images.is_empty() {
            warn!(
                "Scene loaded with {} model and {} image errors",
                self.scene_load_errors.models.len(),
                self.scene_load_errors.images.len()
            );
        }

        Ok(())
    }

    /// Retrieve the list of resources that failed to load during the last
    /// `set_scene` call.
    pub fn scene_load_errors(&self) -> &SceneLoadErrors {
        &self.scene_load_errors
    }

    pub fn shut_down(mut self) {
        drop(self.backend);
        if let Some(ctx) = self.ctx.take() {
            ctx.destroy();
        }
    }
}


