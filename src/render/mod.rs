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

#[derive(Default)]
#[repr(C)]
pub struct DirectionalLightInfo {
    pub direction: Vec4,
    pub color: Vec4,
    pub intensity: f32,
}

pub struct DirectionalLight {}
pub struct CameraInfo<'a> {
    pub pass: &'a str,
    pub transform: Mat4,
    pub projection: Mat4,
}

#[derive(Default)]
pub struct RenderEngineInfo<'a> {
    pub application_path: String,
    pub scene_info: Option<SceneInfo<'a>>,
}

struct EventCallbackInfo {
    event_cb: extern "C" fn(*mut event::Event, *mut c_void),
    user_data: *mut c_void,
}

#[allow(dead_code)]
pub struct RenderEngine {
    ctx: Box<dashi::Context>,
    database: Database,
    event_cb: Option<EventCallbackInfo>,
    mesh_objects: Pool<MeshObject>,
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
        let mut ctx = Box::new(gpu::Context::new(&ContextInfo { device }).unwrap());
//        let event_pump = ctx.get_sdl_ctx().event_pump().unwrap();
//        let mut scene = Box::new(miso::Scene::new(
//            &mut ctx,
//            &miso::SceneInfo {
//                cfg: cfg.scene_cfg_path,
//            },
//        ));

        let database =
            Database::new(cfg.database_path.as_ref().unwrap(), &mut ctx).unwrap();

//        let global_camera = scene.register_camera(&CameraInfo {
//            pass: "ALL",
//            transform: Default::default(),
//            projection: Default::default(),
//        });

        let s = Self {
            ctx,
//            scene,
            database,
            event_cb: None,
            mesh_objects: Default::default(),
//            global_camera,
        };

        s
    }

    pub fn register_directional_light(&mut self, info: &DirectionalLightInfo) -> Handle<DirectionalLight> {
        todo!()
//        self.scene.register_directional_light(info)
    }

    pub fn register_mesh_object(&mut self, info: &FFIMeshObjectInfo) -> Handle<MeshObject> {
        todo!()
//        let info: MeshObjectInfo = info.into();
//        info!(
//            "Registering Mesh Object {} with material {}",
//            info.mesh, info.material
//        );
 //       let object = info.make_object(&mut self.database, &mut self.scene);
//        self.mesh_objects.insert(object).unwrap()
    }

    pub fn set_mesh_object_transform(
        &mut self,
        handle: Handle<MeshObject>,
        transform: &glam::Mat4,
    ) {
//        if let Some(m) = self.mesh_objects.get_ref(handle) {
//            for t in &m.targets {
//                self.scene.update_object_transform(*t, transform);
//            }
//        }
    }

    pub fn update(&mut self, _delta_time: f32) {
//        for event in self.event_pump.poll_iter() {
//            if let Some(cb) = self.event_cb.as_mut() {
//                let mut e: event::Event = event.into();
//                let c = cb.event_cb;
//                c(&mut e, cb.user_data);
//            }
//        }

//        self.scene.update();
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
 //       self.event_cb = Some(EventCallbackInfo {
 //           event_cb,
 //           user_data,
 //       });
    }

    pub fn set_scene(&mut self, _info: &SceneInfo) {
        todo!()
    }
}
