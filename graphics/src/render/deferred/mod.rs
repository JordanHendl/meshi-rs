use std::{collections::HashMap, ptr::NonNull};

use super::scene::GPUScene;
use crate::{RenderObject, RenderObjectInfo, render::scene::*};
use bento::builder::{GraphicsPipelineBuilder, PSO};
use dashi::*;
use driver::command::DrawIndexed;
use execution::{CommandDispatch, CommandRing};
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

pub struct DeferredViewOutput {
    pub camera: Handle<Camera>,
    pub image: ImageView,
    pub semaphore: Handle<Semaphore>,
}

fn to_handle(h: Handle<RenderObjectData>) -> Handle<RenderObject> {
    return Handle::new(h.slot, h.generation);
}

fn from_handle(h: Handle<RenderObject>) -> Handle<RenderObjectData> {
    return Handle::new(h.slot, h.generation);
}

impl DeferredRenderer {
    pub fn new(info: &DeferredRendererInfo) -> Self {
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

        let mut state = Box::new(BindlessState::new(&mut ctx));

        CommandDispatch::init(ctx.as_mut()).expect("Failed to init command dispatcher!");
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

        let graph = RenderGraph::new_with_transient_allocator(&mut ctx, &mut alloc);

        let cull_queue = ctx
            .make_command_ring(&CommandQueueInfo2 {
                debug_name: "[CULL]",
                parent: None,
                queue_type: QueueType::Graphics,
            })
            .expect("Failed to make cull command queue");

        Self {
            ctx,
            state,
            scene,
            alloc,
            graph,
            db: None,
            dynamic,
            pipelines: Default::default(),
            objects: Default::default(),
            scene_lookup: Default::default(),
            viewport: info.initial_viewport,
            cull_queue,
        }
    }

    pub fn alloc(&mut self) -> &mut TransientAllocator {
        &mut self.alloc
    }

    pub fn context(&mut self) -> &'static mut Context {
        unsafe { &mut (*(self.ctx.as_mut() as *mut Context)) }
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
            .add_table_variable_with_resources(
                "per_obj_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::DynamicStorage(self.dynamic.state()),
                    slot: 0,
                }],
            );

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
                .binding("meshi_bindless_lights")
                .unwrap()
                .binding();
            state = state.add_table_variable_with_resources("meshi_bindless_lights", resources);
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
            state = state
                .add_table_variable_with_resources("meshi_bindless_transformations", resources);
        }

        let s = state
            .set_details(GraphicsPipelineDetails {
                color_blend_states: vec![Default::default(); 3],
                ..Default::default()
            })
            .build(unsafe { &mut (*ctx) })
            .expect("Failed to build material!");

        assert!(s.bind_table[0].is_some());
        assert!(s.bind_table[1].is_some());
        s
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        db.import_dashi_context(&mut self.ctx);
        db.import_furikake_state(&mut self.state);

        let materials = db.enumerate_materials();

        for name in materials {
            let (mat, handle) = db.fetch_host_material(&name).unwrap();
            let p = self.build_pipeline(&mat);
            info!(
                "[MESHI/GFX] Creating pipelines for material {} (Handle => {:?}.",
                name, handle
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

    pub fn update(
        &mut self,
        sems: &[Handle<Semaphore>],
        views: &[Handle<Camera>],
        _delta_time: f32,
    ) -> Vec<DeferredViewOutput> {
        if views.is_empty() {
            return Vec::new();
        }

        self.scene.set_active_cameras(views);

        self.cull_queue
            .record(|c| {
                let state_update = self
                    .state
                    .update()
                    .expect("Failed to update furikake state");

                let cull_cmds = state_update.combine(self.scene.cull_and_sync());
                cull_cmds.append(c);
            })
            .expect("Failed to make commands");

        self.cull_queue
            .submit(&Default::default())
            .expect("Failed to submit!");
        self.cull_queue.wait_all().unwrap();

        struct ViewDrawItem {
            model: DeviceModel,
            transformation: Handle<Transformation>,
            total_transform: Mat4,
        }

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
                                transformation: culled.transformation,
                                total_transform: culled.total_transform,
                            });
                        }
                    }
                }
            }
        }

        let default_framebuffer_info = ImageInfo {
            debug_name: "",
            dim: [self.viewport.area.w as u32, self.viewport.area.h as u32, 1],
            layers: 1,
            format: Format::RGBA8,
            mip_levels: 1,
            samples: SampleCount::S1,
            initial_data: None,
        };

        let semaphores = self.graph.make_semaphores(1);
        let mut outputs = Vec::with_capacity(views.len());

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

            let mut deferred_pass_attachments: [Option<ImageView>; 8] = [None; 8];
            deferred_pass_attachments[0] = Some(position);
            deferred_pass_attachments[1] = Some(normal);
            deferred_pass_attachments[2] = Some(diffuse);

            let mut deferred_pass_clear: [Option<ClearValue>; 8] = [None; 8];
            deferred_pass_clear[..3].fill(Some(ClearValue::Color([0.0, 0.0, 0.0, 0.0])));

            let draw_items = &view_draws[view_idx];
            let camera_handle = *camera;
            self.graph.add_subpass(
                &SubpassInfo {
                    viewport: self.viewport,
                    color_attachments: deferred_pass_attachments,
                    depth_attachment: None,
                    clear_values: deferred_pass_clear,
                    depth_clear: None,
                },
                |mut cmd| {
                    for item in draw_items {
                        for mesh in &item.model.meshes {
                            if let Some(material) = &mesh.material {
                                if let Some(mat_idx) = material.furikake_material_handle {
                                    if let Some(pso) = self.pipelines.get(&mat_idx) {
                                        assert!(pso.handle.valid());
                                        if self.cull_queue.current_index() == 0 {
                                            self.dynamic.reset();
                                        }

                                        let mut alloc = self
                                            .dynamic
                                            .bump()
                                            .expect("Failed to allocate dynamic buffer!");

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

            outputs.push(DeferredViewOutput {
                camera: *camera,
                image: normal,
                semaphore: semaphores[0],
            });
        }

        self.graph.execute_with(&SubmitInfo {
            wait_sems: sems,
            signal_sems: &[semaphores[0]],
        });

        outputs
    }
}
