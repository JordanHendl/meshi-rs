use std::ptr::NonNull;

use dashi::*;
use furikake::BindlessState;
use glam::{Mat4, Vec3};
use meshi_ffi_structs::DirectionalLightInfo;
use meshi_utils::MeshiError;
use resource_pool::resource_list::ResourceList;
use tare::transient::TransientAllocator;
use crate::{
    DirectionalLight, RenderObject, RenderObjectInfo, RgbaImage, render::scene::*,
};

use super::scene::GPUScene;

//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////

struct MaterialData {
    material: ShaderResource,
}

struct DeviceData {
    materials: ResourceList<ShaderResource>,
    transformations: ResourceList<ShaderResource>,
}

struct HostData {}

pub struct DeferredRendererInfo {
    pub headless: bool,
}

//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////

pub struct DeferredRenderer {
    ctx: Box<Context>,
    state: Box<BindlessState>,
    scene: GPUScene<furikake::BindlessState>,
    alloc: TransientAllocator,
}

impl DeferredRenderer {
    pub fn new(info: &DeferredRendererInfo) -> Self {
        let mut ctx = if info.headless {
            Box::new(Context::new(&Default::default()).expect(""))
        } else {
            Box::new(Context::headless(&Default::default()).expect(""))
        };

        let mut state = Box::new(BindlessState::new(&mut ctx));

        let scene = GPUScene::new(
            &GPUSceneInfo {
                name: "[MESHI] Deferred Renderer Scene",
                ctx: ctx.as_mut(),
                draw_bins: &[SceneBin {
                    id: 0,
                    mask: 0x000001,

                }],
                ..Default::default()
            },
            state.as_mut(),
        );

        let alloc = TransientAllocator::new(ctx.as_mut());

        Self {
            ctx,
            state,
            scene,
            alloc,
        }
    }

    pub fn context(&mut self) -> &mut Context {
        &mut self.ctx
    }

    pub fn state(&mut self) -> &mut BindlessState {
        &mut self.state
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

    pub fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
        todo!()
    }

    pub fn update(&mut self, _delta_time: f32) {
        let cmds = self.scene.cull();
        todo!()
    }

    pub fn render_to_image(&mut self, extent: [u32; 2]) -> Result<RgbaImage, MeshiError> {
        todo!()
    }

    pub fn set_capture_mouse(&mut self, capture: bool) {
        let _ = capture; // window management handled by renderer
    }
}
