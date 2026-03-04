use std::collections::HashMap;

use bento::{
    Compiler, OptimizationLevel, Request, ShaderLang,
    builder::{AttachmentDesc, PSO, PSOBuilder},
};
use bumpalo::{Bump, collections::Vec as BumpVec};
use bytemuck::cast_slice;
use dashi::{
    cmd::Executable,
    gpu::cmd::{Scope, SyncPoint},
    utils::gpupool::GPUPool,
    *,
};
use driver::command::{BlitImage, Draw, DrawIndexedIndirect};
use execution::{CommandDispatch, CommandRing};
use furikake::{
    BindlessState, PSOBuilderFurikakeExt,
    reservations::{
        ReservedBinding, bindless_camera::ReservedBindlessCamera,
        bindless_indices::ReservedBindlessIndices, bindless_materials::ReservedBindlessMaterials,
        bindless_vertices::ReservedBindlessVertices,
    },
    types::{AnimationState as FurikakeAnimationState, Material, VertexBufferSlot, *},
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use meshi_utils::MeshiError;
use noren::{
    DB, RDBFile,
    meta::{DeviceMaterial, DeviceMesh, DeviceModel},
    rdb::{DeviceGeometry, DeviceGeometryLayer, HostGeometry, primitives::Vertex},
};
use resource_pool::resource_list::ResourceList;
use tare::{graph::*, transient::TransientAllocator, utils::StagedBuffer};
use tracing::{info, warn};

use super::{
    Renderer, RendererInfo, ViewOutput,
    debug::DebugLayer,
    environment::{
        EnvironmentFrameSettings, EnvironmentRenderer, EnvironmentRendererInfo,
        terrain::{TERRAIN_DRAW_BIN, TerrainFrameSettings},
    },
    gui::GuiRenderer,
    shadow::{ShadowPipelineMode, ShadowProcessInfo, ShadowSystem, ShadowSystemInfo},
    text::{TextDraw, TextDrawMode, TextRenderer},
    utils::{
        billboard::{
            BillboardData, allocate_billboard_material, build_billboard_pipeline,
            create_billboard_data, update_billboard_material_texture, update_billboard_vertices,
        },
        gpu_draw_builder::{GPUDrawBuilder, GPUDrawBuilderInfo},
        scene::GPUScene,
        skinning::{SkinningDispatcher, SkinningHandle, SkinningInfo},
    },
};
use crate::{
    AnimationState, BillboardInfo, BillboardType, CloudDebugView, GuiInfo, GuiObject, RenderObject,
    RenderObjectInfo, TextInfo, TextObject, TextRenderMode,
    gui::{
        GuiFrame, Slider,
        debug::{
            DebugRadialOption, DebugRegistryValue, PageType,
            debug_register_radial_with_description_and_conflicts, debug_register_with_description,
        },
    },
    render::{SubrendererDrawInfo, SubrendererInitInfo, SubrendererProcessInfo, utils::scene::*},
};

//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////

#[repr(u32)]
pub enum PassMask {
    PRE_Z = 0x00000001,
    OPAQUE_GEOMETRY = 0x00000010,
    TERRAIN = 0x00000020,
    SHADOW = 0x0000100,
    TRANSPARENT = 0x00001000,
}

const BIN_PRE_Z: u32 = 0;
const BIN_GBUFFER_OPAQUE: u32 = 1;
const BIN_SHADOW: u32 = 2;
const BIN_TRANSPARENT: u32 = 3;
pub(crate) const BIN_TERRAIN: u32 = 4;

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
    terrain_height_texture_id: u32,
    terrain_normal_texture_id: u32,
    terrain_blend_texture_id: u32,
    terrain_blend_ids_texture_id: u32,
    terrain_clipmap_tile: u32,
}

