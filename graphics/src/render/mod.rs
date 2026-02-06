mod debug_layer;
pub mod deferred;
pub mod environment;
pub mod forward;
mod gpu_draw_builder;
pub mod gui;
mod particle_system;
mod scene;
mod skinning;
pub mod text;

use crate::gui::GuiFrame;
use crate::{
    AnimationState, CloudSettings, GuiInfo, GuiObject, RenderObject, RenderObjectInfo,
    ShadowCascadeSettings, TextInfo, TextObject,
};
use bumpalo_herd::Herd;
use dashi::{Context, Handle, ImageView, SampleCount, Semaphore, Viewport};
use furikake::{BindlessState, types::Camera, types::Light, types::Material};
use glam::Mat4;
use meshi_ffi_structs::LightInfo;
use meshi_utils::MeshiError;
use noren::DB;
use noren::RDBFile;
use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub struct RendererInfo {
    pub headless: bool,
    pub initial_viewport: Viewport,
    pub sample_count: SampleCount,
    pub shadow_cascades: ShadowCascadeSettings,
}

pub struct ViewOutput {
    pub camera: Handle<Camera>,
    pub image: ImageView,
    pub semaphore: Handle<Semaphore>,
}

#[derive(Clone, Copy, Debug)]
pub struct SpotShadowLight {
    pub handle: Handle<Light>,
    pub info: LightInfo,
}

pub struct FrameTimer {
    rolling_total: Duration,
    window: VecDeque<Duration>,
    window_size: usize,
    frames: u64,
    report_every: u64,
    last_frame: Option<Instant>,
}

pub fn global_bump() -> &'static Herd {
    static HERD: OnceLock<Herd> = OnceLock::new();
    HERD.get_or_init(Herd::new)
}

impl FrameTimer {
    pub fn new(report_every: u64) -> Self {
        Self {
            rolling_total: Duration::ZERO,
            window: VecDeque::new(),
            window_size: 60,
            frames: 0,
            report_every,
            last_frame: None,
        }
    }

    pub fn start(&mut self) {
        self.last_frame = Some(Instant::now());
    }

    pub fn record_frame(&mut self) -> Option<(f64, u64)> {
        let now = Instant::now();
        let Some(last_frame) = self.last_frame.replace(now) else {
            return None;
        };
        let duration = now.saturating_duration_since(last_frame);
        self.record_duration(duration)
    }

    pub fn record_duration(&mut self, duration: Duration) -> Option<(f64, u64)> {
        self.rolling_total += duration;
        self.window.push_back(duration);
        if self.window.len() > self.window_size {
            if let Some(removed) = self.window.pop_front() {
                self.rolling_total = self.rolling_total.saturating_sub(removed);
            }
        }
        self.frames += 1;

        if self.report_every > 0 && self.frames % self.report_every == 0 {
            let avg_ms = self.average_ms().unwrap_or(0.0);
            Some((avg_ms, self.frames))
        } else {
            None
        }
    }

    pub fn average_ms(&self) -> Option<f64> {
        if self.window.is_empty() {
            None
        } else {
            Some(self.rolling_total.as_secs_f64() * 1000.0 / self.window.len() as f64)
        }
    }
}

pub trait Renderer {
    fn viewport(&self) -> Viewport;
    fn context(&mut self) -> &'static mut Context;
    fn state(&mut self) -> &mut BindlessState;
    fn initialize_database(&mut self, db: &mut DB);
    fn set_skybox_cubemap(&mut self, cubemap: noren::rdb::imagery::DeviceCubemap);
    fn set_skybox_settings(
        &mut self,
        settings: crate::render::environment::sky::SkyboxFrameSettings,
    );
    fn set_sky_settings(&mut self, settings: crate::render::environment::sky::SkyFrameSettings);
    fn set_ocean_settings(
        &mut self,
        settings: crate::render::environment::ocean::OceanFrameSettings,
    );
    fn set_spot_shadow_light(&mut self, light: Option<SpotShadowLight>);
    fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError>;
    fn set_skinned_animation_state(&mut self, handle: Handle<RenderObject>, state: AnimationState);
    fn set_billboard_texture(&mut self, handle: Handle<RenderObject>, texture_id: u32);
    fn set_billboard_material(
        &mut self,
        handle: Handle<RenderObject>,
        material: Option<Handle<Material>>,
    );
    fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &Mat4);
    fn object_transform(&self, handle: Handle<RenderObject>) -> Mat4;
    fn release_object(&mut self, handle: Handle<RenderObject>);
    fn register_text(&mut self, info: &TextInfo) -> Handle<TextObject>;
    fn release_text(&mut self, handle: Handle<TextObject>);
    fn set_text(&mut self, handle: Handle<TextObject>, text: &str);
    fn set_text_info(&mut self, handle: Handle<TextObject>, info: &TextInfo);
    fn register_gui(&mut self, info: &GuiInfo) -> Handle<GuiObject>;
    fn release_gui(&mut self, handle: Handle<GuiObject>);
    fn set_gui_info(&mut self, handle: Handle<GuiObject>, info: &GuiInfo);
    fn set_gui_visibility(&mut self, handle: Handle<GuiObject>, visible: bool);
    fn upload_gui_frame(&mut self, frame: GuiFrame);
    fn update(
        &mut self,
        sems: &[Handle<Semaphore>],
        views: &[Handle<Camera>],
        delta_time: f32,
    ) -> Vec<ViewOutput>;
    fn cloud_settings(&self) -> CloudSettings;
    fn set_cloud_settings(&mut self, settings: CloudSettings);
    fn set_cloud_weather_map(&mut self, view: Option<ImageView>);
    fn set_terrain_project_key(&mut self, project_key: &str);
    fn set_terrain_rdb(&mut self, rdb: &mut RDBFile, project_key: &str);
    fn shut_down(self: Box<Self>);
}
