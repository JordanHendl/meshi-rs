use std::ptr::NonNull;

use crate::{RenderObject, RenderObjectInfo, RgbaImage, render::scene::*};
use dashi::*;
use furikake::BindlessState;
use glam::{Mat4, Vec3};
use meshi_ffi_structs::*;
use meshi_utils::MeshiError;
use noren::DB;
use resource_pool::resource_list::ResourceList;
use tare::transient::TransientAllocator;

use super::scene::GPUScene;
use tare::graph::*;

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
    pub initial_viewport: Viewport,
}

pub struct Display {
    raw: dashi::Display,
    attached_camera: Handle<crate::Camera>,
}

//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////

pub struct DeferredRenderer {
    ctx: Box<Context>,
    viewport: Viewport,
    state: Box<BindlessState>,
    db: Option<NonNull<DB>>,
    scene: GPUScene<furikake::BindlessState>,
    alloc: Box<TransientAllocator>,
    graph: RenderGraph,
}

impl DeferredRenderer {
    pub fn new(info: &DeferredRendererInfo) -> Self {
        let mut ctx = if info.headless {
            Box::new(Context::headless(&Default::default()).expect(""))
        } else {
            Box::new(Context::new(&Default::default()).expect(""))
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

        let mut alloc = Box::new(TransientAllocator::new(ctx.as_mut()));

        let graph = RenderGraph::new_with_transient_allocator(&mut ctx, &mut alloc);

        Self {
            ctx,
            state,
            scene,
            alloc,
            graph,
            db: None,
            viewport: info.initial_viewport,
        }
    }

    pub fn context(&mut self) -> &mut Context {
        &mut self.ctx
    }

    pub fn state(&mut self) -> &mut BindlessState {
        &mut self.state
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        db.import_dashi_context(&mut self.ctx);
        db.import_furikake_state(&mut self.state);
        self.db = Some(NonNull::new(db).expect("lmao"));
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
        //  self.scene.set_object_transform(handle, transform);
    }

    pub fn update(&mut self, _delta_time: f32) {

        let default_framebuffer_info = ImageInfo {
            debug_name: "",
            dim: [self.viewport.area.x as u32, self.viewport.area.y as u32, 1],
            layers: 1,
            format: Format::RGBA8,
            mip_levels: 1,
            samples: SampleCount::S1,
            initial_data: None,
        };

        let position = self.graph.make_image(&ImageInfo {
            debug_name: "[MESHI DEFERRED] Position Framebuffer",
            ..default_framebuffer_info
        });

        let normal = self.graph.make_image(&ImageInfo {
            debug_name: "[MESHI DEFERRED] Normal Framebuffer",
            ..default_framebuffer_info
        });

        let diffuse = self.graph.make_image(&ImageInfo {
            debug_name: "[MESHI DEFERRED] Diffuse Framebuffer",
            ..default_framebuffer_info
        });


        let mut deferred_pass_attachments: [Option<ImageView>; 8] = [None; 8];
        deferred_pass_attachments[0] = Some(position);
        deferred_pass_attachments[1] = Some(normal);
        deferred_pass_attachments[2] = Some(diffuse);

        let mut deferred_pass_clear: [Option<ClearValue>; 8] = [None; 8];
        deferred_pass_clear[..3].fill(Some(ClearValue::Color([0.0, 0.0, 0.0, 0.0])));

        let cmds = self.scene.cull();
        self.graph.add_subpass(
            &SubpassInfo {
                viewport: self.viewport,
                color_attachments: deferred_pass_attachments,
                depth_attachment: None,
                clear_values: deferred_pass_clear,
                depth_clear: None,
            },
            |cmd| {
                return cmd;
            },
        );


        self.graph.execute();
    }
}
