use std::{collections::HashMap, ptr::NonNull};

use super::environment::{EnvironmentRenderer, EnvironmentRendererInfo};
use super::gpu_draw_builder::GPUDrawBuilder;
use super::scene::GPUScene;
use super::skinning::{SkinningDispatcher, SkinningHandle, SkinningInfo};
use super::text::TextRenderer;
use super::{Renderer, RendererInfo, ViewOutput};
use crate::AnimationState;
use crate::render::gpu_draw_builder::GPUDrawBuilderInfo;
use crate::{
    BillboardInfo, RenderObject, RenderObjectInfo, TextInfo, TextObject, render::scene::*,
};
use bento::builder::{AttachmentDesc, PSO, PSOBuilder};
use dashi::structs::{IndexedIndirectCommand, IndirectCommand};
use dashi::utils::gpupool::GPUPool;
use dashi::*;
use driver::command::{Draw, DrawIndexedIndirect, DrawIndirect};
use execution::{CommandDispatch, CommandRing};
use furikake::PSOBuilderFurikakeExt;
use furikake::reservations::ReservedBinding;
use furikake::types::AnimationState as FurikakeAnimationState;
use furikake::{
    BindlessState, reservations::bindless_materials::ReservedBindlessMaterials, types::Material,
    types::*,
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use meshi_utils::MeshiError;
use noren::rdb::Skeleton;
use noren::rdb::primitives::Vertex;
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

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct PerDrawData {
    scene_id: Handle<SceneObject>,
    transform_id: Handle<Transformation>,
    material_id: Handle<Material>,
    skeleton_id: Handle<SkeletonHeader>,
    animation_state_id: Handle<FurikakeAnimationState>,
    per_obj_joints_id: Handle<JointTransform>,
    vertex_id: u32,
    vertex_count: u32,
    index_id: u32,
    index_count: u32,
}

struct RendererData {
    viewport: Viewport,
    objects: ResourceList<RenderObjectData>,
    lookup: HashMap<u16, Handle<RenderObjectData>>,
    renderables: GPUPool<PerDrawData>,
    dynamic: DynamicAllocator,
}

struct DataProcessors {
    scene: GPUScene,
    skinning: SkinningDispatcher,
    draw_builder: GPUDrawBuilder,
}

struct Renderers {
    environment: EnvironmentRenderer,
}

struct DeferredPSO {
    pipelines: HashMap<Handle<Material>, PSO>,
    standard: PSO,
    combine_pso: PSO,
}

struct DeferredExecution {
    cull_queue: CommandRing,
}

pub struct DeferredRenderer {
    ctx: Box<Context>,
    data: RendererData,
    proc: DataProcessors,
    subrender: Renderers,
    psos: DeferredPSO,
    sample_count: SampleCount,
    exec: DeferredExecution,
    state: Box<BindlessState>,
    db: Option<NonNull<DB>>,
    alloc: Box<TransientAllocator>,
    graph: RenderGraph,
    text: TextRenderer,
}

struct RenderObjectData {
    kind: RenderObjectKind,
    scene_handle: Handle<SceneObject>,
    draws: Vec<Handle<PerDrawData>>,
}

enum RenderObjectKind {
    Model(DeviceModel),
    SkinnedModel(SkinnedRenderData),
    Billboard(BillboardData),
}

#[derive(Clone)]
struct SkinnedRenderData {
    model: DeviceModel,
    skinning: SkinningInfo,
    skinning_handle: SkinningHandle,
}

#[derive(Clone)]
struct BillboardData {
    info: BillboardInfo,
    vertex_buffer: Handle<Buffer>,
    owns_material: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BillboardVertex {
    center: [f32; 3],
    offset: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    tex_coords: [f32; 2],
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

        let data = RendererData {
            viewport: info.initial_viewport,
            objects: ResourceList::default(),
            lookup: Default::default(),
            renderables: GPUPool::new(
                ctx.as_mut(),
                &BufferInfo {
                    debug_name: "[MESHI] Deferred Renderer Per Draw Data Pool",
                    byte_size: (std::mem::size_of::<PerDrawData>() * 4096) as u32,
                    visibility: MemoryVisibility::CpuAndGpu,
                    usage: BufferUsage::STORAGE,
                    initial_data: None,
                },
            )
            .expect("Failed to create renderables pool!"),
            dynamic,
        };

        let cull_results = scene.output_bins().get_gpu_handle();
        let bin_counts = scene.bin_counts_gpu().handle;
        let num_bins = scene.num_bins() as u32;
        let proc = DataProcessors {
            scene,
            skinning,
            draw_builder: GPUDrawBuilder::new(
                &GPUDrawBuilderInfo {
                    name: "[MESHI] Deferred Renderer GPU Draw Builder",
                    ctx: ctx.as_mut(),
                    cull_results,
                    bin_counts,
                    num_bins,
                    ..Default::default()
                },
                state.as_mut(),
            ),
        };

        let subrender = Renderers { environment };

        let psos = DeferredPSO {
            pipelines: Default::default(),
            combine_pso: pso,
            standard: Self::build_pipeline(
                ctx.as_mut(),
                &mut state,
                info.sample_count,
                &proc,
                &data,
            ),
        };

        let exec = DeferredExecution { cull_queue };
        let mut text = TextRenderer::new();
        text.initialize_renderer(ctx.as_mut(), state.as_mut(), info.sample_count);
        Self {
            ctx,
            state,
            graph,
            exec,
            db: None,
            sample_count: info.sample_count,
            alloc,
            data,
            proc,
            subrender,
            psos,
            text,
        }
    }

    pub fn alloc(&mut self) -> &mut TransientAllocator {
        &mut self.alloc
    }

    fn build_pipeline(
        ctx: &mut Context,
        state: &mut BindlessState,
        sample_count: SampleCount,
        proc: &DataProcessors,
        data: &RendererData,
    ) -> PSO {
        let shaders = miso::gpudeferred(&[]);

        let s = PSOBuilder::new()
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .add_table_variable_with_resources(
                "per_draw_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(
                        proc.draw_builder.per_draw_data().into(),
                    ),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "per_scene_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::DynamicStorage(data.dynamic.state()),
                    slot: 0,
                }],
            )
            .add_reserved_table_variables(state)
            .unwrap()
            .add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: sample_count,
            })
            .set_details(GraphicsPipelineDetails {
                color_blend_states: vec![Default::default(); 4],
                sample_count,
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

        state.register_pso_tables(&s);
        s
    }

    fn allocate_billboard_material(&mut self, texture_id: u32) -> Handle<Material> {
        let mut material_handle = Handle::default();
        self.state
            .reserved_mut::<ReservedBindlessMaterials, _>("meshi_bindless_materials", |materials| {
                material_handle = materials.add_material();
                let material = materials.material_mut(material_handle);
                *material = Material::default();
                material.base_color_texture_id = texture_id as u32;
                material.normal_texture_id = u32::MAX;
                material.metallic_roughness_texture_id = u32::MAX;
                material.occlusion_texture_id = u32::MAX;
                material.emissive_texture_id = u32::MAX;
            })
            .expect("Failed to allocate billboard material");

        material_handle
    }

    fn update_billboard_material_texture(&mut self, material: Handle<Material>, texture_id: u32) {
        self.state
            .reserved_mut::<ReservedBindlessMaterials, _>("meshi_bindless_materials", |materials| {
                let material = materials.material_mut(material);
                material.base_color_texture_id = texture_id as u32;
            })
            .expect("Failed to update billboard material texture");
    }

    fn create_billboard_data(&mut self, mut info: BillboardInfo) -> BillboardData {
        let vertices = Self::billboard_vertices(Vec3::ZERO, Vec2::ONE, Vec4::ONE);
        let vertex_buffer = self
            .ctx
            .make_buffer(&BufferInfo {
                debug_name: "[MESHI] Billboard Vertex Buffer",
                byte_size: (std::mem::size_of::<BillboardVertex>() * vertices.len()) as u32,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::VERTEX,
                initial_data: Some(unsafe { vertices.align_to::<u8>().1 }),
            })
            .expect("Failed to create billboard vertex buffer");

        let mut owns_material = false;
        if info.material.is_none() {
            info.material = Some(self.allocate_billboard_material(info.texture_id));
            owns_material = true;
        }

        BillboardData {
            info,
            vertex_buffer,
            owns_material,
        }
    }

    fn billboard_vertices(center: Vec3, size: Vec2, color: Vec4) -> [BillboardVertex; 6] {
        let offsets = [
            Vec2::new(-0.5, -0.5),
            Vec2::new(0.5, -0.5),
            Vec2::new(0.5, 0.5),
            Vec2::new(-0.5, 0.5),
        ];
        let tex_coords = [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];

        let color = color.to_array();
        let center = center.to_array();
        let size = size.to_array();

        [
            BillboardVertex {
                center,
                offset: offsets[0].to_array(),
                size,
                color,
                tex_coords: tex_coords[0].to_array(),
            },
            BillboardVertex {
                center,
                offset: offsets[1].to_array(),
                size,
                color,
                tex_coords: tex_coords[1].to_array(),
            },
            BillboardVertex {
                center,
                offset: offsets[2].to_array(),
                size,
                color,
                tex_coords: tex_coords[2].to_array(),
            },
            BillboardVertex {
                center,
                offset: offsets[2].to_array(),
                size,
                color,
                tex_coords: tex_coords[2].to_array(),
            },
            BillboardVertex {
                center,
                offset: offsets[3].to_array(),
                size,
                color,
                tex_coords: tex_coords[3].to_array(),
            },
            BillboardVertex {
                center,
                offset: offsets[0].to_array(),
                size,
                color,
                tex_coords: tex_coords[0].to_array(),
            },
        ]
    }

    fn update_billboard_vertices(&mut self, billboard: &BillboardData, transform: Mat4) {
        let center = transform.transform_point3(Vec3::ZERO);
        let mut size = Vec2::new(
            transform.transform_vector3(Vec3::X).length(),
            transform.transform_vector3(Vec3::Y).length(),
        );

        if size.x <= 0.0 {
            size.x = 1.0;
        }
        if size.y <= 0.0 {
            size.y = 1.0;
        }

        let vertices = Self::billboard_vertices(center, size, Vec4::ONE);
        let mapped = self
            .ctx
            .map_buffer_mut::<BillboardVertex>(BufferView::new(billboard.vertex_buffer))
            .expect("Failed to map billboard vertex buffer");
        mapped[..vertices.len()].copy_from_slice(&vertices);
        self.ctx
            .unmap_buffer(billboard.vertex_buffer)
            .expect("Failed to unmap billboard vertex buffer");
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        db.import_dashi_context(self.ctx.as_mut());
        db.import_furikake_state(self.state.as_mut());
        self.alloc.set_bindless_registry(self.state.as_mut());
        self.db = Some(NonNull::new(db).expect("lmao"));
        self.text.initialize_database(db);
    }

    pub fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError> {
        let (scene_handle, transform_handle) = self.proc.scene.register_object(&SceneObjectInfo {
            local: Default::default(),
            global: Default::default(),
            scene_mask: PassMask::MAIN_COLOR as u32,
        });

        match info {
            RenderObjectInfo::Model(m) => {
                let draws: Vec<Handle<PerDrawData>> = m
                    .meshes
                    .iter()
                    .enumerate()
                    .map(|(idx, mesh)| {
                        self.proc.draw_builder.register_draw(&PerDrawData {
                            scene_id: scene_handle,
                            transform_id: transform_handle,
                            material_id: mesh
                                .material
                                .as_ref()
                                .and_then(|material| material.furikake_material_handle)
                                .unwrap_or_default(),

                            vertex_id: mesh.geometry.base.furikake_vertex_id.unwrap(),
                            vertex_count: mesh.geometry.base.vertex_count,
                            index_id: mesh.geometry.base.furikake_index_id.unwrap(),
                            index_count: mesh.geometry.base.index_count.unwrap(),
                            ..Default::default()
                        })
                    })
                    .collect();

                let h = self.data.objects.push(RenderObjectData {
                    kind: RenderObjectKind::Model(m.clone()),
                    scene_handle,
                    draws,
                });
                Ok(to_handle(h))
            }
            RenderObjectInfo::SkinnedModel(skinned) => {
                let (skinning_handle, skinning_info) = self
                    .proc
                    .skinning
                    .register(skinned.clone(), self.state.as_mut());
                let skinned_data = SkinnedRenderData {
                    model: skinned.model.clone(),
                    skinning: skinning_info,
                    skinning_handle,
                };

                let draws: Vec<Handle<PerDrawData>> = skinned_data
                    .model
                    .meshes
                    .iter()
                    .map(|mesh| {
                        self.proc.draw_builder.register_draw(&PerDrawData {
                            scene_id: scene_handle,
                            transform_id: transform_handle,
                            material_id: mesh
                                .material
                                .as_ref()
                                .and_then(|material| material.furikake_material_handle)
                                .unwrap_or_default(),

                            vertex_id: mesh.geometry.base.furikake_vertex_id.unwrap(),
                            vertex_count: mesh.geometry.base.vertex_count,
                            index_id: mesh.geometry.base.furikake_index_id.unwrap(),
                            index_count: mesh.geometry.base.index_count.unwrap(),
                            skeleton_id: skinned_data.skinning.skeleton,
                            animation_state_id: skinned_data.skinning.animation_state,
                            per_obj_joints_id: skinned_data.skinning.joints,
                            ..Default::default()
                        })
                    })
                    .collect();

                let h = self.data.objects.push(RenderObjectData {
                    kind: RenderObjectKind::SkinnedModel(skinned_data),
                    scene_handle,
                    draws,
                });
                Ok(to_handle(h))
            }
            RenderObjectInfo::Billboard(billboard) => {
                todo!()
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

        if !self
            .data
            .objects
            .entries
            .iter()
            .any(|h| h.slot == handle.slot)
        {
            warn!("Failed to update animation for object {}", handle.slot);
            return;
        }

        let obj = self.data.objects.get_ref_mut(from_handle(handle));

        match &mut obj.kind {
            RenderObjectKind::SkinnedModel(skinned) => {
                self.proc
                    .skinning
                    .set_animation_state(skinned.skinning_handle, state);
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
    }

    pub fn set_billboard_material(
        &mut self,
        handle: Handle<RenderObject>,
        material: Option<Handle<Material>>,
    ) {
        todo!()
    }

    pub fn release_object(&mut self, handle: Handle<RenderObject>) {
        if !handle.valid() {
            return;
        }
    }

    pub fn object_transform(&self, handle: Handle<RenderObject>) -> glam::Mat4 {
        if !handle.valid() {
            return Default::default();
        }

        if !self
            .data
            .objects
            .entries
            .iter()
            .any(|h| h.slot == handle.slot)
        {
            return Default::default();
        }

        let obj = self.data.objects.get_ref(from_handle(handle));
        self.proc.scene.get_object_transform(obj.scene_handle)
    }

    pub fn set_object_transform(&mut self, handle: Handle<RenderObject>, transform: &glam::Mat4) {
        if !handle.valid() {
            warn!("Attempted to update transformation of invalid handle.");
            return;
        }

        if !self
            .data
            .objects
            .entries
            .iter()
            .any(|h| h.slot == handle.slot)
        {
            warn!("Failed to update transform for object {}", handle.slot);
            return;
        }

        let obj = self.data.objects.get_ref(from_handle(handle));
        self.proc
            .scene
            .set_object_transform(obj.scene_handle, transform);
    }

    pub fn register_text(&mut self, info: &TextInfo) -> Handle<TextObject> {
        self.text.register_text(info)
    }

    pub fn release_text(&mut self, handle: Handle<TextObject>) {
        self.text.release_text(handle);
    }

    pub fn set_text(&mut self, handle: Handle<TextObject>, text: &str) {
        self.text.set_text(handle, text);
    }

    pub fn set_text_info(&mut self, handle: Handle<TextObject>, info: &TextInfo) {
        self.text.set_text_info(handle, info);
    }

    fn pull_scene(&mut self) -> Handle<Semaphore> {
        let wait = self.graph.make_semaphore();
        self.exec
            .cull_queue
            .record(|c| {
                let state_update = self
                    .state
                    .update()
                    .expect("Failed to update furikake state");

                let cull_cmds = state_update.combine(self.proc.scene.cull());
                cull_cmds.append(c).unwrap();
            })
            .expect("Failed to make commands");

        self.exec
            .cull_queue
            .submit(&SubmitInfo {
                signal_sems: &[wait],
                ..Default::default()
            })
            .expect("Failed to submit!");
        wait
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
        if self.exec.cull_queue.current_index() == 0 {
            self.data.dynamic.reset();
            self.subrender.environment.reset();
            self.proc.draw_builder.reset();
        }

        let skinning_complete = self.proc.skinning.update(delta_time);

        // Set active scene cameras..
        self.proc.scene.set_active_cameras(views);
        // Pull scene GPU --> CPU to read.
        let scene_processing = self.pull_scene();

        // Default framebuffer info.
        let default_framebuffer_info = ImageInfo {
            debug_name: "",
            dim: [
                self.data.viewport.area.w as u32,
                self.data.viewport.area.h as u32,
                1,
            ],
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

            let camera_handle = *camera;

            self.graph.add_compute_pass(|mut cmd| {
                cmd.combine(self.proc.draw_builder.build_draws(0, view_idx as u32))
                    .end()
            });

            // Deferred SPLIT pass. Renders the following framebuffers:
            // 1) Position
            // 2) Albedo (or diffuse)
            // 3) Normal
            // 4) Material Code
            self.graph.add_subpass(
                &SubpassInfo {
                    viewport: self.data.viewport,
                    color_attachments: deferred_pass_attachments,
                    depth_attachment: Some(depth.view),
                    clear_values: deferred_pass_clear,
                    depth_clear: Some(ClearValue::DepthStencil {
                        depth: 1.0,
                        stencil: 0,
                    }),
                },
                |mut cmd| {
                    struct PerSceneData {
                        camera: Handle<Camera>,
                    }
                    let mut alloc = self
                        .data
                        .dynamic
                        .bump()
                        .expect("Failed to allocate dynamic buffer!");

                    alloc.slice::<PerSceneData>()[0].camera = camera_handle;

                    let indices = self
                        .state
                        .binding("meshi_bindless_indices")
                        .unwrap()
                        .binding();

                    match indices {
                        ReservedBinding::TableBinding { binding, resources } => {
                            match resources[0].resource {
                                ShaderResource::StorageBuffer(view) => {
                                    return cmd
                                        .bind_graphics_pipeline(self.psos.standard.handle)
                                        .update_viewport(&self.data.viewport)
                                        .draw_indexed_indirect(&DrawIndexedIndirect {
                                            indices: view.handle,
                                            indirect: self.proc.draw_builder.draw_list(),
                                            bind_tables: self.psos.standard.tables(),
                                            dynamic_buffers: [None, None, Some(alloc), None],
                                            draw_count: self.proc.draw_builder.draw_count(),
                                            ..Default::default()
                                        })
                                        .unbind_graphics_pipeline();
                                }
                                _ => (),
                            }
                        }
                        _ => (),
                    }

                    return cmd;
                },
            );

            ///////////////////////////////////////////////////////////////////
            ///////////////////////////////////////////////////////////////////
            // Deferred COMBINE pass. Combines all deferred attachments.     //
            ///////////////////////////////////////////////////////////////////
            ///////////////////////////////////////////////////////////////////
            self.graph.add_subpass(
                &SubpassInfo {
                    viewport: self.data.viewport,
                    color_attachments: deferred_combine_attachments,
                    depth_attachment: None,
                    clear_values: deferred_combine_clear,
                    depth_clear: None,
                },
                |mut cmd| {
                    let mut alloc = self
                        .data
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
                        .bind_graphics_pipeline(self.psos.combine_pso.handle)
                        .update_viewport(&self.data.viewport)
                        .draw(&Draw {
                            bind_tables: self.psos.combine_pso.tables(),
                            dynamic_buffers: [None, Some(alloc), None, None],
                            instance_count: 1,
                            count: 3,
                            ..Default::default()
                        })
                        .unbind_graphics_pipeline();

                    cmd
                },
            );

            ///////////////////////////////////////////////////////////////////
            ///////////////////////////////////////////////////////////////////
            // Transparent forward pass.                                      //
            ///////////////////////////////////////////////////////////////////
            ///////////////////////////////////////////////////////////////////

            let mut transparent_attachments: [Option<ImageView>; 8] = [None; 8];
            transparent_attachments[0] = Some(final_combine.view);
            let transparent_clear: [Option<ClearValue>; 8] = [None; 8];

            self.graph.add_subpass(
                &SubpassInfo {
                    viewport: self.data.viewport,
                    color_attachments: transparent_attachments,
                    depth_attachment: Some(depth.view),
                    clear_values: transparent_clear,
                    depth_clear: None,
                },
                |mut cmd| {

                    // TODO this should combine instead
                    //            self.subrender.environment.render(
                    //                &mut self.graph,
                    //                &self.data.viewport,
                    //                final_combine.view,
                    //                Some(depth.view),
                    //                camera_handle,
                    //                delta_time,
                    //            );

                    let c =
                        self.text
                            .render_transparent(self.ctx.as_mut(), &self.data.viewport, cmd);

                    c
                },
            );

            outputs.push(ViewOutput {
                camera: *camera,
                image: final_combine.view,
                semaphore: semaphores[0],
            });
        }

        let mut wait_sems = Vec::with_capacity(sems.len() + 1);
        wait_sems.extend_from_slice(sems);
        if let Some(semaphore) = skinning_complete {
            wait_sems.push(semaphore);
        }

        wait_sems.push(scene_processing);

        self.graph.execute_with(&SubmitInfo {
            wait_sems: &wait_sems,
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

    fn register_text(&mut self, info: &TextInfo) -> Handle<TextObject> {
        DeferredRenderer::register_text(self, info)
    }

    fn release_text(&mut self, handle: Handle<TextObject>) {
        DeferredRenderer::release_text(self, handle);
    }

    fn set_text(&mut self, handle: Handle<TextObject>, text: &str) {
        DeferredRenderer::set_text(self, handle, text);
    }

    fn set_text_info(&mut self, handle: Handle<TextObject>, info: &TextInfo) {
        DeferredRenderer::set_text_info(self, handle, info);
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
