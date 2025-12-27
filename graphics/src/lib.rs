mod render;
pub mod structs;
pub(crate) mod utils;

use dashi::utils::Pool;
use dashi::{
    Buffer, Context, Display as DashiDisplay, DisplayInfo as DashiDisplayInfo, FRect2D, Handle,
    ImageView, Rect2D, Viewport,
};
pub use furikake::types::*;
use furikake::BindlessState;
use glam::{Mat4, Vec3, Vec4};
use meshi_ffi_structs::*;
use meshi_utils::MeshiError;
use meta::{DeviceMesh, DeviceModel};
pub use noren::*;
use render::deferred::{DeferredRenderer, DeferredRendererInfo};
use std::collections::HashMap;
use std::{ffi::c_void, ptr::NonNull};
pub use structs::*;

pub type DisplayInfo = DashiDisplayInfo;
pub type WindowInfo = dashi::WindowInfo;
struct CPUImageOutput {
    img: ImageView,
    staging: Handle<Buffer>,
}

enum DisplayImpl {
    Window(Option<Box<DashiDisplay>>),
    CPUImage(CPUImageOutput),
}

pub struct Display {
    raw: DisplayImpl,
    scene: Handle<Camera>,
}

pub struct RenderEngine {
    renderer: DeferredRenderer,
    displays: Pool<Display>,
    event_cb: Option<EventCallbackInfo>,
    event_loop: Option<winit::event_loop::EventLoop<()>>,
    db: Option<NonNull<DB>>,
}

impl RenderEngine {
    pub fn new(info: &RenderEngineInfo) -> Result<Self, MeshiError> {
        let extent = info.canvas_extent.unwrap_or([1024, 1024]);

        let renderer = DeferredRenderer::new(&DeferredRendererInfo {
            headless: info.headless,
            initial_viewport: Viewport {
                area: FRect2D {
                    x: 0.0,
                    y: 0.0,
                    w: extent[0] as f32,
                    h: extent[1] as f32,
                },
                scissor: Rect2D {
                    x: 0,
                    y: 0,
                    w: extent[0],
                    h: extent[1],
                },
                ..Default::default()
            },
        });

        let event_loop = if cfg!(test) || info.headless {
            None
        } else {
            Some(winit::event_loop::EventLoop::new())
        };

        Ok(Self {
            displays: Default::default(),
            renderer,
            db: None,
            event_cb: None,
            event_loop,
        })
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        self.db = Some(NonNull::new(db).expect("lmao"));
        self.renderer.initialize_database(db);
    }

    pub fn context(&mut self) -> &'static mut Context {
        self.renderer.context()
    }

    pub fn shut_down(&mut self) {
        todo!()
    }

    pub fn register_light(&mut self, info: &LightInfo) -> Handle<Light> {
        let mut h = Handle::default();

        self.renderer
            .state()
            .reserved_mut(
                "meshi_bindless_lights",
                |lights: &mut furikake::reservations::bindless_lights::ReservedBindlessLights| {
                    h = lights.add_light();
                    *lights.light_mut(h) = pack_gpu_light(*info);
                },
            )
            .unwrap();

        h
    }

    pub fn set_light_transform(&mut self, handle: Handle<Light>, transform: &Mat4) {
        todo!()
    }

    pub fn set_light_info(&mut self, handle: Handle<Light>, info: &LightInfo) {
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
        self.renderer.set_object_transform(handle, transform);
    }

    pub fn object_transform(&self, handle: Handle<RenderObject>) -> glam::Mat4 {
        todo!()
    }

    fn publish_events(&mut self) {
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
        } else {
            if let Some(event_loop) = &mut self.event_loop {
                event_loop.run_return(|event, _target, control_flow| {
                    *control_flow = ControlFlow::Exit;
                    if let Some(mut _e) = event::from_winit_event(&event) {}
                });
            }
        }
    }

    pub fn update(&mut self, delta_time: f32) {
        self.publish_events();
        self.renderer.update(delta_time);
        
        let ctx = &mut self.context();
        self.displays.for_each_occupied_mut(|dis| {
            if dis.scene.valid() {
                match &mut dis.raw {
                    DisplayImpl::Window(display) => {
                        let d = display.as_mut().unwrap();
                        let (img, sem, idx, success) = ctx.acquire_new_image(d).unwrap();
                        
                        ctx.present_display(d, &[sem]).expect("Failed to present to display!");
                    },
                    DisplayImpl::CPUImage(cpuimage_output) => {
                        todo!("CPUImage display not yet implemented.")
                    },
                }
            }
        });
    }

    pub fn register_window_display(&mut self, info: dashi::DisplayInfo) -> Handle<Display> {
        let raw = Some(Box::new(
            self.context()
                .make_display(&info)
                .expect("Failed to make display!"),
        ));
        return self
            .displays
            .insert(Display {
                raw: DisplayImpl::Window(raw),
                scene: Default::default(),
            })
            .unwrap();
    }

    pub fn register_cpu_display(&mut self, info: dashi::DisplayInfo) -> Handle<Display> {
        todo!("Not yet implemented.");
        //        let raw = Some(Box::new(
        //            self.context()
        //                .make_display(&info)
        //                .expect("Failed to make display!"),
        //        ));
        //        return self
        //            .displays
        //            .insert(Display {
        //                raw: DisplayImpl::Window(raw),
        //                scene: Default::default(),
        //            })
        //            .unwrap();
    }

    pub fn frame_dump(&mut self, _display: Handle<Display>) -> Option<FFIImage> {
        None
    }

    pub fn attach_camera_to_display(&mut self, display: Handle<Display>, camera: Handle<Camera>) {
        if display.valid() {
            self.displays.get_mut_ref(display).unwrap().scene = camera;
        }
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
        self.event_cb = Some(EventCallbackInfo {
            event_cb,
            user_data,
        });
    }
}
