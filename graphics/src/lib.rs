pub mod gui;
mod render;
pub mod structs;
pub(crate) mod utils;

use crate::gui::debug::{DebugGui, DebugGuiBindings};
use dashi::driver::command::*;
use dashi::execution::CommandRing;
use dashi::utils::Pool;
use dashi::{
    AspectMask, Buffer, BufferInfo, BufferUsage, BufferView, CommandQueueInfo2, CommandStream,
    Context, Display as DashiDisplay, DisplayInfo as DashiDisplayInfo, FRect2D, Format, Handle,
    ImageInfo, ImageView, ImageViewType, MemoryVisibility, QueueType, Rect2D, SampleCount,
    SubmitInfo, SubresourceRange, Viewport,
};
pub use furikake::types::AnimationState as FAnimationState;
pub use furikake::types::{Camera, Light, Material};
use glam::{Mat3, Mat4, Vec2, Vec3};
use meshi_ffi_structs::*;
use meshi_utils::MeshiError;
pub use noren::*;
use render::deferred::DeferredRenderer;
pub use render::environment::clouds::CloudRenderer;
pub use render::environment::ocean::OceanFrameSettings;
pub use render::environment::sky::SkyFrameSettings;
pub use render::environment::sky::SkyboxFrameSettings;
pub use render::environment::terrain::TerrainRenderObject;
use render::forward::ForwardRenderer;
use render::{FrameTimer, Renderer, RendererInfo};
use std::collections::{HashMap, HashSet};
use std::{ffi::c_void, ptr::NonNull};
pub use structs::*;
use tracing::{info, warn};

pub type DisplayInfo = DashiDisplayInfo;
pub type WindowInfo = dashi::WindowInfo;
struct CPUImageOutput {
    img: ImageView,
    staging: Handle<Buffer>,
    width: u32,
    height: u32,
    format: Format,
    pixels: Vec<u8>,
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
    pending_skybox_entry: Option<String>,
    frame_timer: FrameTimer,
    environment_lighting: Option<EnvironmentLightingState>,
    debug_mode: bool,
    renderer_select: RendererSelect,
    debug_gui: DebugGui,
    pending_gui_frame: Option<gui::GuiFrame>,
    gui_input: gui::GuiInput,
    sky_settings: SkyFrameSettings,
    skybox_settings: SkyboxFrameSettings,
    ocean_settings: OceanFrameSettings,
    cloud_settings: CloudSettings,
}

#[derive(Clone, Debug)]
pub struct EnvironmentLightingSettings {
    pub sky: SkyFrameSettings,
    pub sun_light_intensity: f32,
    pub moon_light_intensity: f32,
}

impl Default for EnvironmentLightingSettings {
    fn default() -> Self {
        Self {
            sky: SkyFrameSettings::default(),
            sun_light_intensity: 1.0,
            moon_light_intensity: 0.1,
        }
    }
}

struct EnvironmentLightingState {
    sun_light: Handle<Light>,
    moon_light: Handle<Light>,
    settings: EnvironmentLightingSettings,
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

        let renderer_select = info.renderer;
        let mut renderer: Box<dyn Renderer> = match renderer_select {
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

        let cloud_settings = renderer.cloud_settings();

        Ok(Self {
            displays: Pool::new(8),
            renderer,
            db: None,
            event_cb: None,
            event_loop,
            blit_queue,
            pending_skybox_entry: info.skybox_cubemap_entry.clone(),
            frame_timer: FrameTimer::new(60),
            environment_lighting: None,
            debug_mode: info.debug_mode,
            renderer_select,
            debug_gui: DebugGui::new(),
            pending_gui_frame: None,
            gui_input: gui::GuiInput::default(),
            sky_settings: SkyFrameSettings::default(),
            skybox_settings: SkyboxFrameSettings::default(),
            ocean_settings: OceanFrameSettings::default(),
            cloud_settings,
        })
    }

    pub fn debug_mode(&self) -> bool {
        self.debug_mode
    }

    pub fn set_debug_mode(&mut self, enabled: bool) {
        self.debug_mode = enabled;
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        self.db = Some(NonNull::new(db).expect("lmao"));
        self.renderer.initialize_database(db);
        if let Some(entry) = self.pending_skybox_entry.take() {
            self.set_skybox_cubemap_entry(&entry);
        }
    }

