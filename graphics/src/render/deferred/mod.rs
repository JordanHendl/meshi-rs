use std::{collections::HashMap, ptr::NonNull};

use super::environment::{EnvironmentRenderer, EnvironmentRendererInfo};
use super::scene::GPUScene;
use super::skinning::{
    MAX_SKINNING_DISPATCHES, SkinningDispatch, SkinningDispatcher, SkinnedModelData, SkinningInfo,
    unregister_skinned_model,
};
use super::{Renderer, RendererInfo, ViewOutput};
use crate::AnimationState;
use crate::{BillboardInfo, RenderObject, RenderObjectInfo, render::scene::*};
use bento::builder::{AttachmentDesc, PSO, PSOBuilder};
use dashi::*;
use driver::command::{Draw, DrawIndexed};
use execution::{CommandDispatch, CommandRing};
use furikake::PSOBuilderFurikakeExt;
use furikake::{BindlessState, reservations::ReservedBinding, types::Material, types::*};
use glam::Mat4;
use meshi_utils::MeshiError;
use noren::{
    DB,
    meta::{DeviceModel, HostMaterial},
};
use resource_pool::resource_list::ResourceList;
use tare::graph::*;
use tare::transient::TransientAllocator;
use tracing::{info, warn};

//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////

#[repr(u32)]
pub enum PassMask {
    MAIN_COLOR = 0x00000001,
}

pub struct DeferredRenderer {
    ctx: Box<Context>,
    viewport: Viewport,
    sample_count: SampleCount,
    state: Box<BindlessState>,
    db: Option<NonNull<DB>>,
    scene: GPUScene,
    environment: EnvironmentRenderer,
    pipelines: HashMap<Handle<Material>, PSO>,
    objects: ResourceList<RenderObjectData>,
    scene_lookup: HashMap<u16, Handle<RenderObjectData>>,
    dynamic: DynamicAllocator,
    cull_queue: CommandRing,
    combine_pso: PSO,
    skinning: SkinningDispatcher,
    alloc: Box<TransientAllocator>,
    graph: RenderGraph,
}

struct RenderObjectData {
    kind: RenderObjectKind,
    scene_handle: Handle<SceneObject>,
}

enum RenderObjectKind {
    Model(DeviceModel),
    SkinnedModel(SkinnedModelData),
    Billboard(BillboardInfo),
}

struct ViewDrawItem {
    kind: ViewDrawKind,
    transformation: Handle<Transformation>,
    total_transform: Mat4,
}

enum ViewDrawKind {
    Model(DeviceModel),
    SkinnedModel(SkinnedModelData),
    Billboard(BillboardInfo),
}

fn to_handle(h: Handle<RenderObjectData>) -> Handle<RenderObject> {
    return Handle::new(h.slot, h.generation);
}

fn from_handle(h: Handle<RenderObject>) -> Handle<RenderObjectData> {
    return Handle::new(h.slot, h.generation);
}

