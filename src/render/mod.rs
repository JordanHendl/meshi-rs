use std::{ffi::c_void, fmt};

use dashi::{
    utils::{Handle, Pool},
    *,
};
use database::{Database, Error as DatabaseError, MeshResource};
use glam::{Mat4, Vec4};
use tracing::{info, warn};

use crate::object::{
    Error as MeshObjectError, FFIMeshObjectInfo, MeshObject, MeshObjectInfo, MeshTarget,
};
use crate::render::database::geometry_primitives::{
    self, ConePrimitiveInfo, CubePrimitiveInfo, CylinderPrimitiveInfo, PlanePrimitiveInfo,
    SpherePrimitiveInfo,
};
mod canvas;
pub mod config;
pub mod database;
pub mod event;
mod graph;

#[derive(Debug)]
pub enum RenderError {
    DeviceSelection,
    ContextCreation,
    DisplayCreation,
    Database(DatabaseError),
    Gpu(dashi::GPUError),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::DeviceSelection => write!(f, "failed to select device"),
            RenderError::ContextCreation => write!(f, "failed to create GPU context"),
            RenderError::DisplayCreation => write!(f, "failed to create display"),
            RenderError::Database(err) => write!(f, "database error: {err}"),
            RenderError::Gpu(err) => write!(f, "gpu error: {err:?}"),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RenderError::Database(err) => Some(err),
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

pub struct SceneInfo<'a> {
    pub models: &'a [&'a str],
    pub images: &'a [&'a str],
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
}

struct EventCallbackInfo {
    event_cb: extern "C" fn(*mut event::Event, *mut c_void),
    user_data: *mut c_void,
}

