pub mod event;
pub(crate) mod utils;
mod render;
pub mod structs;

use dashi::{Context, Display, DisplayInfo, Handle};
use glam::{Mat4, Vec3, Vec4};
use meshi_ffi_structs::{DirectionalLightInfo, MeshObjectInfo};
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

    pub fn initialize(&mut self, db: &mut DB) {
        self.db = Some(NonNull::new(db).expect("lmao"));
    }

    pub fn context(&mut self) -> &mut Context {
        self.renderer.context()
    }

    pub fn shut_down(&mut self) {
        todo!()
    }

    pub fn register_directional_light(
        &mut self,
        info: &DirectionalLightInfo,
    ) -> Handle<DirectionalLight> {
        todo!()
    }

    pub fn set_directional_light_transform(
        &mut self,
        handle: Handle<DirectionalLight>,
        transform: &Mat4,
    ) {
        todo!()
    }

    pub fn set_directional_light_info(
        &mut self,
        handle: Handle<DirectionalLight>,
        info: &DirectionalLightInfo,
    ) {
        todo!()
    }

    pub fn release_directional_light(&mut self, handle: Handle<DirectionalLight>) {
        todo!() //self.directional_lights.release(handle);
    }

    pub fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError> {
        todo!()
    }

    pub fn release_object(&mut self, handle: Handle<RenderObject>) {
        todo!()
    }

    pub fn register_object_with_renderer(&mut self, handle: Handle<RenderObject>) {
        todo!()
    }

    pub fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
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
    
    pub fn render_to_image(&mut self, extent: [u32; 2]) -> Result<RgbaImage, MeshiError> {
        todo!()
    }
    
    pub fn register_camera(&mut self, initial_transform: &Mat4) -> Handle<Camera> {
        todo!()
    }
    
    pub fn set_camera_as_active(&mut self, camera: Handle<Camera>) {
        todo!()
    }

    pub fn release_camera(&mut self, camera: Handle<Camera>) {
        todo!()
    }

    pub fn set_projection(&mut self, proj: &Mat4) {
        todo!()
    }

    pub fn set_capture_mouse(&mut self, capture: bool) {
        let _ = capture; // window management handled by renderer
    }
    pub fn set_camera(&mut self, camera: &Mat4) {
        todo!()
    }

    pub fn camera_position(&self) -> Vec3 {
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