impl PerDrawData {
    pub fn terrain_draw(
        scene_id: Handle<SceneObject>,
        transform_id: Handle<Transformation>,
        material_id: Handle<Material>,
        vertex_id: u32,
        vertex_count: u32,
        index_id: u32,
        index_count: u32,
        terrain_clipmap_tile: u32,
    ) -> Self {
        Self {
            scene_id,
            transform_id,
            material_id,
            vertex_id,
            vertex_count,
            index_id,
            index_count,
            terrain_height_texture_id: u32::MAX,
            terrain_normal_texture_id: u32::MAX,
            terrain_blend_texture_id: u32::MAX,
            terrain_blend_ids_texture_id: u32::MAX,
            terrain_clipmap_tile,
            ..Default::default()
        }
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct PerObjectInfo {
    transform: Mat4,
    scene_id: Handle<SceneObject>,
    material_id: Handle<Material>,
    camera_id: Handle<Camera>,
    skeleton_id: Handle<SkeletonHeader>,
    animation_state_id: Handle<FurikakeAnimationState>,
    per_obj_joints_id: Handle<JointTransform>,
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
    debug: DebugLayer,
    shadow: ShadowSystem,
}

struct DeferredPSO {
    pipelines: HashMap<Handle<Material>, PSO>,
    standard: PSO,
    billboard: PSO,
    combine_pso: PSO,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeferredFramebufferDebugView {
    None = 0,
    Position = 1,
    Diffuse = 2,
    Normal = 3,
    Material = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeferredShadowDebugView {
    None = 0,
    Cascaded = 1,
    Spot = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeferredDepthDebugView {
    Off = 0,
    On = 1,
}

#[derive(Clone, Copy, Debug)]
struct DeferredDebugViews {
    cloud_debug_view: CloudDebugView,
    deferred_framebuffer: u32,
    depth: u32,
}

impl Default for DeferredDebugViews {
    fn default() -> Self {
        Self {
            cloud_debug_view: CloudDebugView::None,
            deferred_framebuffer: DeferredFramebufferDebugView::None as u32,
            depth: DeferredDepthDebugView::Off as u32,
        }
    }
}

pub struct DeferredRenderer {
    ctx: Box<Context>,
    data: RendererData,
    proc: DataProcessors,
    subrender: Renderers,
    psos: DeferredPSO,
    sample_count: SampleCount,
    state: Box<BindlessState>,
    alloc: Box<TransientAllocator>,
    graph: RenderGraph,
    text: TextRenderer,
    gui: GuiRenderer,
    frame_count: usize,
    frame_bump: Bump,
    debug_views: DeferredDebugViews,
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
        info!(
            "Initialing Deferred Renderer using device {} with main viewport dimensions [{}, {}]",
            device, info.initial_viewport.area.w, info.initial_viewport.area.h
        );

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

        ctx.init_gpu_timers(64).unwrap();

        // Init global command dispatcher
        CommandDispatch::init(ctx.as_mut()).expect("Failed to init command dispatcher!");
        let mut state = Box::new(BindlessState::new(&mut ctx));

        // Initialize GPU Scene to process scenes (from a camera). Each scene processes data into
        // bins.
        let scene = GPUScene::new(
            &GPUSceneInfo {
                name: "[MESHI] Deferred Renderer Scene",
                ctx: ctx.as_mut(),
                draw_bins: &[
                    SceneBin {
                        id: BIN_PRE_Z,
                        mask: PassMask::PRE_Z as u32,
                    },
                    SceneBin {
                        id: BIN_GBUFFER_OPAQUE,
                        mask: PassMask::OPAQUE_GEOMETRY as u32,
                    },
                    SceneBin {
                        id: BIN_SHADOW,
                        mask: PassMask::OPAQUE_GEOMETRY as u32 | PassMask::SHADOW as u32,
                    },
                    SceneBin {
                        id: BIN_TRANSPARENT,
                        mask: PassMask::TRANSPARENT as u32,
                    },
                    SceneBin {
                        id: BIN_TERRAIN,
                        mask: PassMask::TERRAIN as u32,
                    },
                ],
                ..Default::default()
            },
            state.as_mut(),
        );

        let skinning = SkinningDispatcher::new(ctx.as_mut(), state.as_ref());
        let cull_results = scene.output_bins().get_gpu_handle();
        let bin_counts = scene.bin_counts_gpu().handle;
        let num_bins = scene.num_bins() as u32;
        let mut proc = DataProcessors {
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

        let mut alloc = Box::new(TransientAllocator::new(ctx.as_mut()));

        let dynamic = ctx
            .make_dynamic_allocator(&DynamicAllocatorInfo {
                byte_size: 2048 * 2048,
                ..Default::default()
            })
            .expect("Unable to create dynamic allocator!");

        let environment = EnvironmentRenderer::new(
            ctx.as_mut(),
            state.as_mut(),
            &mut proc.draw_builder, 
            EnvironmentRendererInfo {
                initial_viewport: info.initial_viewport,
                color_format: Format::BGRA8,
                sample_count: info.sample_count,
                use_depth: true,
                skybox: super::environment::sky::SkyboxInfo::default(),
                ocean: super::environment::ocean::OceanInfo::default(),
                terrain: super::environment::terrain::TerrainInfo::default(),
                cloud_depth_view: None,
            },
        );

        let debug = DebugLayer::new(ctx.as_mut());
        let graph = RenderGraph::new_with_transient_allocator(&mut ctx, &mut alloc);

        let cull_queue = ctx
            .make_command_ring(&CommandQueueInfo2 {
                debug_name: "[CULL]",
                parent: None,
                queue_type: QueueType::Graphics,
            })
            .expect("Failed to make cull command queue");

        let compiler = Compiler::new().expect("Failed to create shader compiler");
        let base_request = Request {
            name: Some("meshi_deferred_combine".to_string()),
            lang: ShaderLang::Slang,
            optimization: OptimizationLevel::Performance,
            debug_symbols: true,
            defines: Default::default(),
            ..Default::default()
        };
        let vertex = compiler
            .compile(
                include_str!("shaders/deferred_combine_vert.slang").as_bytes(),
                &Request {
                    stage: ShaderType::Vertex,
                    ..base_request.clone()
                },
            )
            .expect("Failed to compile deferred combine vertex shader");
        let fragment = compiler
            .compile(
                include_str!("shaders/deferred_combine_frag.slang").as_bytes(),
                &Request {
                    stage: ShaderType::Fragment,
                    ..base_request
                },
            )
            .expect("Failed to compile deferred combine fragment shader");

        let dummy = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[MESHI] Dummy Buffer",
                byte_size: 1024,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            })
            .expect("Failed to create dummy buffer");

        let mut psostate = PSOBuilder::new()
            .set_debug_name("[MESHI] Deferred Combine")
            .vertex_compiled(Some(vertex))
            .fragment_compiled(Some(fragment))
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
            )
            .add_table_variable_with_resources(
                "shadow_cascade_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(dummy.into()),
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

        let shadow = ShadowSystem::new(
            SubrendererInitInfo {
                ctx: ctx.as_mut(),
                state: state.as_mut(),
                per_draw_buffer: proc.draw_builder.per_draw_data(),
                per_scene_dynamic: data.dynamic.state(),
            },
            ShadowSystemInfo {
                sample_count: info.sample_count,
                cascades: info.shadow_cascades,
                cascaded_resolution: 2048,
                spot_resolution: 1024,
                depth_clear: ClearValue::DepthStencil {
                    depth: 1.0,
                    stencil: 0,
                },
            },
            ShadowPipelineMode::Deferred,
        );

        let mut subrender = Renderers {
            environment,
            debug,
            shadow,
        };

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
            billboard: build_billboard_pipeline(
                ctx.as_mut(),
                &mut state,
                info.sample_count,
                ShaderResource::DynamicStorage(data.dynamic.state()),
            ),
        };

        let mut text = TextRenderer::new();
        text.initialize_renderer(ctx.as_mut(), state.as_mut(), info.sample_count);
        let gui = GuiRenderer::new();

        Self {
            ctx,
            state,
            graph,
            sample_count: info.sample_count,
            alloc,
            data,
            proc,
            subrender,
            psos,
            text,
            gui,
            frame_count: 0,
            frame_bump: Bump::new(),
            debug_views: DeferredDebugViews::default(),
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
            .set_debug_name("[MESHI] STDDeferred")
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
            .set_attachment_format(0, Format::RGBA32F)
            .set_attachment_format(1, Format::RGBA8)
            .set_attachment_format(2, Format::RGBA32F)
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
                    ..Default::default()
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

    pub fn initialize_database(&mut self, db: &mut DB) {
        db.import_dashi_context(self.ctx.as_mut());
        db.import_furikake_state(self.state.as_mut());
        self.alloc.set_bindless_registry(self.state.as_mut());
        self.subrender.environment.initialize_database(db);
        self.subrender
            .environment
            .initialize_terrain_deferred(&mut self.proc.draw_builder);

        self.register_debug_views();
        self.text.initialize_database(db);
    }

    fn register_debug_views(&mut self) {
        let cloud_view = DebugRegistryValue::CloudDebugView(&mut self.debug_views.cloud_debug_view);
        let deferred_view = DebugRegistryValue::U32(&mut self.debug_views.deferred_framebuffer);
        let depth_view = DebugRegistryValue::U32(&mut self.debug_views.depth);
        let cloud_conflicts = [deferred_view.clone(), depth_view.clone()];
        let deferred_conflicts = [cloud_view.clone(), depth_view.clone()];
        let depth_conflicts = [cloud_view.clone(), deferred_view.clone()];
        unsafe {
            debug_register_radial_with_description_and_conflicts(
                PageType::DebugViews,
                "Cloud Debug Views",
                cloud_view,
                &[
                    DebugRadialOption {
                        label: "None",
                        value: CloudDebugView::None as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Weather Map",
                        value: CloudDebugView::WeatherMap as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Shadow Map",
                        value: CloudDebugView::ShadowMap as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Transmittance",
                        value: CloudDebugView::Transmittance as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Step Heatmap",
                        value: CloudDebugView::StepHeatmap as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Temporal Weight",
                        value: CloudDebugView::TemporalWeight as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Stats",
                        value: CloudDebugView::Stats as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Layer A",
                        value: CloudDebugView::LayerA as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Layer B",
                        value: CloudDebugView::LayerB as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Single Scatter",
                        value: CloudDebugView::SingleScatter as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Multi Scatter",
                        value: CloudDebugView::MultiScatter as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Cloud Cascade 0",
                        value: CloudDebugView::ShadowCascade0 as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Cloud Cascade 1",
                        value: CloudDebugView::ShadowCascade1 as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Cloud Cascade 2",
                        value: CloudDebugView::ShadowCascade2 as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Cloud Cascade 3",
                        value: CloudDebugView::ShadowCascade3 as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Opaque Cascade 0",
                        value: CloudDebugView::OpaqueShadowCascade0 as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Opaque Cascade 1",
                        value: CloudDebugView::OpaqueShadowCascade1 as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Opaque Cascade 2",
                        value: CloudDebugView::OpaqueShadowCascade2 as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Opaque Cascade 3",
                        value: CloudDebugView::OpaqueShadowCascade3 as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Opaque Cascaded Atlas",
                        value: CloudDebugView::OpaqueShadowAtlas as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Opaque Sample UV",
                        value: CloudDebugView::OpaqueShadowSampleUV as u32 as f32,
                    },
                ],
                Some("Selects a cloud rendering debug view to display."),
                Some(&cloud_conflicts),
            );
            debug_register_radial_with_description_and_conflicts(
                PageType::DebugViews,
                "Deferred Framebuffers",
                deferred_view,
                &[
                    DebugRadialOption {
                        label: "None",
                        value: DeferredFramebufferDebugView::None as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Position",
                        value: DeferredFramebufferDebugView::Position as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Diffuse",
                        value: DeferredFramebufferDebugView::Diffuse as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Normal",
                        value: DeferredFramebufferDebugView::Normal as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "Material",
                        value: DeferredFramebufferDebugView::Material as u32 as f32,
                    },
                ],
                Some("Selects a deferred GBuffer attachment to display."),
                Some(&deferred_conflicts),
            );
            debug_register_radial_with_description_and_conflicts(
                PageType::DebugViews,
                "Depth Buffer",
                depth_view,
                &[
                    DebugRadialOption {
                        label: "Off",
                        value: DeferredDepthDebugView::Off as u32 as f32,
                    },
                    DebugRadialOption {
                        label: "On",
                        value: DeferredDepthDebugView::On as u32 as f32,
                    },
                ],
                Some("Displays the main depth buffer as the renderer output."),
                Some(&depth_conflicts),
            );
        }
    }

    pub fn register_object(
        &mut self,
        info: &RenderObjectInfo,
    ) -> Result<Handle<RenderObject>, MeshiError> {
        let scene_mask = match info {
            RenderObjectInfo::Model(_) => {
                PassMask::OPAQUE_GEOMETRY as u32 | PassMask::SHADOW as u32
            }
            RenderObjectInfo::SkinnedModel(_) => {
                PassMask::OPAQUE_GEOMETRY as u32 | PassMask::SHADOW as u32
            }
            RenderObjectInfo::Billboard(_) => PassMask::TRANSPARENT as u32,
            RenderObjectInfo::Empty => PassMask::OPAQUE_GEOMETRY as u32 | PassMask::SHADOW as u32,
        };
        let (scene_handle, transform_handle) = self.proc.scene.register_object(&SceneObjectInfo {
            local: Default::default(),
            global: Default::default(),
            scene_mask,
            scene_type: SceneNodeType::Renderable,
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
                let billboard_data = create_billboard_data(
                    self.ctx.as_mut(),
                    self.state.as_mut(),
                    billboard.clone(),
                );
                let h = self.data.objects.push(RenderObjectData {
                    kind: RenderObjectKind::Billboard(billboard_data),
                    scene_handle,
                    draws: Vec::new(),
                });
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

        if !self
            .data
            .objects
            .entries
            .iter()
            .any(|h| h.slot == handle.slot)
        {
            warn!(
                "Failed to update billboard texture for object {}",
                handle.slot
            );
            return;
        }

        let (owns_material, material_handle) = {
            let obj = self.data.objects.get_ref_mut(from_handle(handle));
            match &mut obj.kind {
                RenderObjectKind::Billboard(billboard) => {
                    billboard.info.texture_id = texture_id;
                    (billboard.owns_material, billboard.info.material)
                }
                _ => {
                    warn!("Attempted to update billboard texture on non-billboard object.");
                    return;
                }
            }
        };

        if owns_material {
            if let Some(material) = material_handle {
                update_billboard_material_texture(self.state.as_mut(), material, texture_id);
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

        if !self
            .data
            .objects
            .entries
            .iter()
            .any(|h| h.slot == handle.slot)
        {
            warn!(
                "Failed to update billboard material for object {}",
                handle.slot
            );
            return;
        }

        let (owns_material, current_material, texture_id) = {
            let obj = self.data.objects.get_ref_mut(from_handle(handle));
            match &mut obj.kind {
                RenderObjectKind::Billboard(billboard) => (
                    billboard.owns_material,
                    billboard.info.material,
                    billboard.info.texture_id,
                ),
                _ => {
                    warn!("Attempted to update billboard material on non-billboard object.");
                    return;
                }
            }
        };

        if owns_material {
            if let Some(material) = current_material {
                let _ = self.state.reserved_mut::<ReservedBindlessMaterials, _>(
                    "meshi_bindless_materials",
                    |materials| {
                        materials.remove_material(material);
                    },
                );
            }
        }

        let s = self as *mut Self;
        let obj = self.data.objects.get_ref_mut(from_handle(handle));
        let RenderObjectKind::Billboard(billboard) = &mut obj.kind else {
            return;
        };

        if let Some(material) = material {
            billboard.info.material = Some(material);
            billboard.owns_material = false;
        } else {
            let new_material =
                allocate_billboard_material(unsafe { &mut (*s) }.state.as_mut(), texture_id);
            billboard.info.material = Some(new_material);
            billboard.owns_material = true;
        }
    }

    pub fn release_object(&mut self, handle: Handle<RenderObject>) {
        if !handle.valid() {
            return;
        }

        if !self
            .data
            .objects
            .entries
            .iter()
            .any(|h| h.slot == handle.slot)
        {
            return;
        }

        let mut billboard_release = None;
        let mut skinning_handle = None;
        let (scene_handle, draws) = {
            let obj = self.data.objects.get_ref(from_handle(handle));
            match &obj.kind {
                RenderObjectKind::SkinnedModel(skinned) => {
                    skinning_handle = Some(skinned.skinning_handle);
                }
                RenderObjectKind::Billboard(billboard) => {
                    billboard_release = Some((
                        billboard.vertex_buffer,
                        billboard.info.material,
                        billboard.owns_material,
                    ));
                }
                RenderObjectKind::Model(_) => {}
            }

            (obj.scene_handle, obj.draws.clone())
        };

        if let Some(handle) = skinning_handle {
            self.proc.skinning.unregister(handle, self.state.as_mut());
        }

        if let Some((vertex_buffer, material, owns_material)) = billboard_release {
            self.ctx.destroy_buffer(vertex_buffer);
            if owns_material {
                if let Some(material) = material {
                    self.state
                        .reserved_mut::<ReservedBindlessMaterials, _>(
                            "meshi_bindless_materials",
                            |materials| materials.remove_material(material),
                        )
                        .expect("Failed to release billboard material");
                }
            }
        }

        for draw in draws {
            self.proc.draw_builder.release_draw(draw);
        }

        self.proc.scene.release_object(scene_handle);
        self.data.objects.release(from_handle(handle));
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

    pub fn register_gui(&mut self, info: &GuiInfo) -> Handle<GuiObject> {
        self.gui.register_gui(info)
    }

    pub fn release_text(&mut self, handle: Handle<TextObject>) {
        self.text.release_text(handle);
    }

    pub fn release_gui(&mut self, handle: Handle<GuiObject>) {
        self.gui.release_gui(handle);
    }

    pub fn set_text(&mut self, handle: Handle<TextObject>, text: &str) {
        self.text.set_text(handle, text);
    }

    pub fn set_text_info(&mut self, handle: Handle<TextObject>, info: &TextInfo) {
        self.text.set_text_info(handle, info);
    }

    pub fn set_gui_info(&mut self, handle: Handle<GuiObject>, info: &GuiInfo) {
        self.gui.set_gui_info(handle, info);
    }

    pub fn set_gui_visibility(&mut self, handle: Handle<GuiObject>, visible: bool) {
        self.gui.set_gui_visibility(handle, visible);
    }

    pub fn upload_gui_frame(&mut self, frame: GuiFrame) {
        let GuiFrame {
            batches,
            text_draws,
        } = frame;
        self.gui.upload_frame(GuiFrame {
            batches,
            text_draws: Vec::new(),
        });
        let frame_draws = text_draws
            .into_iter()
            .map(|draw| TextDraw {
                text: draw.text,
                position: Vec2::new(draw.position[0], draw.position[1]),
                color: Vec4::from_array(draw.color),
                scale: draw.scale,
                mode: TextDrawMode::Plain,
            })
            .collect();
        self.text.set_frame_draws(frame_draws);
    }

    fn prep_frame(&mut self, delta_time: f32, views: &[Handle<Camera>]) -> Vec<Handle<Semaphore>> {
        let mut sems = Vec::new();

        // Init gui...
        self.gui.initialize_renderer(
            self.ctx.as_mut(),
            self.state.as_mut(),
            &self.data.dynamic,
            self.sample_count,
        );

        // Set active scene cameras..
        self.proc.scene.set_active_cameras(views);

        let mut cloud_settings = self.subrender.environment.cloud_settings();
        if cloud_settings.debug_view != self.debug_views.cloud_debug_view {
            cloud_settings.debug_view = self.debug_views.cloud_debug_view;
            self.subrender
                .environment
                .set_cloud_settings(cloud_settings);
        }

        if let Some(s) = self.proc.skinning.update(delta_time) {
            sems.push(s);
        }

        sems
    }

    fn pre_compute(&mut self, delta_time: f32, camera: Handle<Camera>) {
        let bump = crate::render::global_bump().get();
        let camera = bump.alloc(camera.clone());
        let delta_time = bump.alloc(delta_time.clone());
        self.subrender.environment.update(EnvironmentFrameSettings {
            delta_time: *delta_time,
            ..Default::default()
        });
        self.subrender
            .environment
            .update_terrain(*camera, self.state.as_mut());

        self.graph.add_compute_pass(|mut cmd| {
            let state_update = self
                .state
                .update()
                .expect("Failed to update furikake state")
                .combine(self.proc.scene.cull());

            cmd = cmd
                .combine(self.proc.scene.pre_compute())
                .combine(self.proc.draw_builder.pre_compute())
                .combine(
                    self.subrender
                        .environment
                        .pre_compute(self.ctx.as_mut(), self.state.as_mut()),
                )
                .combine(self.subrender.shadow.pre_compute())
                .combine(self.gui.pre_compute())
                .combine(self.text.pre_compute())
                .sync(SyncPoint::TransferToCompute, Scope::AllCommonReads)
                .combine(state_update)
                .combine(self.subrender.environment.record_compute(self.ctx.as_mut()))
                .sync(SyncPoint::ComputeToGraphics, Scope::AllCommonReads)
                .combine(self.subrender.environment.record_clouds_update(
                    self.ctx.as_mut(),
                    self.state.as_mut(),
                    &self.data.viewport,
                    *camera,
                    *delta_time,
                ))
                .sync(SyncPoint::ComputeToGraphics, Scope::AllCommonReads);

            cmd.end()
        });
    }

    fn subrender_draw_info(&mut self) -> SubrendererDrawInfo {
        unsafe {
            let graph = &mut *(&mut self.graph as *mut RenderGraph);
            let draw_builder = &mut *(&mut self.proc.draw_builder as *mut GPUDrawBuilder);
            let alloc = &mut *(&mut self.data.dynamic as *mut DynamicAllocator);
            let state = &mut *(self.state.as_mut() as *mut BindlessState);
            let mut subrender_info = SubrendererDrawInfo {
                draw_builder,
                graph,
                alloc,
                state,
                camera: Default::default(),
                viewport: self.data.viewport,
            };

            return subrender_info;
        }
    }

    pub fn update(
        &mut self,
        sems: &[Handle<Semaphore>],
        views: &[Handle<Camera>],
        delta_time: f32,
    ) -> Vec<ViewOutput> {
        self.frame_count += 1;
        if self.frame_count % 3 == 0 {
            self.frame_bump.reset();
        }
        if views.is_empty() {
            return Vec::new();
        }

        // Prepare for frame... this does the following (not all encompassing):
        // 1) Build skinning transformations
        // 2) Syncs all cpu configto gpu
        let prep_sems = self.prep_frame(delta_time, views);

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

        let mut subrender_info = self.subrender_draw_info();
        for (view_idx, camera) in views.iter().enumerate() {
            subrender_info.camera = *camera;
            let position = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI DEFERRED] Position Framebuffer View {view_idx}"),
                format: Format::RGBA32F,
                ..default_framebuffer_info
            });

            let diffuse = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI DEFERRED] Diffuse Framebuffer View {view_idx}"),
                ..default_framebuffer_info
            });

            let normal = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI DEFERRED] Normal Framebuffer View {view_idx}"),
                format: Format::RGBA32F,
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
            let scene_color = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI DEFERRED] Scene Color View {view_idx}"),
                format: Format::BGRA8,
                samples: self.sample_count,
                ..default_framebuffer_info
            });

            let depth = self.graph.make_image(&ImageInfo {
                debug_name: &format!("[MESHI DEFERRED] Depth Framebuffer View {view_idx}"),
                format: Format::D24S8,
                samples: self.sample_count,
                ..default_framebuffer_info
            });

            assert!(self.ctx.image_info(normal.view.img).format == Format::RGBA32F);
            assert!(self.ctx.image_info(position.view.img).format == Format::RGBA32F);
            assert!(self.ctx.image_info(diffuse.view.img).format == Format::RGBA8);
            assert!(self.ctx.image_info(material_code.view.img).format == Format::RGBA8);

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

            // PRECOMPUTE: Prepare everything for frame
            self.pre_compute(delta_time, camera_handle);

            self.graph.add_compute_pass(|cmd| {
                let cmd = cmd
                    .combine(self.proc.draw_builder.build_draws(
                        &[BIN_GBUFFER_OPAQUE, BIN_SHADOW, BIN_TERRAIN],
                        view_idx as u32,
                    ))
                    .sync(SyncPoint::ComputeToGraphics, Scope::AllCommonReads);

                cmd.end()
            });

            // Deferred SPLIT pass. Renders the following framebuffers:
            // 1) Position
            // 2) Albedo (or diffuse)
            // 3) Normal
            // 4) Material Code
            self.graph.add_subpass(
                &SubpassInfo {
                    name: Some("[MESHI] DEFERRED SPLIT".to_string()),
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

                    let indices_handle = match indices {
                        ReservedBinding::TableBinding {
                            binding: _,
                            resources,
                        } => match resources[0].resource {
                            ShaderResource::StorageBuffer(view) => Some(view.handle),
                            _ => None,
                        },
                        _ => None,
                    };

                    let Some(indices_handle) = indices_handle else {
                        return cmd;
                    };

                    cmd = cmd
                        .bind_graphics_pipeline(self.psos.standard.handle)
                        .update_viewport(&self.data.viewport)
                        .draw_indexed_indirect(&DrawIndexedIndirect {
                            indices: indices_handle,
                            indirect: self.proc.draw_builder.draw_list_for_bin(BIN_GBUFFER_OPAQUE),
                            bind_tables: self.psos.standard.tables(),
                            dynamic_buffers: [None, None, Some(alloc), None],
                            draw_count: self.proc.draw_builder.draw_count(),
                            ..Default::default()
                        })
                        .unbind_graphics_pipeline()
                        .combine(
                            self.subrender
                                .environment
                                .record_deferred_split(&subrender_info, indices_handle),
                        );

                    cmd
                },
            );

            ///////////////////////////////////////////////////////////////////
            ///////////////////////////////////////////////////////////////////
            // Deferred COMBINE pass. Combines all deferred attachments.     //
            ///////////////////////////////////////////////////////////////////
            ///////////////////////////////////////////////////////////////////
            let shadow_result = self.subrender.shadow.process(ShadowProcessInfo {
                subrenderer: SubrendererProcessInfo::new(
                    &mut self.graph,
                    self.state.as_ref(),
                    &mut self.data.dynamic,
                    camera_handle,
                    self.proc.draw_builder.draw_list_for_bin(BIN_SHADOW),
                    self.proc.draw_builder.draw_count(),
                ),
                view_idx: view_idx as u32,
                shadow_bin: BIN_SHADOW,
                primary_light_direction: self.subrender.environment.primary_light_direction(),
                spot_light: None,
            });

            self.graph.add_subpass(
                &SubpassInfo {
                    name: Some("[MESHI] DEFERRED COMBINE".to_string()),
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

                    #[repr(packed)]
                    struct PerObj {
                        pos: u32,
                        diff: u32,
                        norm: u32,
                        mat: u32,
                        shadow: u32,
                        shadow_cascade_count: u32,
                        shadow_resolution: u32,
                        debug_view: u32,
                        spot_shadow_texture: u32,
                        spot_shadow_resolution: u32,
                        spot_shadow_padding0: u32,
                        spot_shadow_padding1: u32,
                        spot_shadow_matrix: Mat4,
                    }

                    let per_obj = &mut alloc.slice::<PerObj>()[0];
                    per_obj.pos = position.bindless_id.unwrap_or(u16::MAX) as u32;
                    per_obj.diff = diffuse.bindless_id.unwrap_or(u16::MAX) as u32;
                    per_obj.norm = normal.bindless_id.unwrap_or(u16::MAX) as u32;
                    per_obj.mat = material_code.bindless_id.unwrap_or(u16::MAX) as u32;
                    per_obj.shadow = shadow_result
                        .cascaded
                        .shadow_map
                        .bindless_id
                        .unwrap_or(u16::MAX) as u32;
                    per_obj.shadow_cascade_count = shadow_result.cascaded.cascade_data.count;
                    per_obj.shadow_resolution = shadow_result.cascaded.shadow_resolution;
                    per_obj.debug_view =
                        self.subrender.environment.cloud_settings().debug_view as u32;
                    per_obj.spot_shadow_texture = shadow_result.spot.shadow_bindless_id;
                    per_obj.spot_shadow_resolution = shadow_result.spot.shadow_resolution;
                    per_obj.spot_shadow_padding0 = 0;
                    per_obj.spot_shadow_padding1 = 0;
                    per_obj.spot_shadow_matrix = shadow_result.spot.shadow_matrix;

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

            let deferred_debug_output = match self.debug_views.deferred_framebuffer {
                value if value == DeferredFramebufferDebugView::Position as u32 => {
                    Some(position.view)
                }
                value if value == DeferredFramebufferDebugView::Diffuse as u32 => {
                    Some(diffuse.view)
                }
                value if value == DeferredFramebufferDebugView::Normal as u32 => Some(normal.view),
                value if value == DeferredFramebufferDebugView::Material as u32 => {
                    Some(material_code.view)
                }
                _ => None,
            };
            let depth_debug_output = if self.debug_views.depth == DeferredDepthDebugView::On as u32
            {
                Some(depth)
            } else {
                None
            };
            let scene_color_view = scene_color.view;
            let final_combine_view = final_combine.view;
            let scene_width = self.data.viewport.area.w as u32;
            let scene_height = self.data.viewport.area.h as u32;

            self.graph.add_compute_pass(move |mut cmd| {
                cmd = cmd.blit_images(&BlitImage {
                    src: final_combine_view.img,
                    dst: scene_color_view.img,
                    src_range: SubresourceRange::new(0, 1, 0, 1),
                    dst_range: SubresourceRange::new(0, 1, 0, 1),
                    filter: Filter::Linear,
                    src_region: Rect2D {
                        x: 0,
                        y: 0,
                        w: scene_width,
                        h: scene_height,
                    },
                    dst_region: Rect2D {
                        x: 0,
                        y: 0,
                        w: scene_width,
                        h: scene_height,
                    },
                });
                cmd.end()
            });

            let overlay_text = if self.subrender.environment.cloud_settings().debug_view
                == CloudDebugView::Stats
            {
                self.subrender.environment.cloud_timing_overlay_text()
            } else {
                String::new()
            };

            ///////////////////////////////////////////////////////////////////
            ///////////////////////////////////////////////////////////////////
            // Transparent forward pass.                                      //
            ///////////////////////////////////////////////////////////////////
            ///////////////////////////////////////////////////////////////////

            struct BillboardDraw {
                vertex_buffer: Handle<Buffer>,
                material: Handle<Material>,
                scene_handle: Handle<SceneObject>,
                transform: Mat4,
            }

            let s = self as *mut Self;
            let mut billboard_draws = BumpVec::new_in(&self.frame_bump);
            let mut handles =
                BumpVec::with_capacity_in(self.data.objects.entries.len(), &self.frame_bump);
            handles.extend(self.data.objects.entries.iter().copied());
            for handle in handles {
                let (scene_handle, billboard) = {
                    let obj = self.data.objects.get_ref(handle);
                    let RenderObjectKind::Billboard(billboard) = &obj.kind else {
                        continue;
                    };
                    (obj.scene_handle, billboard.clone())
                };

                if let Some(material) = billboard.info.material {
                    let transform = self.proc.scene.get_object_transform(scene_handle);
                    unsafe {
                        update_billboard_vertices(
                            (*s).ctx.as_mut(),
                            (*s).state.as_mut(),
                            &billboard,
                            transform,
                            camera_handle,
                        );
                    }
                    billboard_draws.push(BillboardDraw {
                        vertex_buffer: billboard.vertex_buffer,
                        material,
                        scene_handle,
                        transform,
                    });
                }
            }

            let mut transparent_attachments: [Option<ImageView>; 8] = [None; 8];
            transparent_attachments[0] = Some(final_combine.view);
            let transparent_clear: [Option<ClearValue>; 8] = [None; 8];

            self.graph.add_subpass(
                &SubpassInfo {
                    name: Some("[MESHI] TRANSPARENT".to_string()),
                    viewport: self.data.viewport,
                    color_attachments: transparent_attachments,
                    depth_attachment: Some(depth.view),
                    clear_values: transparent_clear,
                    depth_clear: None,
                },
                |mut cmd| {
                    cmd = cmd.combine(self.subrender.environment.render_opaque(
                        &mut subrender_info,
                        scene_color.bindless_id,
                        depth.bindless_id,
                        shadow_result.cascaded.shadow_map.bindless_id,
                        shadow_result.cascaded.shadow_resolution,
                        shadow_result.cascaded.cascade_data.count,
                        Vec4::from(shadow_result.cascaded.cascade_data.splits),
                        shadow_result.cascaded.cascade_data.matrices,
                    ));

                    if !billboard_draws.is_empty() {
                        let mut c = cmd
                            .bind_graphics_pipeline(self.psos.billboard.handle)
                            .update_viewport(&self.data.viewport);

                        for draw in billboard_draws.iter() {
                            let mut alloc = self
                                .data
                                .dynamic
                                .bump()
                                .expect("Failed to allocate billboard draw buffer!");
                            let per_obj = &mut alloc.slice::<PerObjectInfo>()[0];
                            per_obj.transform = draw.transform;
                            per_obj.scene_id = draw.scene_handle;
                            per_obj.material_id = draw.material;
                            per_obj.camera_id = camera_handle;
                            per_obj.skeleton_id = Handle::default();
                            per_obj.animation_state_id = Handle::default();
                            per_obj.per_obj_joints_id = Handle::default();

                            c = c.draw(&Draw {
                                vertices: draw.vertex_buffer,
                                bind_tables: self.psos.billboard.tables(),
                                dynamic_buffers: [None, Some(alloc), None, None],
                                instance_count: 1,
                                count: 6,
                            });
                        }

                        cmd = c.unbind_graphics_pipeline();
                    }

                    cmd = cmd.combine(
                        self.subrender
                            .environment
                            .render_fog(&self.data.viewport, camera_handle),
                    );

                    cmd = cmd.combine(
                        self.gui
                            .render_gui(&self.data.viewport, &mut self.data.dynamic),
                    );
                    cmd.combine(
                        self.text
                            .render_transparent(self.ctx.as_mut(), &self.data.viewport),
                    )
                },
            );

            let output_image = final_combine.view;
            outputs.push(ViewOutput {
                camera: *camera,
                image: output_image,
                semaphore: semaphores[0],
            });
        }

        self.graph.add_compute_pass(|mut cmd| {
            cmd.combine(self.proc.scene.post_compute())
                .combine(self.proc.draw_builder.post_compute())
                .combine(self.subrender.environment.post_compute())
                .combine(self.subrender.shadow.post_compute())
                .combine(self.gui.post_compute())
                .combine(self.text.post_compute())
                .sync(SyncPoint::ComputeToGraphics, Scope::AllCommonReads)
                // .combine(self.subrender.debug.record(self.ctx.as_mut(), self.state.as_ref(), outputs[0].image, [self.data.viewport.area.w as u32, self.data.viewport.area.h as u32]))
                .end()
        });

        let mut wait_sems = BumpVec::with_capacity_in(sems.len() + 1, &self.frame_bump);
        wait_sems.extend(sems.iter().copied());
        wait_sems.extend(prep_sems.iter().copied());

        self.graph.execute_with(&SubmitInfo {
            wait_sems: &wait_sems,
            signal_sems: &[semaphores[0]],
        });

        outputs
    }

    pub fn shut_down(self) {
        self.ctx.destroy();
    }

    pub fn set_terrain_rdb(&mut self, rdb: &mut RDBFile, project_key: &str) {
        self.subrender.environment.set_terrain_rdb(rdb, project_key);
    }

    pub fn set_terrain_project_key(&mut self, project_key: &str) {
        self.subrender
            .environment
            .set_terrain_project_key(project_key);
    }
}

impl Renderer for DeferredRenderer {
    fn viewport(&self) -> Viewport {
        self.data.viewport
    }

