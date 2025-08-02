use std::ffi::c_void;

use dashi::{
    utils::{Handle, Pool},
    *,
};
use database::Database;
use glam::{Mat4, Vec4};
use tracing::info;

use crate::object::{FFIMeshObjectInfo, MeshObject, MeshObjectInfo};
pub mod config;
pub mod database;
pub mod event;

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

#[derive(Default)]
pub struct RenderEngineInfo<'a> {
    pub application_path: String,
    pub scene_info: Option<SceneInfo<'a>>,
    pub headless: bool,
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
}

#[allow(dead_code)]
impl RenderEngine {
    pub fn new(info: &RenderEngineInfo) -> Self {
        let device = DeviceSelector::new()
            .unwrap()
            .select(DeviceFilter::default().add_required_type(DeviceType::Dedicated))
            .unwrap_or_default();

        info!("Initializing Render Engine with device {}", device);

        let cfg = config::RenderEngineConfig {
            scene_cfg_path: Some(format!("{}/koji.json", info.application_path)),
            database_path: Some(format!("{}/database", info.application_path)),
        };

        // The GPU context that holds all the data.
        let mut ctx = if info.headless {
            Box::new(gpu::Context::headless(&ContextInfo { device }).unwrap())
        } else {
            Box::new(gpu::Context::new(&ContextInfo { device }).unwrap())
        };

        let display = if info.headless {
            None
        } else {
            Some(ctx.make_display(&Default::default()).unwrap())
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

        let database = Database::new(cfg.database_path.as_ref().unwrap(), &mut ctx).unwrap();

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
        };

        s
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

    pub fn register_mesh_object(&mut self, info: &FFIMeshObjectInfo) -> Handle<MeshObject> {
        let info: MeshObjectInfo = info.into();
        let object = info
            .make_object(&mut self.database)
            .expect("failed to create mesh object");
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_cube(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_CUBE",
            material: "MESHI_CUBE",
            transform: Mat4::IDENTITY,
        };
        let object = info
            .make_object(&mut self.database)
            .expect("failed to create mesh object");
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_sphere(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_SPHERE",
            material: "MESHI_SPHERE",
            transform: Mat4::IDENTITY,
        };
        let object = info
            .make_object(&mut self.database)
            .expect("failed to create mesh object");
        self.mesh_objects.insert(object).unwrap()
    }

    pub fn create_triangle(&mut self) -> Handle<MeshObject> {
        let info = MeshObjectInfo {
            mesh: "MESHI_TRIANGLE",
            material: "MESHI_TRIANGLE",
            transform: Mat4::IDENTITY,
        };
        let object = info
            .make_object(&mut self.database)
            .expect("failed to create mesh object");
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
                    info!(
                        "Submitting transform for mesh '{}'", 
                        target.mesh.name
                    );
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
    }

    pub fn set_projection(&mut self, proj: &Mat4) {
        //        self.scene
        //            .update_camera_projection(self.global_camera, proj);
    }

    pub fn set_capture_mouse(&mut self, capture: bool) {
        //        self.ctx.get_sdl_ctx().mouse().set_relative_mouse_mode(capture);
    }
    pub fn set_camera(&mut self, camera: &Mat4) {
        //       self.scene
        //           .update_camera_transform(self.global_camera, camera);
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
        for m in info.models {
            self.database.load_model(m)?;
        }
        for i in info.images {
            self.database.load_image(i)?;
        }
        Ok(())
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