impl DeferredRenderer {
    pub fn new(info: &RendererInfo) -> Self {
        let device = DeviceSelector::new()
            .unwrap()
            .select(DeviceFilter::default().add_required_type(DeviceType::Dedicated))
            .unwrap();
        let mut ctx = if info.headless {
            Box::new(
                Context::headless(&ContextInfo {
                    device,
                    ..Default::default()
                })
                .expect(""),
            )
        } else {
            Box::new(
                Context::new(&ContextInfo {
                    device,
                    ..Default::default()
                })
                .expect(""),
            )
        };

        CommandDispatch::init(ctx.as_mut()).expect("Failed to init command dispatcher!");
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

        let dynamic = ctx
            .make_dynamic_allocator(&DynamicAllocatorInfo {
                ..Default::default()
            })
            .expect("Unable to create dynamic allocator!");

        let environment = EnvironmentRenderer::new(
            ctx.as_mut(),
            state.as_mut(),
            EnvironmentRendererInfo {
                color_format: Format::BGRA8,
                sample_count: info.sample_count,
                use_depth: true,
                skybox: super::environment::sky::SkyboxInfo::default(),
                ocean: super::environment::ocean::OceanInfo::default(),
                terrain: super::environment::terrain::TerrainInfo::default(),
            },
        );

        let graph = RenderGraph::new_with_transient_allocator(&mut ctx, &mut alloc);

        let cull_queue = ctx
            .make_command_ring(&CommandQueueInfo2 {
                debug_name: "[CULL]",
                parent: None,
                queue_type: QueueType::Graphics,
            })
            .expect("Failed to make cull command queue");

        let skinning = SkinningDispatcher::new(ctx.as_mut(), state.as_ref());

        let shaders = miso::stddeferred_combine(&[]);
        let mut psostate = PSOBuilder::new()
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .set_attachment_format(0, Format::BGRA8)
            .set_details(GraphicsPipelineDetails {
                color_blend_states: vec![Default::default(); 1],
                sample_count: info.sample_count,
                ..Default::default()
            })
            .add_table_variable_with_resources(
                "per_obj_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::DynamicStorage(dynamic.state()),
                    slot: 0,
                }],
            );

        psostate = psostate
            .add_reserved_table_variables(state.as_mut())
            .unwrap();

        let pso = psostate
            .build(ctx.as_mut())
            .expect("Failed to make deferred combine pso!");

        state.register_pso_tables(&pso);
        info!(
            "Initialized Deferred Renderer with dimensions [{}, {}]",
            info.initial_viewport.area.w, info.initial_viewport.area.h
        );
        Self {
            ctx,
            combine_pso: pso,
            state,
            scene,
            graph,
            db: None,
            environment,
            dynamic,
            pipelines: Default::default(),
            objects: Default::default(),
            scene_lookup: Default::default(),
            viewport: info.initial_viewport,
            sample_count: info.sample_count,
            cull_queue,
            skinning,
            alloc,
        }
    }

    pub fn alloc(&mut self) -> &mut TransientAllocator {
        &mut self.alloc
    }

    fn build_pipeline(&mut self, mat: &HostMaterial) -> PSO {
        let ctx: *mut Context = self.ctx.as_mut();

        let mut defines = Vec::new();

        if mat.material.render_mask & PassMask::MAIN_COLOR as u16 > 0 {
            defines.push("-DLMAO".to_string());
        }

        let shaders = miso::stddeferred(&defines);

        let mut state = PSOBuilder::new()
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .add_table_variable_with_resources(
                "per_obj_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::DynamicStorage(self.dynamic.state()),
                    slot: 0,
                }],
            );

        state = state
            .add_reserved_table_variables(self.state.as_mut())
            .unwrap();

        state = state.add_depth_target(AttachmentDesc {
            format: Format::D24S8,
            samples: self.sample_count,
        });

        let s = state
            .set_details(GraphicsPipelineDetails {
                color_blend_states: vec![Default::default(); 4],
                sample_count: self.sample_count,
                depth_test: Some(DepthInfo {
                    should_test: true,
                    should_write: true,
                }),
                ..Default::default()
            })
            .build(unsafe { &mut (*ctx) })
            .expect("Failed to build material!");

        assert!(s.bind_table[0].is_some());
        assert!(s.bind_table[1].is_some());

        self.state.register_pso_tables(&s);
        s
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        db.import_dashi_context(&mut self.ctx);
        db.import_furikake_state(&mut self.state);
        self.alloc.set_bindless_registry(self.state.as_mut());

        let materials = db.enumerate_materials();

        for name in materials {
            let (mat, handle) = db.fetch_host_material(&name).unwrap();
            let p = self.build_pipeline(&mat);
            info!(
                "[MESHI/GFX] Creating pipelines for material {} (Handle => {}).",
                name,
                handle.as_ref().unwrap().slot
            );
            self.pipelines.insert(handle.unwrap(), p);
        }

        self.db = Some(NonNull::new(db).expect("lmao"));
    }

    pub fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError> {
        let scene_handle = self.scene.register_object(&SceneObjectInfo {
            local: Default::default(),
            global: Default::default(),
            scene_mask: PassMask::MAIN_COLOR as u32,
        });

        match info {
            RenderObjectInfo::Model(m) => {
                let h = self.objects.push(RenderObjectData {
                    kind: RenderObjectKind::Model(m.clone()),
                    scene_handle,
                });

                self.scene_lookup.insert(scene_handle.slot, h);

                Ok(to_handle(h))
            }
            RenderObjectInfo::SkinnedModel(skinned) => {
                let skinned_data = SkinnedModelData::new(skinned.clone(), self.state.as_mut());
                let h = self.objects.push(RenderObjectData {
                    kind: RenderObjectKind::SkinnedModel(skinned_data),
                    scene_handle,
                });

                self.scene_lookup.insert(scene_handle.slot, h);

                Ok(to_handle(h))
            }
            RenderObjectInfo::Billboard(billboard) => {
                let h = self.objects.push(RenderObjectData {
                    kind: RenderObjectKind::Billboard(billboard.clone()),
                    scene_handle,
                });

                self.scene_lookup.insert(scene_handle.slot, h);

                Ok(to_handle(h))
            }
            RenderObjectInfo::Empty => todo!(), //Err(MeshiError::ResourceUnavailable),
        }
    }

    pub fn set_skinned_animation_state(
        &mut self,
        handle: Handle<RenderObject>,
        state: AnimationState,
    ) {
        if !handle.valid() {
            warn!("Attempted to update animation on invalid handle.");
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            warn!("Failed to update animation for object {}", handle.slot);
            return;
        }

        let obj = self.objects.get_ref_mut(from_handle(handle));

        match &mut obj.kind {
            RenderObjectKind::SkinnedModel(skinned) => {
                skinned.info.animation = state;
                skinned.mark_animation_dirty();
            }
            _ => {
                warn!("Attempted to update animation on non-skinned object.");
            }
        }
    }

    pub fn set_billboard_texture(&mut self, handle: Handle<RenderObject>, texture_id: u32) {
        if !handle.valid() {
            warn!("Attempted to update billboard texture on invalid handle.");
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            warn!(
                "Failed to update billboard texture for object {}",
                handle.slot
            );
            return;
        }

        let obj = self.objects.get_ref_mut(from_handle(handle));
        match &mut obj.kind {
            RenderObjectKind::Billboard(billboard) => {
                billboard.texture_id = texture_id;
            }
            _ => {
                warn!("Attempted to update billboard texture on non-billboard object.");
            }
        }
    }

    pub fn set_billboard_material(
        &mut self,
        handle: Handle<RenderObject>,
        material: Option<Handle<Material>>,
    ) {
        if !handle.valid() {
            warn!("Attempted to update billboard material on invalid handle.");
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            warn!(
                "Failed to update billboard material for object {}",
                handle.slot
            );
            return;
        }

        let obj = self.objects.get_ref_mut(from_handle(handle));
        match &mut obj.kind {
            RenderObjectKind::Billboard(billboard) => {
                billboard.material = material;
            }
            _ => {
                warn!("Attempted to update billboard material on non-billboard object.");
            }
        }
    }

    pub fn release_object(&mut self, handle: Handle<RenderObject>) {
        if !handle.valid() {
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            return;
        }

        let obj = self.objects.get_ref(from_handle(handle));
        if let RenderObjectKind::SkinnedModel(skinned) = &obj.kind {
            unregister_skinned_model(self.state.as_mut(), skinned);
        }
        self.scene.release_object(obj.scene_handle);
        self.scene_lookup.remove(&obj.scene_handle.slot);

        self.objects.release(from_handle(handle));
    }

    pub fn object_transform(&self, handle: Handle<RenderObject>) -> glam::Mat4 {
        if !handle.valid() {
            return Default::default();
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            return Default::default();
        }

        let obj = self.objects.get_ref(from_handle(handle));
        self.scene.get_object_transform(obj.scene_handle)
    }

    pub fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
        if !handle.valid() {
            warn!("Attempted to update transformation of invalid handle.");
            return;
        }

        if !self.objects.entries.iter().any(|h| h.slot == handle.slot) {
            warn!("Failed to update transform for object {}", handle.slot);
            return;
        }

        let obj = self.objects.get_ref(from_handle(handle));
        self.scene.set_object_transform(obj.scene_handle, transform);
    }

    fn pull_scene(&mut self) {
        self.cull_queue
            .record(|c| {
                let state_update = self
                    .state
                    .update()
                    .expect("Failed to update furikake state");

                let cull_cmds = state_update.combine(self.scene.cull_and_sync());
                cull_cmds.append(c).unwrap();
            })
            .expect("Failed to make commands");

        self.cull_queue
            .submit(&Default::default())
            .expect("Failed to submit!");
        self.cull_queue.wait_all().unwrap();
    }

    fn collect_draws(&mut self, views: &[Handle<Camera>]) -> Vec<Vec<ViewDrawItem>> {
        let num_bins = self.scene.num_bins();
        let max_objects = self.scene.max_objects_per_bin() as usize;
        let bin_counts = self.scene.bin_counts();
        let mut view_draws: Vec<Vec<ViewDrawItem>> = (0..views.len()).map(|_| Vec::new()).collect();

        for (view_idx, _) in views.iter().enumerate() {
            for bin in 0..num_bins {
                let bin_offset = view_idx * num_bins + bin;
                if bin_offset >= bin_counts.len() {
                    continue;
                }

                let count = bin_counts[bin_offset] as usize;
                for draw_idx in 0..count {
                    let slot = bin_offset * max_objects + draw_idx;
                    if let Some(culled) = self.scene.culled_object(slot as u32) {
                        if let Some(obj_handle) = self.scene_lookup.get(&(culled.object_id as u16))
                        {
                            let obj = self.objects.get_ref(*obj_handle);
                            let kind = match &obj.kind {
                                RenderObjectKind::Model(model) => {
                                    ViewDrawKind::Model(model.clone())
                                }
                                RenderObjectKind::SkinnedModel(skinned) => {
                                    ViewDrawKind::SkinnedModel(skinned.clone())
                                }
                                RenderObjectKind::Billboard(billboard) => {
                                    ViewDrawKind::Billboard(billboard.clone())
                                }
                            };
                            view_draws[view_idx].push(ViewDrawItem {
                                kind,
                                transformation: GPUScene::unpack_handle(culled.transformation),
                                total_transform: culled.total_transform,
                            });
                        }
                    }
                }
            }
        }
        view_draws
    }

    fn update_skinned_animations(&mut self, delta_time: f32) {
        let entries = self.objects.entries.clone();
        let mut dispatches = Vec::new();
        for entry in entries {
            let obj = self.objects.get_ref_mut(entry);
            if let RenderObjectKind::SkinnedModel(skinned) = &mut obj.kind {
                if dispatches.len() >= MAX_SKINNING_DISPATCHES {
                    break;
                }
                dispatches.push(SkinningDispatch::from_model(skinned, delta_time));
                skinned.clear_animation_dirty();
            }
        }

        self.skinning.update(&dispatches);
    }

    pub fn update(
        &mut self,
        sems: &[Handle<Semaphore>],
        views: &[Handle<Camera>],
        delta_time: f32,
    ) -> Vec<ViewOutput> {
        if views.is_empty() {
            return Vec::new();
        }
        if self.cull_queue.current_index() == 0 {
            self.dynamic.reset();
            self.environment.reset();
        }

        self.update_skinned_animations(delta_time);

        // Set active scene cameras..
        self.scene.set_active_cameras(views);
        // Pull scene GPU --> CPU to read.
        self.pull_scene();

        // Manually collect all draws per view.
        let view_draws = self.collect_draws(views);

        // Default framebuffer info.
        let default_framebuffer_info = ImageInfo {
            debug_name: "",
            dim: [self.viewport.area.w as u32, self.viewport.area.h as u32, 1],
            layers: 1,
            format: Format::RGBA8,
            mip_levels: 1,
            samples: self.sample_count,
            initial_data: None,
            ..Default::default()
        };

        let semaphores = self.graph.make_semaphores(1);
        let mut outputs = Vec::with_capacity(views.len());
        let mut depth = self.graph.make_image(&ImageInfo {
            debug_name: &format!("[MESHI DEFERRED] Depth buffer"),
            format: Format::D24S8,
            ..default_framebuffer_info
        });

        depth.view.aspect = AspectMask::Depth;

        for (view_idx, camera) in views.iter().enumerate() {
            let position = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI DEFERRED] Position Framebuffer View {view_idx}"),
                ..default_framebuffer_info
            });

            let normal = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI DEFERRED] Normal Framebuffer View {view_idx}"),
                ..default_framebuffer_info
            });

            let diffuse = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI DEFERRED] Diffuse Framebuffer View {view_idx}"),
                ..default_framebuffer_info
            });

            let material_code = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI DEFERRED] Material Code Framebuffer View {view_idx}"),
                ..default_framebuffer_info
            });

            let final_combine = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI DEFERRED] Combined Framebuffer View {view_idx}"),
                format: Format::BGRA8,
                samples: self.sample_count,
                ..default_framebuffer_info
            });

            let mut deferred_pass_attachments: [Option<ImageView>; 8] = [None; 8];
            deferred_pass_attachments[0] = Some(position.view);
            deferred_pass_attachments[1] = Some(diffuse.view);
            deferred_pass_attachments[2] = Some(normal.view);
            deferred_pass_attachments[3] = Some(material_code.view);

            let mut deferred_pass_clear: [Option<ClearValue>; 8] = [None; 8];
            deferred_pass_clear[..4].fill(Some(ClearValue::Color([0.0, 0.0, 0.0, 0.0])));

            let mut deferred_combine_attachments: [Option<ImageView>; 8] = [None; 8];
            deferred_combine_attachments[0] = Some(final_combine.view);
            let mut deferred_combine_clear: [Option<ClearValue>; 8] = [None; 8];
            deferred_combine_clear[0] = Some(ClearValue::Color([0.0, 0.0, 0.0, 0.0]));

            let draw_items = &view_draws[view_idx];
            let camera_handle = *camera;

            // Deferred SPLIT pass. Renders the following framebuffers:
            // 1) Position
            // 2) Albedo (or diffuse)
            // 3) Normal
            // 4) Material Code
            self.graph.add_subpass(
                &SubpassInfo {
                    viewport: self.viewport,
                    color_attachments: deferred_pass_attachments,
                    depth_attachment: Some(depth.view),
                    clear_values: deferred_pass_clear,
                    depth_clear: Some(ClearValue::DepthStencil {
                        depth: 1.0,
                        stencil: 0,
                    }),
                },
                |mut cmd| {
                    struct ModelDraw<'a> {
                        item: &'a ViewDrawItem,
                        model: &'a DeviceModel,
                        skinning: SkinningInfo,
                    }

                    let mut model_draws = Vec::new();
                    let mut billboard_draws = Vec::new();

                    for item in draw_items {
                        match &item.kind {
                            ViewDrawKind::Model(model) => {
                                model_draws.push(ModelDraw {
                                    item,
                                    model,
                                    skinning: SkinningInfo::default(),
                                });
                            }
                            ViewDrawKind::SkinnedModel(skinned) => {
                                model_draws.push(ModelDraw {
                                    item,
                                    model: skinned.model(),
                                    skinning: skinned.skinning_info(),
                                });
                            }
                            ViewDrawKind::Billboard(billboard) => {
                                billboard_draws.push((item, billboard));
                            }
                        }
                    }

                    for draw in model_draws {
                        for mesh in &draw.model.meshes {
                            if let Some(material) = &mesh.material {
                                if let Some(mat_idx) = material.furikake_material_handle {
                                    if let Some(pso) = self.pipelines.get(&mat_idx) {
                                        assert!(pso.handle.valid());

                                        let mut alloc = self
                                            .dynamic
                                            .bump()
                                            .expect("Failed to allocate dynamic buffer!");

                                        // Per Object dynamic structure.
                                        #[repr(C)]
                                        #[derive(Default)]
                                        struct PerObj {
                                            transform: Mat4, // Backup transform
                                            transformation: Handle<Transformation>,
                                            material_id: Handle<Material>,
                                            camera: Handle<Camera>,
                                            skeleton_id: Handle<furikake::types::SkeletonHeader>,
                                            animation_state_id:
                                                Handle<furikake::types::AnimationState>,
                                        }

                                        let per_obj = &mut alloc.slice::<PerObj>()[0];
                                        *per_obj = Default::default();
                                        per_obj.transform = draw.item.total_transform;
                                        per_obj.transformation = draw.item.transformation;
                                        per_obj.material_id = mat_idx;
                                        per_obj.camera = camera_handle;
                                        per_obj.skeleton_id = draw.skinning.skeleton;
                                        per_obj.animation_state_id = draw.skinning.animation_state;

                                        cmd = cmd
                                            .bind_graphics_pipeline(pso.handle)
                                            .update_viewport(&self.viewport)
                                            .draw_indexed(&DrawIndexed {
                                                vertices: mesh
                                                    .geometry
                                                    .base
                                                    .vertices
                                                    .handle()
                                                    .unwrap(),
                                                indices: mesh
                                                    .geometry
                                                    .base
                                                    .indices
                                                    .handle()
                                                    .unwrap(),
                                                index_count: mesh
                                                    .geometry
                                                    .base
                                                    .index_count
                                                    .unwrap(),
                                                bind_tables: pso.tables(),
                                                dynamic_buffers: [None, Some(alloc), None, None],
                                                ..Default::default()
                                            })
                                            .unbind_graphics_pipeline();
                                    }
                                }
                            }
                        }
                    }

                    if !billboard_draws.is_empty() {
                        warn!(
                            "Deferred renderer received {} billboard draw(s) without a billboard pass.",
                            billboard_draws.len()
                        );
                    }

                    cmd
                },
            );

            self.graph.add_subpass(
                &SubpassInfo {
                    viewport: self.viewport,
                    color_attachments: deferred_combine_attachments,
                    depth_attachment: None,
                    clear_values: deferred_combine_clear,
                    depth_clear: None,
                },
                |mut cmd| {
                    let mut alloc = self
                        .dynamic
                        .bump()
                        .expect("Failed to allocate dynamic buffer!");

                    #[repr(C)]
                    struct PerObj {
                        pos: u32,
                        diff: u32,
                        norm: u32,
                        mat: u32,
                    }

                    let per_obj = &mut alloc.slice::<PerObj>()[0];
                    per_obj.pos = position.bindless_id.unwrap() as u32;
                    per_obj.diff = diffuse.bindless_id.unwrap() as u32;
                    per_obj.norm = normal.bindless_id.unwrap() as u32;
                    per_obj.mat = material_code.bindless_id.unwrap() as u32;

                    cmd = cmd
                        .bind_graphics_pipeline(self.combine_pso.handle)
                        .update_viewport(&self.viewport)
                        .draw(&Draw {
                            bind_tables: self.combine_pso.tables(),
                            dynamic_buffers: [None, Some(alloc), None, None],
                            instance_count: 1,
                            count: 3,
                            ..Default::default()
                        })
                        .unbind_graphics_pipeline();

                    cmd
                },
            );

            self.environment.render(
                &mut self.graph,
                &self.viewport,
                final_combine.view,
                Some(depth.view),
                camera_handle,
                delta_time,
            );

            outputs.push(ViewOutput {
                camera: *camera,
                image: final_combine.view,
                semaphore: semaphores[0],
            });
        }

        self.graph.execute_with(&SubmitInfo {
            wait_sems: sems,
            signal_sems: &[semaphores[0]],
        });

        outputs
    }

    pub fn shut_down(self) {
        self.ctx.destroy();
    }
}

