pub mod deferred;
pub mod environment;
pub mod forward;
pub mod text;
mod gpu_draw_builder;
mod particle_system;
mod skinning;
mod scene;

use crate::{AnimationState, CloudSettings, RenderObject, RenderObjectInfo, TextInfo, TextObject};
use dashi::{Context, Handle, ImageView, SampleCount, Semaphore, Viewport};
use furikake::{BindlessState, types::Camera, types::Material};
use glam::Mat4;
use meshi_utils::MeshiError;
use noren::DB;
use std::time::{Duration, Instant};

pub struct RendererInfo {
    pub headless: bool,
    pub initial_viewport: Viewport,
    pub sample_count: SampleCount,
}

pub struct ViewOutput {
    pub camera: Handle<Camera>,
    pub image: ImageView,
    pub semaphore: Handle<Semaphore>,
}

pub struct FrameTimer {
    total: Duration,
    frames: u64,
    report_every: u64,
    last_frame: Option<Instant>,
}

impl FrameTimer {
    pub fn new(report_every: u64) -> Self {
        Self {
            total: Duration::ZERO,
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
        self.total += duration;
        self.frames += 1;

        if self.report_every > 0 && self.frames % self.report_every == 0 {
            let avg_ms = self.average_ms().unwrap_or(0.0);
            Some((avg_ms, self.frames))
        } else {
            None
        }
    }

    pub fn average_ms(&self) -> Option<f64> {
        if self.frames == 0 {
            None
        } else {
            Some(self.total.as_secs_f64() * 1000.0 / self.frames as f64)
        }
    }
}

pub trait Renderer {
    fn context(&mut self) -> &'static mut Context;
    fn state(&mut self) -> &mut BindlessState;
    fn initialize_database(&mut self, db: &mut DB);
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
    fn register_text(&mut self, info: &TextInfo) -> Handle<TextObject>;
    fn release_text(&mut self, handle: Handle<TextObject>);
    fn set_text(&mut self, handle: Handle<TextObject>, text: &str);
    fn set_text_info(&mut self, handle: Handle<TextObject>, info: &TextInfo);
    fn update(
        &mut self,
        sems: &[Handle<Semaphore>],
        views: &[Handle<Camera>],
        delta_time: f32,
    ) -> Vec<ViewOutput>;
    fn cloud_settings(&self) -> CloudSettings;
    fn set_cloud_settings(&mut self, settings: CloudSettings);
    fn set_cloud_weather_map(&mut self, view: Option<ImageView>);
    fn shut_down(self: Box<Self>);
}
