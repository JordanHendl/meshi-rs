use super::environment::{EnvironmentRenderer, EnvironmentRendererInfo};
use super::scene::GPUScene;
use super::{Renderer, RendererInfo, ViewOutput};
use crate::{RenderObject, RenderObjectInfo, render::scene::*};
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
use std::{collections::HashMap, ptr::NonNull};
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

pub struct ForwardRenderer {
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
    alloc: Box<TransientAllocator>,
    graph: RenderGraph,
}

struct RenderObjectData {
    model: DeviceModel,
    scene_handle: Handle<SceneObject>,
}

struct ViewDrawItem {
    model: DeviceModel,
    transformation: Handle<Transformation>,
    total_transform: Mat4,
}

fn to_handle(h: Handle<RenderObjectData>) -> Handle<RenderObject> {
    return Handle::new(h.slot, h.generation);
}

fn from_handle(h: Handle<RenderObject>) -> Handle<RenderObjectData> {
    return Handle::new(h.slot, h.generation);
}

impl ForwardRenderer {
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
                name: "[MESHI] Forward Renderer Scene",
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

        info!(
            "Initialized Forward Renderer with dimensions [{}, {}]",
            info.initial_viewport.area.w, info.initial_viewport.area.h
        );
        Self {
            ctx,
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

        let shaders = miso::stdforward(&defines);

        let mut state = PSOBuilder::new()
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .set_attachment_format(0, Format::BGRA8)
            .add_table_variable_with_resources(
                "per_obj_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::DynamicStorage(self.dynamic.state()),
                    slot: 0,
                }],
            );

        state = state
            .add_reserved_table_variable(self.state.as_mut(), "meshi_bindless_cameras")
            .unwrap();
        state = state
            .add_reserved_table_variable(self.state.as_mut(), "meshi_bindless_lights")
            .unwrap();
        state = state
            .add_reserved_table_variable(self.state.as_mut(), "meshi_bindless_textures")
            .unwrap();
        state = state
            .add_reserved_table_variable(self.state.as_mut(), "meshi_bindless_samplers")
            .unwrap();
        state = state
            .add_reserved_table_variable(self.state.as_mut(), "meshi_bindless_materials")
            .unwrap();
        state = state
            .add_reserved_table_variable(self.state.as_mut(), "meshi_bindless_transformations")
            .unwrap();

        state = state.add_depth_target(AttachmentDesc {
            format: Format::D24S8,
            samples: self.sample_count,
        });

        let s = state
            .set_details(GraphicsPipelineDetails {
                color_blend_states: vec![Default::default(); 1],
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
                    model: m.clone(),
                    scene_handle,
                });

                self.scene_lookup.insert(scene_handle.slot, h);

                Ok(to_handle(h))
            }
            RenderObjectInfo::Empty => todo!(), //Err(MeshiError::ResourceUnavailable),
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
                            view_draws[view_idx].push(ViewDrawItem {
                                model: obj.model.clone(),
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
            format: Format::BGRA8,
            mip_levels: 1,
            samples: self.sample_count,
            initial_data: None,
            ..Default::default()
        };

        let semaphores = self.graph.make_semaphores(1);
        let mut outputs = Vec::with_capacity(views.len());
        let mut depth = self.graph.make_image(&ImageInfo {
            debug_name: &format!("[MESHI FORWARD] Depth buffer"),
            format: Format::D24S8,
            ..default_framebuffer_info
        });

        depth.view.aspect = AspectMask::Depth;

        for (view_idx, camera) in views.iter().enumerate() {
            let color = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI FORWARD] Position Framebuffer View {view_idx}"),
                ..default_framebuffer_info
            });

            let mut forward_pass_attachments: [Option<ImageView>; 8] = [None; 8];
            forward_pass_attachments[0] = Some(color.view);

            let mut forward_pass_clear: [Option<ClearValue>; 8] = [None; 8];
            forward_pass_clear[0] = Some(ClearValue::Color([0.0, 0.0, 0.0, 0.0]));

            let draw_items = &view_draws[view_idx];
            let camera_handle = *camera;

            // Forward SPLIT pass. Renders the following framebuffers:
            // 1) Color
            self.graph.add_subpass(
                &SubpassInfo {
                    viewport: self.viewport,
                    color_attachments: forward_pass_attachments,
                    depth_attachment: Some(depth.view),
                    clear_values: forward_pass_clear,
                    depth_clear: Some(ClearValue::DepthStencil {
                        depth: 1.0,
                        stencil: 0,
                    }),
                },
                |mut cmd| {
                    for item in draw_items {
                        for mesh in &item.model.meshes {
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
                                        struct PerObj {
                                            transform: Mat4, // Backup transform
                                            transformation: Handle<Transformation>,
                                            material_id: Handle<Material>,
                                            camera: Handle<Camera>,
                                        }

                                        let per_obj = &mut alloc.slice::<PerObj>()[0];
                                        per_obj.transform = item.total_transform;
                                        per_obj.transformation = item.transformation;
                                        per_obj.material_id = mat_idx;
                                        per_obj.camera = camera_handle;
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

                    cmd
                },
            );

            self.environment.render(
                &mut self.graph,
                &self.viewport,
                color.view,
                Some(depth.view),
                camera_handle,
                delta_time,
            );

            outputs.push(ViewOutput {
                camera: *camera,
                image: color.view,
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

impl Renderer for ForwardRenderer {
    fn context(&mut self) -> &'static mut Context {
        unsafe { &mut (*(self.ctx.as_mut() as *mut Context)) }
    }

    fn state(&mut self) -> &mut BindlessState {
        &mut self.state
    }

    fn initialize_database(&mut self, db: &mut DB) {
        ForwardRenderer::initialize_database(self, db);
    }

    fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError> {
        ForwardRenderer::register_object(self, info)
    }

    fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
        ForwardRenderer::set_object_transform(self, handle, transform);
    }

    fn object_transform(&self, handle: Handle<RenderObject>) -> glam::Mat4 {
        ForwardRenderer::object_transform(self, handle)
    }

    fn update(
        &mut self,
        sems: &[Handle<Semaphore>],
        views: &[Handle<Camera>],
        delta_time: f32,
    ) -> Vec<ViewOutput> {
        ForwardRenderer::update(self, sems, views, delta_time)
    }

    fn shut_down(self: Box<Self>) {
        self.ctx.destroy();
    }
}