impl Renderer for DeferredRenderer {
    fn context(&mut self) -> &'static mut Context {
        unsafe { &mut (*(self.ctx.as_mut() as *mut Context)) }
    }

    fn state(&mut self) -> &mut BindlessState {
        &mut self.state
    }

    fn initialize_database(&mut self, db: &mut DB) {
        DeferredRenderer::initialize_database(self, db);
    }

    fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError> {
        DeferredRenderer::register_object(self, info)
    }

    fn set_skinned_animation_state(&mut self, handle: Handle<RenderObject>, state: AnimationState) {
        DeferredRenderer::set_skinned_animation_state(self, handle, state);
    }

    fn set_billboard_texture(&mut self, handle: Handle<RenderObject>, texture_id: u32) {
        DeferredRenderer::set_billboard_texture(self, handle, texture_id);
    }

    fn set_billboard_material(
        &mut self,
        handle: Handle<RenderObject>,
        material: Option<Handle<Material>>,
    ) {
        DeferredRenderer::set_billboard_material(self, handle, material);
    }

    fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
        DeferredRenderer::set_object_transform(self, handle, transform);
    }

    fn object_transform(&self, handle: Handle<RenderObject>) -> glam::Mat4 {
        DeferredRenderer::object_transform(self, handle)
    }

    fn update(
        &mut self,
        sems: &[Handle<Semaphore>],
        views: &[Handle<Camera>],
        delta_time: f32,
    ) -> Vec<ViewOutput> {
        DeferredRenderer::update(self, sems, views, delta_time)
    }

    fn shut_down(self: Box<Self>) {
        self.ctx.destroy();
    }
}