    fn context(&mut self) -> &'static mut Context {
        unsafe { &mut (*(self.ctx.as_mut() as *mut Context)) }
    }

    fn state(&mut self) -> &mut BindlessState {
        &mut self.state
    }

    fn initialize_database(&mut self, db: &mut DB) {
        DeferredRenderer::initialize_database(self, db);
    }

    fn set_skybox_cubemap(&mut self, cubemap: noren::rdb::imagery::DeviceCubemap) {
        self.subrender
            .environment
            .update_skybox(super::environment::sky::SkyboxFrameSettings {
                cubemap: Some(cubemap),
                use_procedural_cubemap: false,
                ..Default::default()
            });
    }

    fn set_skybox_settings(&mut self, settings: super::environment::sky::SkyboxFrameSettings) {
        self.subrender.environment.update_skybox(settings);
    }

    fn set_sky_settings(&mut self, settings: super::environment::sky::SkyFrameSettings) {
        self.subrender.environment.update_sky(settings);
    }

    fn set_ocean_settings(&mut self, settings: super::environment::ocean::OceanFrameSettings) {
        self.subrender.environment.update_ocean(settings);
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

    fn release_object(&mut self, handle: Handle<RenderObject>) {
        DeferredRenderer::release_object(self, handle);
    }

    fn register_text(&mut self, info: &TextInfo) -> Handle<TextObject> {
        DeferredRenderer::register_text(self, info)
    }

    fn register_gui(&mut self, info: &GuiInfo) -> Handle<GuiObject> {
        DeferredRenderer::register_gui(self, info)
    }

    fn release_text(&mut self, handle: Handle<TextObject>) {
        DeferredRenderer::release_text(self, handle);
    }

    fn release_gui(&mut self, handle: Handle<GuiObject>) {
        DeferredRenderer::release_gui(self, handle);
    }

    fn set_text(&mut self, handle: Handle<TextObject>, text: &str) {
        DeferredRenderer::set_text(self, handle, text);
    }

    fn set_text_info(&mut self, handle: Handle<TextObject>, info: &TextInfo) {
        DeferredRenderer::set_text_info(self, handle, info);
    }

    fn set_gui_info(&mut self, handle: Handle<GuiObject>, info: &GuiInfo) {
        DeferredRenderer::set_gui_info(self, handle, info);
    }

    fn set_gui_visibility(&mut self, handle: Handle<GuiObject>, visible: bool) {
        DeferredRenderer::set_gui_visibility(self, handle, visible);
    }

    fn upload_gui_frame(&mut self, frame: GuiFrame) {
        DeferredRenderer::upload_gui_frame(self, frame);
    }

    fn update(
        &mut self,
        sems: &[Handle<Semaphore>],
        views: &[Handle<Camera>],
        delta_time: f32,
    ) -> Vec<ViewOutput> {
        DeferredRenderer::update(self, sems, views, delta_time)
    }

    fn cloud_settings(&self) -> crate::CloudSettings {
        self.subrender.environment.cloud_settings()
    }

    fn set_cloud_settings(&mut self, settings: crate::CloudSettings) {
        self.subrender.environment.set_cloud_settings(settings);
    }

    fn set_cloud_weather_map(&mut self, view: Option<ImageView>) {
        self.subrender.environment.set_cloud_weather_map(view);
    }

    fn set_terrain_rdb(&mut self, rdb: &mut RDBFile, project_key: &str) {
        DeferredRenderer::set_terrain_rdb(self, rdb, project_key);
    }

    fn set_terrain_project_key(&mut self, project_key: &str) {
        DeferredRenderer::set_terrain_project_key(self, project_key);
    }

    fn set_terrain_render_settings(&mut self, settings: crate::TerrainRenderSettings) {
        self.subrender
            .environment
            .set_terrain_render_settings(settings);
    }

    fn shut_down(self: Box<Self>) {
        self.ctx.destroy();
    }
}
