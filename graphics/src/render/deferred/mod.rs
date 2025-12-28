use std::{collections::HashMap, ptr::NonNull};

use crate::{RenderObject, RenderObjectInfo, render::scene::*};
use bento::builder::{GraphicsPipelineBuilder, PSO};
use dashi::*;
use dashi::driver::command::BlitImage;
use furikake::{
    BindlessState,
    reservations::ReservedBinding,
    types::Material,
    types::*,
};
use meshi_ffi_structs::*;
use meshi_utils::MeshiError;
use noren::{DB, meta::HostMaterial};
use tare::transient::TransientAllocator;
use tracing::info;
use utils::gpupool::GPUPool;

use super::scene::GPUScene;
use tare::graph::*;

//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////

#[repr(u32)]
pub enum PassMask {
    MAIN_COLOR = 0x00000001,
}

pub struct DeferredRendererInfo {
    pub headless: bool,
    pub initial_viewport: Viewport,
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
    scene: GPUScene,
    pipelines: HashMap<Handle<Material>, PSO>,
    dynamic: DynamicAllocator,
    alloc: Box<TransientAllocator>,
    graph: RenderGraph,
    view_draws: Vec<SceneViewDraw>,
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
                    mask: PassMask::MAIN_COLOR as u32,
                }],
                ..Default::default()
            },
            state.as_mut(),
        );

        let mut alloc = Box::new(TransientAllocator::new(ctx.as_mut()));
        
        let dynamic = ctx.make_dynamic_allocator(&DynamicAllocatorInfo {
            ..Default::default()
        }).expect("Unable to create dynamic allocator!");

        let graph = RenderGraph::new_with_transient_allocator(&mut ctx, &mut alloc);

        Self {
            ctx,
            state,
            scene,
            alloc,
            graph,
            db: None,
            dynamic,
            pipelines: Default::default(),
            viewport: info.initial_viewport,
            view_draws: Vec::new(),
        }
    }

    pub fn context(&mut self) -> &mut Context {
        &mut self.ctx
    }

    pub fn state(&mut self) -> &mut BindlessState {
        &mut self.state
    }

    fn build_pipeline(&mut self, mat: &HostMaterial) -> PSO {
        let ctx: *mut Context = self.ctx.as_mut();

        let mut defines = Vec::new();

        if mat.material.render_mask & PassMask::MAIN_COLOR as u16 > 0 {
            defines.push("-DLMAO".to_string());
        }

        let shaders = miso::stddeferred(&defines);

        let mut state = GraphicsPipelineBuilder::new()
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .add_variable("per_object_ssbo", ShaderResource::DynamicStorage(self.dynamic.state()));

        {
            let ReservedBinding::TableBinding {
                binding: _,
                resources,
            } = self
                .state
                .binding("meshi_bindless_cameras")
                .unwrap()
                .binding();
            state = state.add_table_variable_with_resources("meshi_bindless_cameras", resources);
        }

        {
            let ReservedBinding::TableBinding {
                binding: _,
                resources,
            } = self
                .state
                .binding("meshi_bindless_textures")
                .unwrap()
                .binding();
            state = state.add_table_variable_with_resources("meshi_bindless_textures", resources);
        }

        {
            let ReservedBinding::TableBinding {
                binding: _,
                resources,
            } = self
                .state
                .binding("meshi_bindless_materials")
                .unwrap()
                .binding();
            state = state.add_table_variable_with_resources("meshi_bindless_materials", resources);
        }

        {
            let ReservedBinding::TableBinding {
                binding: _,
                resources,
            } = self
                .state
                .binding("meshi_bindless_transformations")
                .unwrap()
                .binding();
            state = state.add_table_variable_with_resources("meshi_bindless_transformations", resources);
        }

        state
            .build(unsafe { &mut (*ctx) })
            .expect("Failed to build material!")
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        db.import_dashi_context(&mut self.ctx);
        db.import_furikake_state(&mut self.state);

        let materials = db.enumerate_materials();

        for name in materials {
            let (mat, handle) = db.fetch_host_material(&name).unwrap();
            info!("[MESHI/GFX] Creating pipelines for material {}.", name);
            let p = self.build_pipeline(&mat);
            self.pipelines.insert(handle.unwrap(), p);
        }

        self.db = Some(NonNull::new(db).expect("lmao"));
    }

    pub fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError> {
        let h = self.scene.register_object(&SceneObjectInfo {
            local: Default::default(),
            global: Default::default(),
            scene_mask: PassMask::MAIN_COLOR as u32,
        });

        if let RenderObjectInfo::Model(m) = info {}
        todo!()
    }

    pub fn release_object(&mut self, handle: Handle<RenderObject>) {
        todo!()
    }

    pub fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
        //  self.scene.set_object_transform(handle, transform);
    }

    pub fn set_active_cameras(&mut self, cameras: &[Handle<Camera>]) {
        self.scene.set_active_cameras(cameras);
    }

    pub fn update(
        &mut self,
        _delta_time: f32,
        view_outputs: &HashMap<Handle<Camera>, Vec<ImageView>>,
        submit_info: &SubmitInfo,
    ) {
        let default_framebuffer_info = ImageInfo {
            debug_name: "",
            dim: [self.viewport.area.x as u32, self.viewport.area.y as u32, 1],
            layers: 1,
            format: Format::RGBA8,
            mip_levels: 1,
            samples: SampleCount::S1,
            initial_data: None,
        };

        let cmds = self.scene.cull();
        let mut readback = CommandStream::new().begin();
        readback.combine(cmds);
        self.scene
            .output_bins_mut()
            .sync_down(&mut readback)
            .expect("download culled bins");
        readback.prepare_buffer(
            self.scene.output_bin_counts().device().handle,
            UsageBits::COPY_SRC,
        );
        readback.combine(self.scene.output_bin_counts_mut().sync_down());

        let readback = readback.end();
        let ctx_ptr = self.ctx.as_mut() as *mut Context;
        let mut queue = self
            .ctx
            .pool_mut(QueueType::Graphics)
            .begin(ctx_ptr, "deferred_scene_cull", false)
            .expect("begin compute queue");
        let (_, fence) = readback.submit(
            &mut queue,
            &SubmitInfo2 {
                ..Default::default()
            },
        );
        if let Some(fence) = fence {
            self.ctx.wait(fence).expect("wait for cull");
        }

        self.view_draws.clear();
        let bin_count = self.scene.bin_count();
        let max_objects = self.scene.max_objects_per_bin();
        let bin_counts = self.scene.output_bin_counts().as_slice::<u32>();

        for (view_index, camera) in self.scene.active_cameras().iter().copied().enumerate() {
            let mut draws = Vec::new();
            for bin in 0..bin_count {
                let count_index = view_index * bin_count + bin;
                let count = bin_counts[count_index] as usize;
                for idx in 0..count {
                    let slot = count_index * max_objects + idx;
                    let culled = self
                        .scene
                        .output_bins()
                        .get_ref::<CulledObject>(Handle::new(slot as u16, 0))
                        .expect("culled object");
                    draws.push(PerDrawData {
                        transform: culled.transformation,
                        object_id: culled.object_id,
                        bin_id: culled.bin_id,
                    });
                }
            }
            self.view_draws.push(SceneViewDraw { camera, draws });
        }

        for view in &self.view_draws {
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

            let outputs = view_outputs
                .get(&view.camera)
                .cloned()
                .unwrap_or_default();
            let color_image = diffuse.img;

            self.graph.add_subpass(
                &SubpassInfo {
                    viewport: self.viewport,
                    color_attachments: deferred_pass_attachments,
                    depth_attachment: None,
                    clear_values: deferred_pass_clear,
                    depth_clear: None,
                },
                move |mut cmd| {
                    for output in outputs.iter() {
                        cmd.blit_images(&BlitImage {
                            src: color_image,
                            dst: output.img,
                            filter: Filter::Nearest,
                            ..Default::default()
                        });
                    }
                    cmd
                },
            );
        }

        self.graph.execute_with(submit_info);
    }
}

#[derive(Clone, Copy)]
struct PerDrawData {
    transform: Handle<Transformation>,
    object_id: u32,
    bin_id: u32,
}

struct SceneViewDraw {
    camera: Handle<Camera>,
    draws: Vec<PerDrawData>,
}