#[allow(dead_code)]
pub struct RenderEngine {
    ctx: Option<Box<gpu::Context>>,
    display: Option<gpu::Display>,
    event_loop: Option<winit::event_loop::EventLoop<()>>,
    database: Database,
    event_cb: Option<EventCallbackInfo>,
    mesh_objects: Pool<MeshObject>,
    directional_lights: Pool<DirectionalLight>,
    camera: Mat4,
    projection: Mat4,
    backend: Backend,
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
                info!("Using canvas backend");
                Backend::Canvas(canvas::CanvasRenderer::new())
            }
            RenderBackend::Graph => {
                info!("Using graph backend");
                Backend::Graph(graph::GraphRenderer::new(cfg.scene_cfg_path.clone()))
            }
        };

        // The GPU context that holds all the data.
        let mut ctx = if info.headless {
            Box::new(
                gpu::Context::headless(&ContextInfo { device })
                    .map_err(|_| RenderError::ContextCreation)?,
            )
        } else {
            Box::new(
                gpu::Context::new(&ContextInfo { device })
                    .map_err(|_| RenderError::ContextCreation)?,
            )
        };

        let display = if info.headless {
            None
        } else {
            Some(
                ctx.make_display(&Default::default())
                    .map_err(|_| RenderError::DisplayCreation)?,
            )
        };

        let event_loop = if info.headless {
            None
        } else {
            Some(winit::event_loop::EventLoop::new())
        };
        //        let event_pump = ctx.get_sdl_ctx().event_pump().unwrap();
        //        let mut scene = Box::new(miso::Scene::new(
        //            &mut ctx,
        //            &miso::SceneInfo {
        //                cfg: cfg.scene_cfg_path,
        //            },
        //        ));

        let database = Database::new(cfg.database_path.as_ref().unwrap(), &mut ctx)?;

        //        let global_camera = scene.register_camera(&CameraInfo {
        //            pass: "ALL",
        //            transform: Default::default(),
        //            projection: Default::default(),
        //        });

        let s = Self {
            ctx: Some(ctx),
            display,
            event_loop,
            database,
            event_cb: None,
            mesh_objects: Default::default(),
            directional_lights: Default::default(),
            camera: Mat4::IDENTITY,
            projection: Mat4::IDENTITY,
            backend,
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
        Ok(self.mesh_objects.insert(object).unwrap())
    }

    pub fn release_mesh_object(&mut self, handle: Handle<MeshObject>) {
        self.mesh_objects.release(handle);
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
                    .fetch_mesh(info.mesh)
                    .expect("failed to fetch mesh");
                let material = self
                    .database
                    .fetch_material("DEFAULT")
                    .expect("failed to fetch default material");
                MeshObject {
                    targets: vec![MeshTarget {
                        mesh: mesh.clone(),
                        material,
                    }],
                    mesh,
                    transform: info.transform,
                }
            }
        };
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_cube_ex(&mut self, info: &CubePrimitiveInfo) -> Handle<MeshObject> {
        let ctx = self.ctx.as_mut().expect("render context not initialized");
        let mesh = geometry_primitives::make_cube(info, ctx).unwrap_or_else(|e| {
            warn!("failed to create cube primitive: {:?}", e);
            MeshResource::default()
        });
        let material = self
            .database
            .fetch_material("DEFAULT")
            .expect("failed to fetch default material");
        let target = MeshTarget {
            mesh: mesh.clone(),
            material,
        };
        let object = MeshObject {
            targets: vec![target],
            mesh,
            transform: Mat4::IDENTITY,
        };
        self.mesh_objects.insert(object).unwrap()
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
                    .fetch_mesh(info.mesh)
                    .expect("failed to fetch mesh");
                let material = self
                    .database
                    .fetch_material("DEFAULT")
                    .expect("failed to fetch default material");
                MeshObject {
                    targets: vec![MeshTarget {
                        mesh: mesh.clone(),
                        material,
                    }],
                    mesh,
                    transform: info.transform,
                }
            }
        };
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_sphere_ex(&mut self, info: &SpherePrimitiveInfo) -> Handle<MeshObject> {
        let ctx = self.ctx.as_mut().expect("render context not initialized");
        let mesh = geometry_primitives::make_sphere(info, ctx).unwrap_or_else(|e| {
            warn!("failed to create sphere primitive: {:?}", e);
            MeshResource::default()
        });
        let material = self
            .database
            .fetch_material("DEFAULT")
            .expect("failed to fetch default material");
        let target = MeshTarget {
            mesh: mesh.clone(),
            material,
        };
        let object = MeshObject {
            targets: vec![target],
            mesh,
            transform: Mat4::IDENTITY,
        };
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_cylinder(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_CYLINDER",
            material: "MESHI_CYLINDER",
            transform: Mat4::IDENTITY,
        };
        let object = info
            .make_object(&mut self.database)
            .expect("failed to create mesh object");
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_cylinder_ex(&mut self, info: &CylinderPrimitiveInfo) -> Handle<MeshObject> {
        let ctx = self.ctx.as_mut().expect("render context not initialized");
        let mesh = geometry_primitives::make_cylinder(info, ctx).unwrap_or_else(|e| {
            warn!("failed to create cylinder primitive: {:?}", e);
            MeshResource::default()
        });
        let material = self
            .database
            .fetch_material("DEFAULT")
            .expect("failed to fetch default material");
        let target = MeshTarget {
            mesh: mesh.clone(),
            material,
        };
        let object = MeshObject {
            targets: vec![target],
            mesh,
            transform: Mat4::IDENTITY,
        };
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_plane(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_PLANE",
            material: "MESHI_PLANE",
            transform: Mat4::IDENTITY,
        };
        let object = info
            .make_object(&mut self.database)
            .expect("failed to create mesh object");
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_plane_ex(&mut self, info: &PlanePrimitiveInfo) -> Handle<MeshObject> {
        let ctx = self.ctx.as_mut().expect("render context not initialized");
        let mesh = geometry_primitives::make_plane(info, ctx).unwrap_or_else(|e| {
            warn!("failed to create plane primitive: {:?}", e);
            MeshResource::default()
        });
        let material = self
            .database
            .fetch_material("DEFAULT")
            .expect("failed to fetch default material");
        let target = MeshTarget {
            mesh: mesh.clone(),
            material,
        };
        let object = MeshObject {
            targets: vec![target],
            mesh,
            transform: Mat4::IDENTITY,
        };
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_cone(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_CONE",
            material: "MESHI_CONE",
            transform: Mat4::IDENTITY,
        };
        let object = info
            .make_object(&mut self.database)
            .expect("failed to create mesh object");
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_cone_ex(&mut self, info: &ConePrimitiveInfo) -> Handle<MeshObject> {
        let ctx = self.ctx.as_mut().expect("render context not initialized");
        let mesh = geometry_primitives::make_cone(info, ctx).unwrap_or_else(|e| {
            warn!("failed to create cone primitive: {:?}", e);
            MeshResource::default()
        });
        let material = self
            .database
            .fetch_material("DEFAULT")
            .expect("failed to fetch default material");
        let target = MeshTarget {
            mesh: mesh.clone(),
            material,
        };
        let object = MeshObject {
            targets: vec![target],
            mesh,
            transform: Mat4::IDENTITY,
        };
        self.mesh_objects.insert(object).unwrap()
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
                    .fetch_mesh(info.mesh)
                    .expect("failed to fetch mesh");
                let material = self
                    .database
                    .fetch_material("DEFAULT")
                    .expect("failed to fetch default material");
                MeshObject {
                    targets: vec![MeshTarget {
                        mesh: mesh.clone(),
                        material,
                    }],
                    mesh,
                    transform: info.transform,
                }
            }
        };
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn set_mesh_object_transform(
        &mut self,
        handle: Handle<MeshObject>,
        transform: &glam::Mat4,
    ) {
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
    }

    pub fn update(&mut self, _delta_time: f32) {
        use winit::event_loop::ControlFlow;
        use winit::platform::run_return::EventLoopExtRunReturn;

        if self.event_cb.is_some() {
            let cb = self.event_cb.as_mut().unwrap();
            let mut triggered = false;

            if let Some(display) = &mut self.display {
                let event_loop = display.winit_event_loop();
                event_loop.run_return(|event, _target, control_flow| {
                    *control_flow = ControlFlow::Exit;
                    if let Some(mut e) = event::from_winit_event(&event) {
                        triggered = true;
                        let c = cb.event_cb;
                        c(&mut e, cb.user_data);
                    }
                });
            } else if let Some(event_loop) = &mut self.event_loop {
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

        if let (Some(ctx), Some(display)) = (self.ctx.as_mut(), self.display.as_mut()) {
            match &mut self.backend {
                Backend::Canvas(r) => {
                    if let Err(e) = r.render(ctx, display, &self.mesh_objects) {
                        warn!("render error: {}", e);
                    }
                }
                Backend::Graph(r) => {
                    if let Err(e) = r.render(ctx, display, &self.mesh_objects) {
                        warn!("render error: {}", e);
                    }
                }
            }
        }
    }

    pub fn set_projection(&mut self, proj: &Mat4) {
        self.projection = *proj;
    }

    pub fn set_capture_mouse(&mut self, capture: bool) {
        if let Some(display) = &self.display {
            let window = display.winit_window();
            if let Err(e) = window.set_cursor_grab(capture) {
                warn!("failed to set cursor grab: {:?}", e);
            }
            window.set_cursor_visible(!capture);
        }
    }
    pub fn set_camera(&mut self, camera: &Mat4) {
        self.camera = *camera;
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
    /// database. Models and images that fail to load will return an error so
    /// callers can react accordingly.
    pub fn set_scene(&mut self, info: &SceneInfo) -> Result<(), database::Error> {
        let mut results = Vec::new();

        for m in info.models {
            let res = self.database.load_model(m);
            if let Err(ref e) = res {
                warn!("Failed to load model {}: {}", m, e);
            }
            results.push(res);
        }

        for i in info.images {
            let res = self.database.load_image(i);
            if let Err(ref e) = res {
                warn!("Failed to load image {}: {}", i, e);
            }
            results.push(res);
        }

        results.into_iter().collect::<Result<(), _>>()
    }
}

impl Drop for RenderEngine {
    fn drop(&mut self) {
        if let Some(display) = self.display.take() {
            if let Some(ctx) = self.ctx.as_mut() {
                ctx.destroy_display(display);
            }
        }
        if let Some(ctx) = self.ctx.take() {
            ctx.destroy();
        }
    }
}
