pub mod deferred;
pub mod environment;
pub mod forward;
mod skinning;
mod scene;

use crate::{AnimationState, RenderObject, RenderObjectInfo};
use dashi::{Context, Handle, ImageView, SampleCount, Semaphore, Viewport};
use furikake::{BindlessState, types::Camera, types::Material};
use glam::Mat4;
use meshi_utils::MeshiError;
use noren::DB;
use std::time::Duration;

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
}

impl FrameTimer {
    pub fn new(report_every: u64) -> Self {
        Self {
            total: Duration::ZERO,
            frames: 0,
            report_every,
        }
    }

    pub fn record(&mut self, duration: Duration) -> Option<(f64, u64)> {
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
    fn update(
        &mut self,
        sems: &[Handle<Semaphore>],
        views: &[Handle<Camera>],
        delta_time: f32,
    ) -> Vec<ViewOutput>;
    fn shut_down(self: Box<Self>);
}
