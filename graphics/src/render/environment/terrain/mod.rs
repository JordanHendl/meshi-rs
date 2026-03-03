pub mod settings;
use self::settings::TerrainRenderSettings;
use super::EnvironmentRendererInfo;
use bento::builder::{AttachmentDesc, CSOBuilder, PSO, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use crossbeam_queue::SegQueue;
use dashi::cmd::{Executable, PendingGraphics};
use dashi::driver::command::{Dispatch, Draw, DrawIndexedIndirect};
use dashi::{
    Buffer, BufferInfo, BufferUsage, CommandStream, Context, DynamicAllocator,
    DynamicAllocatorState, Format, Handle, MemoryVisibility, SampleCount, ShaderResource,
    UsageBits, Viewport,
};
use furikake::BindlessState;
use furikake::PSOBuilderFurikakeExt;
use glam::{Mat4, Vec2, Vec3, Vec4};
use noren::DB;
use noren::RDBFile;
use noren::rdb::DeviceGeometryLayer;
use noren::rdb::primitives::Vertex;
use noren::rdb::terrain::{
    TerrainCameraInfo, TerrainChunk, TerrainChunkArtifact, TerrainFrustum, TerrainProjectSettings,
    chunk_artifact_entry, chunk_coord_key, lod_key, project_settings_entry,
};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tare::transient::BindlessTextureRegistry;
use tracing::{info, warn};

use crate::render::SubrendererDrawInfo;
use crate::render::deferred::BIN_TERRAIN;
use crate::render::deferred::PerDrawData;
use crate::render::utils::gpu_draw_builder::{GPUDrawBuilder, GPUDrawBuilderInfo};
use crate::terrain_loader;
use furikake::reservations::bindless_camera::ReservedBindlessCamera;
use furikake::reservations::bindless_indices::ReservedBindlessIndices;
use furikake::reservations::bindless_materials::ReservedBindlessMaterials;
use furikake::reservations::bindless_transformations::ReservedBindlessTransformations;
use furikake::reservations::bindless_vertices::ReservedBindlessVertices;
use furikake::types::{
    Camera, MATERIAL_FLAG_VERTEX_COLOR, Material, Transformation, VertexBufferSlot,
};
use tare::utils::StagedBuffer;

pub type TerrainInfo = TerrainRenderSettings;

pub const TERRAIN_DRAW_BIN: u32 = BIN_TERRAIN;
const TERRAIN_REFRESH_FRAME_INTERVAL: u64 = 4;
const TERRAIN_SETTINGS_POLL_INTERVAL: u64 = 30;
const TERRAIN_CAMERA_POSITION_EPSILON: f32 = 0.25;
const TERRAIN_VIEW_PROJECTION_EPSILON: f32 = 1e-3;
const TERRAIN_CAMERA_VELOCITY_EPSILON: f32 = 0.02;
const TERRAIN_MISSING_LOD_POLL_LIMIT: usize = 4;
const TERRAIN_UPDATE_BUDGET_PER_FRAME: usize = 12;
const TERRAIN_REFRESH_CHUNK_BUDGET_PER_FRAME: usize = 12;

#[derive(Clone, Copy, Default)]
pub struct TerrainFrameSettings {
    pub camera_position: Vec3,
    pub camera_far: f32,
    pub view_projection: Option<Mat4>,
}

#[derive(Clone, Debug)]
pub struct TerrainRenderObject {
    pub key: String,
    pub artifact: TerrainChunkArtifact,
    pub transform: Mat4,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ClipmapDescriptor {
    center: [f32; 2],
    level: u32,
    _padding: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TerrainComputeParams {
    camera_position: Vec3,
    lod_levels: u32,
    patch_size: f32,
    max_tiles: u32,
    clipmap_resolution: u32,
    height_scale: f32,
    _padding: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TerrainDrawParams {
    camera_position: Vec3,
    lod_levels: u32,
    patch_size: f32,
    max_tiles: u32,
    clipmap_resolution: u32,
    height_scale: f32,
    _padding: [f32; 2],
}

pub struct TerrainRenderer {
    deferred: Option<TerrainDeferredResources>,
    terrain_rdb: Option<NonNull<RDBFile>>,
    context: Option<NonNull<Context>>,
    settings: TerrainRenderSettings,
    project_key: String,
    enabled: bool,
    //    compute_pipeline: Option<bento::builder::CSO>,
    //    clipmap_buffer: Handle<Buffer>,
    //    draw_args_buffer: Handle<Buffer>,
    //    instance_buffer: Handle<Buffer>,
    //    heightmap_buffer: Handle<Buffer>,
    //    meshlet_buffer: Handle<Buffer>,
    //    patch_size: f32,
    //    lod_levels: u32,
    //    clipmap_resolution: u32,
    //    max_tiles: u32,
    //    enabled: bool,
    //    clipmap_surface_tile_resolution: [u32; 2],
    //    clipmap_material_tile_resolution: [u32; 2],
    //    camera_position: Vec3,
    //    camera_far: f32,
    //    frustum_planes: Option<[Vec4; 6]>,
    //    view_projection: Option<Mat4>,
    //    use_depth: bool,
    //    static_geometry: Option<TerrainStaticGeometry>,
    //    lod_sources: HashMap<TerrainChunkKey, Vec<TerrainRenderObject>>,
    //    active_chunk_lods: HashMap<TerrainChunkKey, String>,
    //    active_chunk_lod_levels: HashMap<TerrainChunkKey, u8>,
    //    terrain_project_key: Option<String>,
    //    terrain_settings: Option<TerrainProjectSettings>,
    //    terrain_settings_dirty: bool,
    //    terrain_render_objects: HashMap<String, TerrainRenderObject>,
    //    terrain_dirty: bool,
    //    refresh_frame_index: u64,
    //    last_refresh_frame: u64,
    //    last_settings_poll_frame: u64,
    //    last_frame_camera_position: Option<Vec3>,
    //    last_refresh_camera_position: Option<Vec3>,
    //    last_refresh_view_projection: Option<Mat4>,
    //    last_refresh_chunk_coords: Option<[i32; 2]>,
    //    last_base_chunk_hashes: HashMap<TerrainChunkKey, u64>,
    //    missing_lod_artifacts: HashSet<String>,
    //    visibility_cache_selection_hash: u64,
    //    visibility_cache_view_projection: Option<Mat4>,
    //    visibility_cache_camera_position: Option<Vec3>,
    //    visibility_cache_camera_far: f32,
    //    visibility_cache_keys: HashSet<String>,
    //    camera_velocity: f32,
    //    pending_selection: Option<Vec<TerrainRenderObject>>,
    //    pending_selection_keys: HashMap<TerrainChunkKey, String>,
    //    pending_selection_lod_levels: HashMap<TerrainChunkKey, u8>,
    //    pending_refresh: Option<PendingTerrainRefresh>,
    //    texture_data_cache: HashMap<TerrainTextureCacheKey, Arc<Vec<f32>>>,
    //    texture_work_requests: Arc<SegQueue<TerrainTextureWorkItem>>,
    //    texture_work_results: Arc<SegQueue<TerrainTextureBuildResult>>,
    //    texture_work_pending: HashSet<TerrainTextureWorkKey>,
    //    texture_worker_running: Arc<AtomicBool>,
    //    texture_worker_handle: Option<JoinHandle<()>>,
    //    clipmap_buffers: Option<TerrainClipmapBuffers>,
    //    clipmap_slots: Vec<Option<TerrainClipmapSlot>>,
    //    clipmap_buffers_dirty: bool,
    //    clipmap_cache_dirty: bool,
    //    deferred_sample_count: Option<SampleCount>,
    //    deferred_dynamic_state: Option<DynamicAllocatorState>,
    //    pending_selection_tiles: HashMap<TerrainChunkKey, u32>,
    //    active_selection_tiles: HashMap<TerrainChunkKey, u32>,
}

#[derive(Clone)]
struct TerrainObjectEntry {
    transform_handle: Handle<Transformation>,
    draws: Vec<Handle<PerDrawData>>,
    draw_instances: Vec<TerrainDrawInstance>,
    content_hash: u64,
    material_handle: Handle<Material>,
    clipmap_tile_index: u32,
}

#[derive(Clone)]
struct TerrainDrawInstance {
    material: Handle<Material>,
    vertex_id: u32,
    vertex_count: u32,
    index_id: u32,
    index_count: u32,
    clipmap_tile_index: u32,
}

#[derive(Clone, Copy)]
struct TerrainPlaneGeometry {
    vertex_id: u32,
    vertex_count: u32,
    index_id: u32,
    index_count: u32,
}

struct TerrainStaticGeometry {
    lods: Vec<TerrainPlaneGeometry>,
    settings_hash: u64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TerrainGeometryCacheKey {
    state_id: usize,
    tiles_per_chunk: [u32; 2],
    tile_size_bits: u32,
    lod: u8,
    lod_levels: u8,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TerrainTextureCacheKey {
    kind: &'static str,
    hash: u64,
    grid: [u32; 2],
}

struct TerrainClipmapBuffers {
    surface_grid_size: [u32; 2],
    material_grid_size: [u32; 2],
    surface_tile_texel_count: u32,
    material_tile_texel_count: u32,
    tile_count: u32,
    height: StagedBuffer,
    normal: StagedBuffer,
    blend: StagedBuffer,
    blend_ids: StagedBuffer,
    hole_mask: StagedBuffer,
}

#[derive(Clone, Copy)]
struct TerrainClipmapSlot {
    hash: u64,
    lod: u8,
    grid_size: [u32; 2],
}

struct PendingTerrainRefresh {
    kind: PendingRefreshKind,
}

enum PendingRefreshKind {
    Full(PendingFullRefresh),
    Delta(PendingDeltaRefresh),
}

struct PendingFullRefresh {
    project_key: String,
    settings: TerrainProjectSettings,
    base_artifacts: Vec<TerrainChunkArtifact>,
    next_base_hashes: HashMap<TerrainChunkKey, u64>,
    lod_levels: u8,
    next_objects: HashMap<String, TerrainRenderObject>,
    ordered_objects: Vec<TerrainRenderObject>,
    loaded_artifacts: usize,
    index: usize,
}

struct PendingDeltaRefresh {
    project_key: String,
    settings: TerrainProjectSettings,
    updates: Vec<(TerrainChunkKey, TerrainChunkArtifact)>,
    removals: Vec<TerrainChunkKey>,
    next_base_hashes: HashMap<TerrainChunkKey, u64>,
    lod_levels: u8,
    updated_chunks: usize,
    loaded_artifacts: usize,
    update_index: usize,
    removal_index: usize,
}

struct TerrainEntryBuild {
    draw_instances: Vec<TerrainDrawInstance>,
    material_handle: Handle<Material>,
    clipmap_tile_index: u32,
}

struct TerrainDeferredResources {
    pipeline: PSO,
    draw_builder: Option<NonNull<GPUDrawBuilder>>,
    objects: HashMap<String, TerrainObjectEntry>,
    db: Option<NonNull<DB>>,
    sample_count: SampleCount,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
struct TerrainChunkKey {
    project_key: String,
    coords: [i32; 2],
}

fn compile_terrain_shaders() -> [bento::CompilationResult; 2] {
    let compiler = Compiler::new().expect("Failed to create shader compiler");
    let base_request = Request {
        name: Some("environment_terrain".to_string()),
        lang: ShaderLang::Glsl,
        optimization: OptimizationLevel::Performance,
        debug_symbols: true,
        ..Default::default()
    };

    let vertex = compiler
        .compile(
            include_str!("../shaders/environment_terrain.vert.glsl").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Vertex,
                ..base_request.clone()
            },
        )
        .expect("Failed to compile terrain vertex shader");

    let fragment = compiler
        .compile(
            include_str!("../shaders/environment_terrain.frag.glsl").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Fragment,
                ..base_request
            },
        )
        .expect("Failed to compile terrain fragment shader");

    [vertex, fragment]
}

fn compile_terrain_deferred_shaders() -> [bento::CompilationResult; 2] {
    let compiler = Compiler::new().expect("Failed to create shader compiler");
    let base_request = Request {
        name: Some("environment_terrain_deferred".to_string()),
        lang: ShaderLang::Slang,
        optimization: OptimizationLevel::Performance,
        debug_symbols: true,
        ..Default::default()
    };

    let vertex = compiler
        .compile(
            include_str!("../shaders/terrain_deferred_vert.slang").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Vertex,
                ..base_request.clone()
            },
        )
        .expect("Failed to compile terrain deferred vertex shader");

    let fragment = compiler
        .compile(
            include_str!("../shaders/terrain_deferred_frag.slang").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Fragment,
                ..base_request
            },
        )
        .expect("Failed to compile terrain deferred fragment shader");

    [vertex, fragment]
}

impl TerrainRenderer {
    pub fn pre_compute(&mut self) -> CommandStream<Executable> {
        if !self.enabled {
            return CommandStream::new().begin().end();
        }
        let mut stream = CommandStream::new().begin();
        //        if let Some(sync) = self.sync_clipmap_buffers() {
        //            stream = stream.combine(sync);
        //        }
        stream.end()
    }

    pub fn post_compute(&mut self) -> CommandStream<Executable> {
        if !self.enabled {
            return CommandStream::new().begin().end();
        }
        let mut stream = CommandStream::new().begin();
        stream.end()
    }

    pub fn set_render_settings(&mut self, settings: TerrainRenderSettings) {
        self.settings = settings;
    }

    pub fn new_deferred(
        ctx: &mut Context,
        state: &mut BindlessState,
        draw: &mut GPUDrawBuilder,
        info: &EnvironmentRendererInfo,
        dynamic: &DynamicAllocator,
    ) -> Self {
        let clipmaps = todo!();
        Self {
            deferred: Some(TerrainDeferredResources {
                pipeline: Self::build_deferred_pipeline(ctx, state, info.sample_count, draw, dynamic.state(), clipmaps),
                draw_builder: None,
                objects: Default::default(),
                db: Default::default(),
                sample_count: info.sample_count,
            }),
            terrain_rdb: None,
            context: None,
            settings: TerrainRenderSettings::default(),
            project_key: String::new(),
            enabled: false,
        }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn initialize_draws(&mut self, draw_builder: &mut GPUDrawBuilder) {
        let Some(deferred) = self.deferred.as_mut() else {
            return;
        };

        deferred.draw_builder = Some(NonNull::from(draw_builder));
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        if let Some(deferred) = &mut self.deferred {
            deferred.db = Some(NonNull::new(db).expect("terrain db"));
        }
    }

    pub fn set_rdb(&mut self, rdb: &mut RDBFile, project_key: &str) {
        self.terrain_rdb = Some(NonNull::new(rdb).expect("terrain rdb"));
        self.project_key = project_key.to_string();
    }

    pub fn set_project_key(&mut self, project_key: &str) {
        self.project_key = project_key.to_string();
    }

    pub fn update(&mut self, camera: Handle<Camera>, state: &mut BindlessState) {
        if !self.enabled {
            return;
        }
    }

    fn load_terrain_settings(
        &mut self,
        project_key: &str,
        db: &mut DB,
    ) -> Option<TerrainProjectSettings> {
        let settings_entry = project_settings_entry(project_key);
        match db.terrain_mut().fetch_project_settings(&settings_entry) {
            Ok(settings) => Some(settings),
            Err(err) => {
                warn!(
                    "Failed to load terrain project settings '{}': {err:?}",
                    settings_entry
                );
                if let Some(mut rdb_ptr) = self.terrain_rdb {
                    unsafe { rdb_ptr.as_mut() }
                        .fetch::<TerrainProjectSettings>(&settings_entry)
                        .ok()
                } else {
                    None
                }
            }
        }
    }

    pub fn record_deferred_draws(
        &mut self,
        info: &SubrendererDrawInfo,
        indices_handle: Handle<Buffer>,
    ) -> CommandStream<PendingGraphics> {
        if !self.enabled {
            return CommandStream::<PendingGraphics>::subdraw();
        }
        let Some(deferred) = &mut self.deferred else {
            return CommandStream::<PendingGraphics>::subdraw();
        };

        #[repr(C)]
        struct PerSceneData {
            camera: Handle<Camera>,
            surface_grid_size: [u32; 2],
            surface_tile_texel_count: u32,
            _padding0: u32,
            material_grid_size: [u32; 2],
            material_tile_texel_count: u32,
            _padding1: u32,
        }

        let mut alloc = info
            .alloc
            .bump()
            .expect("Failed to allocate terrain per-scene data");

        //        alloc.slice::<PerSceneData>()[0] = PerSceneData {
        //            camera: info.camera,
        //            surface_grid_size,
        //            surface_tile_texel_count,
        //            _padding0: 0,
        //            material_grid_size,
        //            material_tile_texel_count,
        //            _padding1: 0,
        //        };

        let mut stream = CommandStream::<PendingGraphics>::subdraw();
        stream
            .bind_graphics_pipeline(deferred.pipeline.handle)
            .update_viewport(&info.viewport)
            .draw_indexed_indirect(&DrawIndexedIndirect {
                indices: indices_handle,
                indirect: info.draw_builder.draw_list_for_bin(TERRAIN_DRAW_BIN).into(),
                bind_tables: deferred.pipeline.tables(),
                dynamic_buffers: [Some(alloc), None, None, None],
                draw_count: info.draw_builder.draw_count(),
                ..Default::default()
            })
            .unbind_graphics_pipeline()
    }

    fn build_deferred_pipeline(
        ctx: &mut Context,
        state: &mut BindlessState,
        sample_count: SampleCount,
        draw_builder: &GPUDrawBuilder,
        dynamic_state: DynamicAllocatorState,
        clipmap_buffers: &TerrainClipmapBuffers,
    ) -> PSO {
        let shaders = compile_terrain_deferred_shaders();

        let s = PSOBuilder::new()
            .set_debug_name("[MESHI] Deferred Terrain")
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .set_attachment_format(0, Format::RGBA32F)
            .set_attachment_format(1, Format::RGBA8)
            .set_attachment_format(2, Format::RGBA32F)
            .set_attachment_format(3, Format::RGBA8)
            .add_table_variable_with_resources(
                "per_draw_ssbo",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::StorageBuffer(draw_builder.per_draw_data().into()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "per_scene_ssbo",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::DynamicStorage(dynamic_state),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "terrain_height_buffer",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::StorageBuffer(clipmap_buffers.height.device().into()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "terrain_normal_buffer",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::StorageBuffer(clipmap_buffers.normal.device().into()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "terrain_blend_buffer",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::StorageBuffer(clipmap_buffers.blend.device().into()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "terrain_blend_ids_buffer",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::StorageBuffer(
                        clipmap_buffers.blend_ids.device().into(),
                    ),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "terrain_hole_mask_buffer",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::StorageBuffer(
                        clipmap_buffers.hole_mask.device().into(),
                    ),
                    slot: 0,
                }],
            )
            .add_reserved_table_variables(state)
            .unwrap()
            .add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: sample_count,
            })
            .set_details(dashi::GraphicsPipelineDetails {
                color_blend_states: vec![Default::default(); 4],
                sample_count,
                depth_test: Some(dashi::DepthInfo {
                    should_test: true,
                    should_write: true,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .build(ctx)
            .expect("Failed to build terrain pipeline!");

        state.register_pso_tables(&s);
        s
    }

    fn release_terrain_draws(entry: &mut TerrainObjectEntry, draw_builder: &mut GPUDrawBuilder) {
        for draw in entry.draws.drain(..) {
            draw_builder.release_draw(draw);
        }
    }

    fn register_draw_instances(
        &mut self,
        instances: &[TerrainDrawInstance],
        transform_handle: Handle<Transformation>,
        draw_builder: &mut GPUDrawBuilder,
    ) -> Vec<Handle<PerDrawData>> {
        instances
            .iter()
            .map(|instance| {
                draw_builder.register_draw(&PerDrawData::terrain_draw(
                    Handle::default(),
                    transform_handle,
                    instance.material,
                    instance.vertex_id,
                    instance.vertex_count,
                    instance.index_id,
                    instance.index_count,
                    instance.clipmap_tile_index,
                ))
            })
            .collect()
    }

    fn static_geometry_hash(settings: &TerrainProjectSettings, lod_levels: u32) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        settings.tiles_per_chunk.hash(&mut hasher);
        settings.tile_size.to_bits().hash(&mut hasher);
        lod_levels.hash(&mut hasher);
        hasher.finish()
    }

    fn build_plane_geometry(
        &mut self,
        settings: &TerrainProjectSettings,
        state: &mut BindlessState,
        lod: u8,
        lod_levels: u8,
    ) -> Option<TerrainPlaneGeometry> {
        let tiles_x = settings.tiles_per_chunk[0].max(1);
        let tiles_y = settings.tiles_per_chunk[1].max(1);
        let base_step = 1u32.checked_shl(lod as u32).unwrap_or(1).max(1);
        let max_step = tiles_x.max(tiles_y);
        let step = if lod + 1 == lod_levels {
            max_step
        } else {
            base_step.min(max_step)
        };
        let step_x = step.min(tiles_x).max(1);
        let step_y = step.min(tiles_y).max(1);
        let grid_x = (tiles_x / step_x).max(1) + 1;
        let grid_y = (tiles_y / step_y).max(1) + 1;

        let spacing_x = settings.tile_size * step_x as f32;
        let spacing_y = settings.tile_size * step_y as f32;

        let mut vertices = Vec::with_capacity((grid_x * grid_y) as usize);
        for y in 0..grid_y {
            for x in 0..grid_x {
                let position = [x as f32 * spacing_x, 0.0, y as f32 * spacing_y];
                let uv = [
                    x as f32 / (grid_x.saturating_sub(1).max(1)) as f32,
                    y as f32 / (grid_y.saturating_sub(1).max(1)) as f32,
                ];
                vertices.push(Vertex {
                    position,
                    normal: [0.0, 1.0, 0.0],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    uv,
                    color: [1.0, 1.0, 1.0, 1.0],
                    joint_indices: [0; 4],
                    joint_weights: [0.0; 4],
                });
            }
        }

        let mut indices = Vec::new();
        for y in 0..grid_y.saturating_sub(1) {
            for x in 0..grid_x.saturating_sub(1) {
                let i0 = (y * grid_x + x) as u32;
                let i1 = (y * grid_x + x + 1) as u32;
                let i2 = ((y + 1) * grid_x + x) as u32;
                let i3 = ((y + 1) * grid_x + x + 1) as u32;
                indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
            }
        }

        if lod + 1 < lod_levels {
            let skirt_depth = spacing_x.max(spacing_y);
            let mut add_skirt_edge =
                |edge: &[u32], vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>| {
                    if edge.len() < 2 {
                        return;
                    }
                    let skirt_start = vertices.len() as u32;
                    for &base in edge {
                        let mut skirt_vertex = vertices[base as usize];
                        skirt_vertex.position[1] -= skirt_depth;
                        vertices.push(skirt_vertex);
                    }
                    for i in 0..edge.len() - 1 {
                        let b0 = edge[i];
                        let b1 = edge[i + 1];
                        let s0 = skirt_start + i as u32;
                        let s1 = skirt_start + i as u32 + 1;
                        indices.extend_from_slice(&[b0, s0, b1, b1, s0, s1]);
                        indices.extend_from_slice(&[b0, b1, s0, b1, s1, s0]);
                    }
                };

            let mut top_edge = Vec::with_capacity(grid_x as usize);
            let mut bottom_edge = Vec::with_capacity(grid_x as usize);
            for x in 0..grid_x {
                top_edge.push(x);
                bottom_edge.push((grid_y - 1) * grid_x + x);
            }

            let mut left_edge = Vec::with_capacity(grid_y as usize);
            let mut right_edge = Vec::with_capacity(grid_y as usize);
            for y in 0..grid_y {
                left_edge.push(y * grid_x);
                right_edge.push(y * grid_x + grid_x - 1);
            }

            add_skirt_edge(&top_edge, &mut vertices, &mut indices);
            add_skirt_edge(&bottom_edge, &mut vertices, &mut indices);
            add_skirt_edge(&left_edge, &mut vertices, &mut indices);
            add_skirt_edge(&right_edge, &mut vertices, &mut indices);
        }

        let mut layer = DeviceGeometryLayer::default();
        if !self.register_furikake_geometry_layer(&mut layer, &vertices, Some(&indices), state) {
            return None;
        }

        Some(TerrainPlaneGeometry {
            vertex_id: layer.furikake_vertex_id?,
            vertex_count: vertices.len() as u32,
            index_id: layer.furikake_index_id?,
            index_count: indices.len() as u32,
        })
    }

    fn register_furikake_geometry_layer(
        &mut self,
        layer: &mut DeviceGeometryLayer,
        vertices: &[Vertex],
        indices: Option<&[u32]>,
        state: &mut BindlessState,
    ) -> bool {
        let vertex_bytes = bytemuck::cast_slice(vertices);
        if vertex_bytes.is_empty() {
            layer.furikake_vertex_id = None;
        } else {
            let mut inserted_offset = None;
            let slot = Self::vertex_buffer_slot(vertices);
            if let Err(err) = state.reserved_mut::<ReservedBindlessVertices, _>(
                "meshi_bindless_vertices",
                |buffer| {
                    inserted_offset = buffer.push_vertex_bytes(slot, vertex_bytes);
                },
            ) {
                warn!("Failed to reserve terrain vertices: {err:?}");
                return false;
            }

            let Some(offset) = inserted_offset else {
                warn!("Failed to allocate bindless terrain vertices.");
                return false;
            };
            layer.furikake_vertex_id = Some(offset);
        }

        if let Some(indices) = indices.filter(|indices| !indices.is_empty()) {
            let mut inserted_offset = None;
            if let Err(err) = state.reserved_mut::<ReservedBindlessIndices, _>(
                "meshi_bindless_indices",
                |buffer| {
                    inserted_offset = buffer.push_indices(indices);
                },
            ) {
                warn!("Failed to reserve terrain indices: {err:?}");
                return false;
            }

            let Some(offset) = inserted_offset else {
                warn!("Failed to allocate bindless terrain indices.");
                return false;
            };
            layer.furikake_index_id = Some(offset);
        } else {
            layer.furikake_index_id = None;
        }

        true
    }

    fn vertex_buffer_slot(vertices: &[Vertex]) -> VertexBufferSlot {
        let _ = vertices;
        VertexBufferSlot::Skeleton
    }
}
