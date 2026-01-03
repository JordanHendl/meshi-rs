mod render;
pub mod structs;
pub(crate) mod utils;

use dashi::driver::command::*;
use dashi::execution::CommandRing;
use dashi::utils::Pool;
use dashi::{
    Buffer, CommandQueueInfo2, CommandStream, Context, Display as DashiDisplay,
    DisplayInfo as DashiDisplayInfo, FRect2D, Filter, Handle, ImageView, QueueType, Rect2D,
    SampleCount, SubmitInfo, SubmitInfo2, Viewport,
};
pub use furikake::types::AnimationState as FAnimationState;
pub use furikake::types::{Camera, Light, Material};
use glam::{Mat4, Vec3};
use meshi_ffi_structs::*;
use meshi_utils::MeshiError;
pub use noren::*;
use render::deferred::DeferredRenderer;
use render::forward::ForwardRenderer;
use render::{Renderer, RendererInfo};
use std::collections::{HashMap, HashSet};
use std::{ffi::c_void, ptr::NonNull};
pub use structs::*;
use tracing::{error, info, warn};

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
    renderer: Box<dyn Renderer>,
    displays: Pool<Display>,
    event_cb: Option<EventCallbackInfo>,
    blit_queue: CommandRing,
    event_loop: Option<winit::event_loop::EventLoop<()>>,
    db: Option<NonNull<DB>>,
}

impl RenderEngine {
    pub fn new(info: &RenderEngineInfo) -> Result<Self, MeshiError> {
        let extent = info.canvas_extent.unwrap_or([1024, 1024]);
        let sample_count = info.sample_count.unwrap_or(SampleCount::S4);

        let renderer_info = RendererInfo {
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
            sample_count,
        };

        let mut renderer: Box<dyn Renderer> = match info.renderer {
            RendererSelect::Deferred => Box::new(DeferredRenderer::new(&renderer_info)),
            RendererSelect::Forward => Box::new(ForwardRenderer::new(&renderer_info)),
        };

        let event_loop = if cfg!(test) || info.headless {
            None
        } else {
            Some(winit::event_loop::EventLoop::new())
        };
        let blit_queue = renderer
            .context()
            .make_command_ring(&CommandQueueInfo2 {
                debug_name: "[BLIT]",
                parent: None,
                queue_type: QueueType::Graphics,
            })
            .expect("Failed to make render queue");

        Ok(Self {
            displays: Default::default(),
            renderer,
            db: None,
            event_cb: None,
            event_loop,
            blit_queue,
        })
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        self.db = Some(NonNull::new(db).expect("lmao"));
        self.renderer.initialize_database(db);
    }

    pub fn context(&mut self) -> &'static mut Context {
        self.renderer.context()
    }

    pub fn shut_down(mut self) {
        info!("Shutting down render engine.");
        self.event_cb = None;
        self.event_loop = None;

        let mut displays = std::mem::take(&mut self.displays);
        displays.for_each_occupied_mut(|display| match &mut display.raw {
            DisplayImpl::Window(window) => {
                let _ = window.take();
            }
            DisplayImpl::CPUImage(_cpuimage) => {}
        });
        drop(displays);
        self.renderer.shut_down();
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

    pub fn set_skinned_object_animation(
        &mut self,
        handle: Handle<RenderObject>,
        state: AnimationState,
    ) {
        self.renderer.set_skinned_animation_state(handle, state);
    }

    pub fn set_billboard_texture(&mut self, handle: Handle<RenderObject>, texture_id: u32) {
        self.renderer.set_billboard_texture(handle, texture_id);
    }

    pub fn set_billboard_material(
        &mut self,
        handle: Handle<RenderObject>,
        material: Option<Handle<Material>>,
    ) {
        self.renderer.set_billboard_material(handle, material);
    }

    pub fn release_object(&mut self, handle: Handle<RenderObject>) {
        todo!()
    }

    pub fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
        self.renderer.set_object_transform(handle, transform);
    }

    pub fn object_transform(&self, handle: Handle<RenderObject>) -> glam::Mat4 {
        self.renderer.object_transform(handle)
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
        let mut views = Vec::new();
        let mut seen = HashSet::new();

        self.displays.for_each_occupied(|dis| {
            if dis.scene.valid() && seen.insert(dis.scene) {
                views.push(dis.scene);
            }
        });

        let view_outputs = self.renderer.update(&[], &views, delta_time);
        let mut outputs_by_camera = HashMap::new();
        for output in view_outputs {
            outputs_by_camera.insert(output.camera, output);
        }

        let ctx = self.context();
        self.displays.for_each_occupied_mut(|dis| {
            if !dis.scene.valid() {
                return;
            }

            let Some(output) = outputs_by_camera.get(&dis.scene) else {
                return;
            };

            match &mut dis.raw {
                DisplayImpl::Window(display) => {
                    let d = display.as_mut().unwrap();
                    let (img, acquire_sem, _, _) = ctx.acquire_new_image(d).unwrap();
                    let blit_sem = ctx.make_semaphore().expect("make blit semaphore");
                    self.blit_queue
                        .record(|c| {
                            CommandStream::new()
                                .begin()
                                .resolve_images(&MSImageResolve {
                                    src: output.image.img,
                                    dst: img.img,
                                    ..Default::default()
                                })
                                .prepare_for_presentation(img.img)
                                .end()
                                .append(c);
                        })
                        .expect("Failed to make commands");

                    //        self.cull_queue.wait_all().unwrap();

                    self.blit_queue
                        .submit(&SubmitInfo {
                            wait_sems: &[acquire_sem, output.semaphore],
                            signal_sems: &[blit_sem],
                        })
                        .expect("Failed to submit!");

                    ctx.present_display(d, &[blit_sem])
                        .expect("Failed to present to display!");
                }
                DisplayImpl::CPUImage(_cpuimage_output) => {
                    todo!("CPUImage display not yet implemented.")
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

        info!("Registered window {}", info.window.title);
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
