use std::ptr::NonNull;

use dashi::*;
use glam::{Mat4, Vec3};
use meshi_ffi_structs::DirectionalLightInfo;
use meshi_utils::MeshiError;
use resource_pool::resource_list::ResourceList;

use crate::{DirectionalLight, RenderObject, RenderObjectInfo, RgbaImage};

struct MaterialData {
    material: ShaderResource,
}

struct DeviceData {
    cameras: ResourceList<ShaderResource>,
    materials: ResourceList<ShaderResource>,
    transformations: ResourceList<ShaderResource>,
}

struct HostData {}

pub struct DeferredRenderer {
    ctx: Context,
}

pub struct DeferredRendererInfo {
    pub headless: bool,
}

impl DeferredRenderer {
    pub fn new(info: &DeferredRendererInfo) -> Self {
        let mut ctx = if info.headless {
            Context::new(&Default::default()).expect("");
        } else {
            Context::headless(&Default::default()).expect("");
        };

        todo!()
    }

    pub fn context(&mut self) -> &mut Context {
        &mut self.ctx
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
    }

    pub fn render_to_image(&mut self, extent: [u32; 2]) -> Result<RgbaImage, MeshiError> {
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
}
