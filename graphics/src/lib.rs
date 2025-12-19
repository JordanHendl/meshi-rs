pub mod event;
mod render;
pub mod structs;
pub(crate) mod utils;

use dashi::{Context, Display, DisplayInfo, Handle};
use furikake::BindlessState;
pub use furikake::types::*;
use glam::{Mat4, Vec3, Vec4};
use meshi_ffi_structs::*;
use meshi_utils::MeshiError;
use meta::{DeviceMesh, DeviceModel};
pub use noren::*;
use render::deferred::{DeferredRenderer, DeferredRendererInfo};
use std::{ffi::c_void, ptr::NonNull};
pub use structs::*;

pub struct RenderEngine {
    display: Option<Display>,
    renderer: DeferredRenderer,
    db: Option<NonNull<DB>>,
}

impl RenderEngine {
    pub fn new(info: &RenderEngineInfo) -> Result<Self, MeshiError> {
        let mut renderer = DeferredRenderer::new(&DeferredRendererInfo {
            headless: info.headless,
        });
        let display = if info.headless {
            Some(renderer.context().make_display(&DisplayInfo {
                window: todo!(),
                vsync: todo!(),
                buffering: todo!(),
            })?)
        } else {
            None
        };

        Ok(Self {
            display,
            renderer,
            db: None,
        })
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        self.db = Some(NonNull::new(db).expect("lmao"));
        self.renderer.initialize_database(db);
    }

    pub fn context(&mut self) -> &mut Context {
        self.renderer.context()
    }

    pub fn shut_down(&mut self) {
        todo!()
    }

    pub fn register_light(&mut self, info: &LightInfo) -> Handle<Light> {
        todo!()
    }

    pub fn set_light_transform(&mut self, handle: Handle<Light>, transform: &Mat4) {
        todo!()
    }

    pub fn set_light_info(
        &mut self,
        handle: Handle<Light>,
        info: &LightInfo,
    ) {
        todo!()
    }

    pub fn release_light(&mut self, handle: Handle<Light>) {
        todo!() //self.lights.release(handle);
    }

    pub fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError> {
        self.renderer.register_object(info)
    }

    pub fn release_object(&mut self, handle: Handle<RenderObject>) {
        todo!()
    }

    pub fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
        todo!()
    }

    pub fn object_transform(&self, handle: Handle<RenderObject>) -> glam::Mat4 {
        todo!()
    }

    pub fn update(&mut self, _delta_time: f32) {
        //        use winit::event_loop::ControlFlow;
        //        use winit::platform::run_return::EventLoopExtRunReturn;
        //
        //        if self.event_cb.is_some() {
        //            let cb = self.event_cb.as_mut().unwrap();
        //            let mut triggered = false;
        //
        //            if let Some(event_loop) = &mut self.event_loop {
        //                event_loop.run_return(|event, _target, control_flow| {
        //                    *control_flow = ControlFlow::Exit;
        //                    if let Some(mut e) = event::from_winit_event(&event) {
        //                        triggered = true;
        //                        let c = cb.event_cb;
        //                        c(&mut e, cb.user_data);
        //                    }
        //                });
        //            }
        //
        //            if !triggered {
        //                let mut synthetic: event::Event = unsafe { std::mem::zeroed() };
        //                let c = cb.event_cb;
        //                c(&mut synthetic, cb.user_data);
        //            }
        //        }
        //
    }
    
    pub fn register_display(&mut self, info: dashi::DisplayInfo) -> Handle<Display> {
        todo!()
    }
    
    pub fn render_to_image(&mut self, extent: [u32; 2]) -> Result<RgbaImage, MeshiError> {
        todo!()
    }

    pub fn set_capture_mouse(&mut self, capture: bool) {
        let _ = capture; // window management handled by renderer
    }

    pub fn register_camera(&mut self, initial_transform: &Mat4) -> Handle<Camera> {
        let mut h = Handle::default();
        self.renderer
            .state()
            .reserved_mut(
                "meshi_bindless_cameras",
                |a: &mut furikake::reservations::bindless_camera::ReservedBindlessCamera| {
                    h = a.add_camera();
                    let c = a.camera_mut(h);
                    c.set_transform(initial_transform.clone());
                },
            )
            .unwrap();

        h
    }

    pub fn release_camera(&mut self, camera: Handle<Camera>) {
        todo!()
    }

    pub fn set_camera_perspective(
        &mut self,
        camera: Handle<Camera>,
        fov_y_radians: f32,
        width: f32,
        height: f32,
        near: f32,
        far: f32,
    ) {
        self.renderer
            .state()
            .reserved_mut(
                "meshi_bindless_cameras",
                |a: &mut furikake::reservations::bindless_camera::ReservedBindlessCamera| {
                    let c = a.camera_mut(camera);
                    c.set_perspective(fov_y_radians, width, height, near, far);
                },
            )
            .unwrap();
    }

    pub fn set_primary_camera(&mut self, camera: Handle<Camera>) {
        todo!()
    }

    pub fn camera_position(&self, camera: Handle<Camera>) -> Vec3 {
        todo!()
    }

    pub fn camera_transform(&self, camera: Handle<Camera>) -> Mat4 {
        todo!()
    }

    pub fn camera_view(&self, camera: Handle<Camera>) -> Mat4 {
        todo!()
    }

    pub fn set_event_cb(
        &mut self,
        event_cb: extern "C" fn(*mut event::Event, *mut c_void),
        user_data: *mut c_void,
    ) {
        //        self.event_cb = Some(EventCallbackInfo {
        //            event_cb,
        //            user_data,
        //        });
    }
}
