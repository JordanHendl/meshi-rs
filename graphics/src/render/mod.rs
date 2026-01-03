pub mod deferred;
pub mod environment;
pub mod forward;
mod scene;

use crate::{AnimationState, RenderObject, RenderObjectInfo};
use dashi::{Context, Handle, ImageView, SampleCount, Semaphore, Viewport};
use furikake::{BindlessState, types::Camera, types::Material};
use glam::Mat4;
use meshi_utils::MeshiError;
use noren::DB;

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