    pub fn context(&mut self) -> &'static mut Context {
        self.renderer.context()
    }

    pub fn set_skybox_cubemap_entry(&mut self, entry: &str) {
        let Some(mut db) = self.db else {
            warn!("Attempted to set skybox cubemap without a database.");
            return;
        };

        match unsafe { db.as_mut() }
            .imagery_mut()
            .fetch_gpu_cubemap(entry)
        {
            Ok(cubemap) => {
                let mut settings = self.skybox_settings.clone();
                settings.cubemap = Some(cubemap);
                settings.use_procedural_cubemap = false;
                self.skybox_settings = settings.clone();
                self.renderer.set_skybox_settings(settings);
            }
            Err(err) => warn!("Failed to load skybox cubemap '{entry}': {err:?}"),
        }
    }

    pub fn set_skybox_settings(&mut self, settings: SkyboxFrameSettings) {
        self.skybox_settings = settings.clone();
        self.renderer.set_skybox_settings(settings);
    }

    pub fn set_sky_settings(&mut self, settings: SkyFrameSettings) {
        self.sky_settings = settings.clone();
        self.renderer.set_sky_settings(settings);
    }

    pub fn set_ocean_settings(&mut self, settings: OceanFrameSettings) {
        self.ocean_settings = settings;
        self.renderer.set_ocean_settings(settings);
    }

    pub fn set_environment_lighting(&mut self, settings: EnvironmentLightingSettings) {
        self.sky_settings = settings.sky.clone();
        let sky_settings = settings.sky.clone();
        let (sun_direction, moon_direction) = resolve_sun_moon_direction(&sky_settings);
        let sun_info = directional_light_info(
            sun_direction,
            sky_settings.sun_color,
            settings.sun_light_intensity,
        );
        let moon_info = directional_light_info(
            moon_direction,
            sky_settings.moon_color,
            settings.moon_light_intensity,
        );

        let existing_lights = if let Some(state) = &mut self.environment_lighting {
            state.settings = settings.clone();
            Some((state.sun_light, state.moon_light))
        } else {
            None
        };

        if let Some((sun_light, moon_light)) = existing_lights {
            self.set_light_info(sun_light, &sun_info);
            self.set_light_info(moon_light, &moon_info);
        } else {
            let sun_light = self.register_light(&sun_info);
            let moon_light = self.register_light(&moon_info);
            self.environment_lighting = Some(EnvironmentLightingState {
                sun_light,
                moon_light,
                settings: settings.clone(),
            });
        }

        self.renderer.set_sky_settings(sky_settings);
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
        if !handle.valid() {
            return;
        }
        self.renderer
            .state()
            .reserved_mut(
                "meshi_bindless_lights",
                |lights: &mut furikake::reservations::bindless_lights::ReservedBindlessLights| {
                    *lights.light_mut(handle) = pack_gpu_light(*info);
                },
            )
            .unwrap();
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

    pub fn register_text(&mut self, info: &TextInfo) -> Handle<TextObject> {
        self.renderer.register_text(info)
    }

    pub fn register_gui(&mut self, info: &GuiInfo) -> Handle<GuiObject> {
        self.renderer.register_gui(info)
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

    pub fn cloud_settings(&self) -> CloudSettings {
        self.cloud_settings
    }

    pub fn set_cloud_settings(&mut self, settings: CloudSettings) {
        self.cloud_settings = settings;
        self.renderer.set_cloud_settings(self.cloud_settings);
    }

    pub fn set_cloud_weather_map(&mut self, view: Option<ImageView>) {
        self.renderer.set_cloud_weather_map(view);
    }

    pub fn set_terrain_render_objects(
        &mut self,
        objects: &[render::environment::terrain::TerrainRenderObject],
    ) {
        self.renderer.set_terrain_render_objects(objects);
    }

    pub fn release_object(&mut self, handle: Handle<RenderObject>) {
        todo!()
    }

    pub fn release_text(&mut self, handle: Handle<TextObject>) {
        self.renderer.release_text(handle);
    }

    pub fn release_gui(&mut self, handle: Handle<GuiObject>) {
        self.renderer.release_gui(handle);
    }

    pub fn set_text(&mut self, handle: Handle<TextObject>, text: &str) {
        self.renderer.set_text(handle, text);
    }

    pub fn set_text_info(&mut self, handle: Handle<TextObject>, info: &TextInfo) {
        self.renderer.set_text_info(handle, info);
    }

    pub fn set_gui_info(&mut self, handle: Handle<GuiObject>, info: &GuiInfo) {
        self.renderer.set_gui_info(handle, info);
    }

    pub fn set_gui_visibility(&mut self, handle: Handle<GuiObject>, visible: bool) {
        self.renderer.set_gui_visibility(handle, visible);
    }

    pub fn upload_gui_frame(&mut self, frame: gui::GuiFrame) {
        self.pending_gui_frame = Some(frame);
    }

    pub fn gui_input(&self) -> &gui::GuiInput {
        &self.gui_input
    }

    pub fn gui_input_mut(&mut self) -> &mut gui::GuiInput {
        &mut self.gui_input
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

        let mut triggered = false;
        let debug_gui_ptr = &mut self.debug_gui as *mut DebugGui;
        let gui_input_ptr = &mut self.gui_input as *mut gui::GuiInput;

        if let Some(cb) = self.event_cb.as_mut() {
            self.displays.for_each_occupied_mut(|dis| {
                if let DisplayImpl::Window(Some(display)) = &mut dis.raw {
                    let event_loop = display.winit_event_loop();
                    event_loop.run_return(|event, _target, control_flow| {
                        *control_flow = ControlFlow::Exit;
                        if let Some(mut e) = event::from_winit_event(&event) {
                            triggered = true;
                            unsafe {
                                (*debug_gui_ptr).handle_event(&e);
                            }
                            unsafe {
                                (*gui_input_ptr).handle_event(&e);
                            }
                            let c = cb.event_cb;
                            c(&mut e, cb.user_data);
                        }
                    });
                }
            });

            if !triggered {
                let mut synthetic: event::Event = unsafe { std::mem::zeroed() };
                let c = cb.event_cb;
                c(&mut synthetic, cb.user_data);
            }
        } else {
            self.displays.for_each_occupied_mut(|dis| {
                if let DisplayImpl::Window(Some(display)) = &mut dis.raw {
                    let event_loop = display.winit_event_loop();
                    event_loop.run_return(|event, _target, control_flow| {
                        *control_flow = ControlFlow::Exit;
                        if let Some(e) = event::from_winit_event(&event) {
                            unsafe {
                                (*debug_gui_ptr).handle_event(&e);
                            }
                            unsafe {
                                (*gui_input_ptr).handle_event(&e);
                            }
                        }
                    });
                }
            });
        }
    }

    pub fn update(&mut self, delta_time: f32) {
        self.gui_input.begin_frame();
        self.publish_events();
        let viewport = self.renderer.viewport();
        let viewport_size = Vec2::new(viewport.area.w, viewport.area.h);
        let renderer_label = match self.renderer_select {
            RendererSelect::Deferred => "Deferred",
            RendererSelect::Forward => "Forward",
        };
        let bindings = DebugGuiBindings {
            debug_mode: &mut self.debug_mode as *mut bool,
            skybox_settings: &mut self.skybox_settings as *mut SkyboxFrameSettings,
            sky_settings: &mut self.sky_settings as *mut SkyFrameSettings,
            ocean_settings: &mut self.ocean_settings as *mut OceanFrameSettings,
            cloud_settings: &mut self.cloud_settings as *mut CloudSettings,
        };
        let debug_output = self
            .debug_gui
            .build_frame(viewport_size, renderer_label, bindings);

        if debug_output.skybox_dirty {
            self.renderer
                .set_skybox_settings(self.skybox_settings.clone());
        }
        if debug_output.sky_dirty {
            self.renderer.set_sky_settings(self.sky_settings.clone());
        }
        if debug_output.ocean_dirty {
            self.renderer
                .set_ocean_settings(self.ocean_settings.clone());
        }
        if debug_output.cloud_dirty {
            self.renderer
                .set_cloud_settings(self.cloud_settings.clone());
        }

        let mut gui_frame = self.pending_gui_frame.take().unwrap_or_default();
        if let Some(mut debug_frame) = debug_output.frame {
            gui_frame.batches.append(&mut debug_frame.batches);
            gui_frame.text_draws.append(&mut debug_frame.text_draws);
        }
        self.renderer.upload_gui_frame(gui_frame);

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
                                .append(c)
                                .unwrap();
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
                    self.blit_queue
                        .record(|c| {
                            CommandStream::new()
                                .begin()
                                .resolve_images(&MSImageResolve {
                                    src: output.image.img,
                                    dst: _cpuimage_output.img.img,
                                    ..Default::default()
                                })
                                .copy_image_to_buffer(&CopyImageBuffer {
                                    src: _cpuimage_output.img.img,
                                    dst: _cpuimage_output.staging,
                                    range: _cpuimage_output.img.range,
                                    dst_offset: 0,
                                })
                                .end()
                                .append(c)
                                .unwrap();
                        })
                        .expect("Failed to make commands");

                    self.blit_queue
                        .submit(&SubmitInfo {
                            wait_sems: &[output.semaphore],
                            signal_sems: &[],
                        })
                        .expect("Failed to submit!");
                }
            }
        });
        self.frame_timer.record_frame();
    }

    pub fn average_frame_time_ms(&self) -> Option<f64> {
        self.frame_timer.average_ms()
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
        let size = info.window.size;
        let format = Format::BGRA8;
        let img = self
            .context()
            .make_image(&ImageInfo {
                debug_name: "[MESHI CPU] Display Image",
                dim: [size[0], size[1], 1],
                layers: 1,
                format,
                mip_levels: 1,
                samples: SampleCount::S1,
                initial_data: None,
                ..Default::default()
            })
            .expect("Failed to make CPU display image");
        let img = ImageView {
            img,
            range: SubresourceRange::default(),
            aspect: AspectMask::Color,
            view_type: ImageViewType::Type2D,
        };
        let byte_size = size[0] as usize * size[1] as usize * 4;
        let staging = self
            .context()
            .make_buffer(&BufferInfo {
                debug_name: "[MESHI CPU] Display Staging",
                byte_size: byte_size as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::ALL,
                initial_data: None,
            })
            .expect("Failed to make CPU display staging buffer");

        self.displays
            .insert(Display {
                raw: DisplayImpl::CPUImage(CPUImageOutput {
                    img,
                    staging,
                    width: size[0],
                    height: size[1],
                    format,
                    pixels: vec![0; byte_size],
                }),
                scene: Default::default(),
            })
            .unwrap()
    }

    pub fn frame_dump(&mut self, display: Handle<Display>) -> Option<FFIImage> {
        if !display.valid() {
            return None;
        }

        let ctx = unsafe { &mut *(self.context() as *mut Context) };
        let output = match &mut self.displays.get_mut_ref(display)?.raw {
            DisplayImpl::CPUImage(output) => output,
            _ => return None,
        };

        if let Err(err) = self.blit_queue.wait_all() {
            warn!("Failed waiting on blit queue: {err:?}");
            return None;
        }

        let mapped = match ctx.map_buffer::<u8>(BufferView::new(output.staging)) {
            Ok(mapped) => mapped,
            Err(err) => {
                warn!("Failed to map CPU display buffer: {err:?}");
                return None;
            }
        };

        if output.pixels.len() != mapped.len() {
            output.pixels.resize(mapped.len(), 0);
        }
        output.pixels.copy_from_slice(mapped);

        if let Err(err) = ctx.unmap_buffer(output.staging) {
            warn!("Failed to unmap CPU display buffer: {err:?}");
        }

        Some(FFIImage {
            width: output.width,
            height: output.height,
            format: output.format as u32,
            pixels: output.pixels.as_ptr(),
        })
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
        if !camera.valid() {
            return;
        }

        self.renderer
            .state()
            .reserved_mut(
                "meshi_bindless_cameras",
                |a: &mut furikake::reservations::bindless_camera::ReservedBindlessCamera| {
                    a.remove_camera(camera);
                },
            )
            .unwrap();
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

    pub fn set_camera_transform(&mut self, camera: Handle<Camera>, transform: &Mat4) {
        if !camera.valid() {
            return;
        }

        self.renderer
            .state()
            .reserved_mut(
                "meshi_bindless_cameras",
                |a: &mut furikake::reservations::bindless_camera::ReservedBindlessCamera| {
                    let c = a.camera_mut(camera);
                    c.set_transform(*transform);
                },
            )
            .unwrap();
    }

    pub fn set_camera_position(&mut self, camera: Handle<Camera>, position: Vec3) {
        if !camera.valid() {
            return;
        }

        self.renderer
            .state()
            .reserved_mut(
                "meshi_bindless_cameras",
                |a: &mut furikake::reservations::bindless_camera::ReservedBindlessCamera| {
                    let c = a.camera_mut(camera);
                    c.set_position(position);
                },
            )
            .unwrap();
    }

    pub fn camera_position(&mut self, camera: Handle<Camera>) -> Vec3 {
        if !camera.valid() {
            return Vec3::ZERO;
        }

        let mut position = Vec3::ZERO;
        self.renderer
            .state()
            .reserved_mut(
                "meshi_bindless_cameras",
                |a: &mut furikake::reservations::bindless_camera::ReservedBindlessCamera| {
                    position = a.camera(camera).position();
                },
            )
            .unwrap();
        position
    }

    pub fn camera_transform(&mut self, camera: Handle<Camera>) -> Mat4 {
        if !camera.valid() {
            return Mat4::IDENTITY;
        }

        let mut transform = Mat4::IDENTITY;
        self.renderer
            .state()
            .reserved_mut(
                "meshi_bindless_cameras",
                |a: &mut furikake::reservations::bindless_camera::ReservedBindlessCamera| {
                    transform = a.camera(camera).as_matrix();
                },
            )
            .unwrap();
        transform
    }

    pub fn camera_view(&mut self, camera: Handle<Camera>) -> Mat4 {
        if !camera.valid() {
            return Mat4::IDENTITY;
        }

        let mut view = Mat4::IDENTITY;
        self.renderer
            .state()
            .reserved_mut(
                "meshi_bindless_cameras",
                |a: &mut furikake::reservations::bindless_camera::ReservedBindlessCamera| {
                    view = a.camera(camera).view_matrix();
                },
            )
            .unwrap();
        view
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

fn directional_light_info(direction: Vec3, color: Vec3, intensity: f32) -> LightInfo {
    LightInfo {
        ty: LightType::Directional,
        flags: LightFlags::CASTS_SHADOWS.bits(),
        intensity,
        range: 0.0,
        color_r: color.x,
        color_g: color.y,
        color_b: color.z,
        pos_x: 0.0,
        pos_y: 0.0,
        pos_z: 0.0,
        dir_x: direction.x,
        dir_y: direction.y,
        dir_z: direction.z,
        spot_inner_angle_rad: 0.0,
        spot_outer_angle_rad: 0.0,
        rect_half_width: 0.0,
        rect_half_height: 0.0,
    }
}

fn resolve_sun_moon_direction(settings: &SkyFrameSettings) -> (Vec3, Vec3) {
    let sun_dir = resolve_celestial_direction(
        settings.sun_direction,
        settings.time_of_day,
        settings.latitude_degrees,
        settings.longitude_degrees,
        false,
    );
    let moon_dir = resolve_celestial_direction(
        settings.moon_direction,
        settings.time_of_day,
        settings.latitude_degrees,
        settings.longitude_degrees,
        true,
    );
    (sun_dir, moon_dir)
}

fn resolve_celestial_direction(
    explicit: Option<Vec3>,
    time_of_day: Option<f32>,
    latitude_degrees: Option<f32>,
    longitude_degrees: Option<f32>,
    is_moon: bool,
) -> Vec3 {
    if let Some(direction) = explicit {
        if direction.length_squared() > 0.0 {
            return direction.normalize();
        }
    }

    if let Some(time) = time_of_day {
        let day_time = time.rem_euclid(24.0);
        let angle = day_time / 24.0 * std::f32::consts::TAU;
        let elevation = (angle - std::f32::consts::FRAC_PI_2).sin();
        let base = Vec3::new(angle.cos(), elevation, angle.sin());
        let latitude = latitude_degrees.unwrap_or(0.0).to_radians();
        let longitude = longitude_degrees.unwrap_or(0.0).to_radians();
        let rotation = Mat3::from_rotation_y(longitude) * Mat3::from_rotation_x(latitude);
        let mut dir = rotation * base;
        if is_moon {
            dir = -dir;
        }
        if dir.length_squared() > 0.0 {
            return dir.normalize();
        }
    }

    if is_moon { -Vec3::Y } else { Vec3::Y }
}
