use super::EnvironmentRendererInfo;
use bento::builder::{AttachmentDesc, CSOBuilder, PSO, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use dashi::cmd::{Executable, PendingGraphics};
use dashi::driver::command::{Dispatch, Draw, DrawIndexedIndirect};
use dashi::{
    Buffer, BufferInfo, BufferUsage, CommandStream, Context, DynamicAllocator, Format, Handle,
    MemoryVisibility, SampleCount, ShaderResource, UsageBits, Viewport,
};
use furikake::BindlessState;
use furikake::PSOBuilderFurikakeExt;
use glam::{Mat4, Vec2, Vec3, Vec4};
use noren::DB;
use noren::RDBFile;
use noren::rdb::primitives::Vertex;
use noren::rdb::terrain::{
    TerrainCameraInfo, TerrainChunk, TerrainChunkArtifact, TerrainFrustum, TerrainProjectSettings,
    chunk_artifact_entry, chunk_coord_key, lod_key, project_settings_entry,
};
use noren::rdb::DeviceGeometryLayer;
use tare::transient::BindlessTextureRegistry;
use crossbeam_queue::SegQueue;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::{info, warn};

use crate::render::deferred::PerDrawData;
use crate::render::gpu_draw_builder::{GPUDrawBuilder, GPUDrawBuilderInfo};
use crate::terrain_loader;
use tare::utils::StagedBuffer;
use furikake::reservations::bindless_camera::ReservedBindlessCamera;
use furikake::reservations::bindless_indices::ReservedBindlessIndices;
use furikake::reservations::bindless_materials::ReservedBindlessMaterials;
use furikake::reservations::bindless_transformations::ReservedBindlessTransformations;
use furikake::reservations::bindless_vertices::ReservedBindlessVertices;
use furikake::types::{
    Camera, MATERIAL_FLAG_VERTEX_COLOR, Material, Transformation, VertexBufferSlot,
};

#[derive(Clone, Copy)]
pub struct TerrainInfo {
    pub patch_size: f32,
    pub lod_levels: u32,
    pub clipmap_resolution: u32,
    pub max_tiles: u32,
    pub clipmap_tile_resolution: [u32; 2],
}

pub const TERRAIN_DRAW_BIN: u32 = 0;
const TERRAIN_REFRESH_FRAME_INTERVAL: u64 = 4;
const TERRAIN_SETTINGS_POLL_INTERVAL: u64 = 30;
const TERRAIN_CAMERA_POSITION_EPSILON: f32 = 0.25;
const TERRAIN_VIEW_PROJECTION_EPSILON: f32 = 1e-3;
const TERRAIN_CAMERA_VELOCITY_EPSILON: f32 = 0.02;
const TERRAIN_MISSING_LOD_POLL_LIMIT: usize = 4;
const TERRAIN_UPDATE_BUDGET_PER_FRAME: usize = 12;
const TERRAIN_REFRESH_CHUNK_BUDGET_PER_FRAME: usize = 12;

impl Default for TerrainInfo {
    fn default() -> Self {
        let clipmap_resolution = 8;
        Self {
            patch_size: 64.0,
            lod_levels: 4,
            clipmap_resolution,
            max_tiles: clipmap_resolution * clipmap_resolution,
            clipmap_tile_resolution: [65, 65],
        }
    }
}

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

#[derive(Clone, Copy)]
pub struct TerrainDrawInfo {
    pub per_draw_data: Handle<Buffer>,
    pub draw_list: Handle<Buffer>,
    pub draw_count: u32,
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
    pipeline: PSO,
    compute_pipeline: Option<bento::builder::CSO>,
    clipmap_buffer: Handle<Buffer>,
    draw_args_buffer: Handle<Buffer>,
    instance_buffer: Handle<Buffer>,
    heightmap_buffer: Handle<Buffer>,
    meshlet_buffer: Handle<Buffer>,
    patch_size: f32,
    lod_levels: u32,
    clipmap_resolution: u32,
    max_tiles: u32,
    camera_position: Vec3,
    camera_far: f32,
    frustum_planes: Option<[Vec4; 6]>,
    view_projection: Option<Mat4>,
    use_depth: bool,
    deferred: Option<TerrainDeferredResources>,
    static_geometry: Option<TerrainStaticGeometry>,
    lod_sources: HashMap<TerrainChunkKey, Vec<TerrainRenderObject>>,
    active_chunk_lods: HashMap<TerrainChunkKey, String>,
    active_chunk_lod_levels: HashMap<TerrainChunkKey, u8>,
    context: Option<NonNull<Context>>,
    terrain_rdb: Option<NonNull<RDBFile>>,
    terrain_project_key: Option<String>,
    terrain_settings: Option<TerrainProjectSettings>,
    terrain_settings_dirty: bool,
    terrain_render_objects: HashMap<String, TerrainRenderObject>,
    terrain_dirty: bool,
    refresh_frame_index: u64,
    last_refresh_frame: u64,
    last_settings_poll_frame: u64,
    last_frame_camera_position: Option<Vec3>,
    last_refresh_camera_position: Option<Vec3>,
    last_refresh_view_projection: Option<Mat4>,
    last_refresh_chunk_coords: Option<[i32; 2]>,
    last_base_chunk_hashes: HashMap<TerrainChunkKey, u64>,
    missing_lod_artifacts: HashSet<String>,
    visibility_cache_selection_hash: u64,
    visibility_cache_view_projection: Option<Mat4>,
    visibility_cache_camera_position: Option<Vec3>,
    visibility_cache_camera_far: f32,
    visibility_cache_keys: HashSet<String>,
    camera_velocity: f32,
    pending_selection: Option<Vec<TerrainRenderObject>>,
    pending_selection_keys: HashMap<TerrainChunkKey, String>,
    pending_selection_lod_levels: HashMap<TerrainChunkKey, u8>,
    pending_refresh: Option<PendingTerrainRefresh>,
    texture_data_cache: HashMap<TerrainTextureCacheKey, Arc<Vec<f32>>>,
    texture_work_requests: Arc<SegQueue<TerrainTextureWorkItem>>,
    texture_work_results: Arc<SegQueue<TerrainTextureBuildResult>>,
    texture_work_pending: HashSet<TerrainTextureWorkKey>,
    texture_worker_running: Arc<AtomicBool>,
    texture_worker_handle: Option<JoinHandle<()>>,
    clipmap_buffers: Option<TerrainClipmapBuffers>,
    clipmap_slots: Vec<Option<TerrainClipmapSlot>>,
    clipmap_buffers_dirty: bool,
    clipmap_cache_dirty: bool,
    pending_selection_tiles: HashMap<TerrainChunkKey, u32>,
    active_selection_tiles: HashMap<TerrainChunkKey, u32>,
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TerrainTextureWorkKey {
    hash: u64,
    grid: [u32; 2],
}

#[derive(Clone)]
struct TerrainTextureWorkItem {
    artifact: Arc<TerrainChunkArtifact>,
    work_key: TerrainTextureWorkKey,
}

struct TerrainTextureBuildResult {
    work_key: TerrainTextureWorkKey,
    textures: Vec<(TerrainTextureCacheKey, Arc<Vec<f32>>)>,
}

struct TerrainClipmapBuffers {
    grid_size: [u32; 2],
    tile_texel_count: u32,
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
    draw_builder: GPUDrawBuilder,
    pipeline: PSO,
    objects: HashMap<String, TerrainObjectEntry>,
    db: Option<NonNull<DB>>,
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
            include_str!("shaders/environment_terrain.vert.glsl").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Vertex,
                ..base_request.clone()
            },
        )
        .expect("Failed to compile terrain vertex shader");

    let fragment = compiler
        .compile(
            include_str!("shaders/environment_terrain.frag.glsl").as_bytes(),
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
            include_str!("shaders/terrain_deferred_vert.slang").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Vertex,
                ..base_request.clone()
            },
        )
        .expect("Failed to compile terrain deferred vertex shader");

    let fragment = compiler
        .compile(
            include_str!("shaders/terrain_deferred_frag.slang").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Fragment,
                ..base_request
            },
        )
        .expect("Failed to compile terrain deferred fragment shader");

    [vertex, fragment]
}

impl TerrainRenderer {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        info: &EnvironmentRendererInfo,
        dynamic: &DynamicAllocator,
    ) -> Self {
        let texture_work_requests = Arc::new(SegQueue::new());
        let texture_work_results = Arc::new(SegQueue::new());
        let texture_worker_running = Arc::new(AtomicBool::new(true));
        let texture_worker_handle = Some(Self::spawn_texture_worker(
            Arc::clone(&texture_work_requests),
            Arc::clone(&texture_work_results),
            Arc::clone(&texture_worker_running),
        ));

        let terrain_info = info.terrain;
        let mut clipmap_descs = Vec::with_capacity(terrain_info.lod_levels as usize);
        for level in 0..terrain_info.lod_levels {
            clipmap_descs.push(ClipmapDescriptor {
                center: [0.0, 0.0],
                level,
                _padding: 0,
            });
        }

        let clipmap_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[MESHI GFX TERRAIN] Clipmap Buffer",
                byte_size: (std::mem::size_of::<ClipmapDescriptor>() * clipmap_descs.len()) as u32,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: Some(unsafe { clipmap_descs.align_to::<u8>().1 }),
            })
            .expect("Failed to create terrain clipmap buffer");

        let draw_args_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[MESHI GFX TERRAIN] Draw Args",
                byte_size: 16 * terrain_info.max_tiles.max(1),
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            })
            .expect("Failed to create terrain draw args buffer");

        let instance_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[MESHI GFX TERRAIN] Instance Data",
                byte_size: 32 * terrain_info.max_tiles.max(1),
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            })
            .expect("Failed to create terrain instance buffer");

        let heightmap_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[MESHI GFX TERRAIN] Heightmap Buffer",
                byte_size: 4 * terrain_info.clipmap_resolution * terrain_info.clipmap_resolution,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            })
            .expect("Failed to create terrain heightmap buffer");

        let meshlet_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[MESHI GFX TERRAIN] Meshlet Buffer",
                byte_size: 16 * terrain_info.clipmap_resolution * terrain_info.clipmap_resolution,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            })
            .expect("Failed to create terrain meshlet buffer");

        let clipmap_buffers = if terrain_info.clipmap_tile_resolution[0] > 0
            && terrain_info.clipmap_tile_resolution[1] > 0
        {
            Some(Self::allocate_clipmap_buffers(
                ctx,
                terrain_info.clipmap_resolution,
                terrain_info.clipmap_tile_resolution,
            ))
        } else {
            None
        };
        let clipmap_slots = clipmap_buffers
            .as_ref()
            .map(|buffers| vec![None; buffers.tile_count as usize])
            .unwrap_or_default();
        let clipmap_buffers_dirty = clipmap_buffers.is_some();

        let compute_pipeline = Some(
            CSOBuilder::new()
                .shader(Some(
                    include_str!("shaders/environment_terrain.comp.glsl").as_bytes(),
                ))
                .add_variable(
                    "clipmap",
                    ShaderResource::StorageBuffer(clipmap_buffer.into()),
                )
                .add_variable(
                    "draw_args",
                    ShaderResource::StorageBuffer(draw_args_buffer.into()),
                )
                .add_variable(
                    "instance_data",
                    ShaderResource::StorageBuffer(instance_buffer.into()),
                )
                .add_variable("params", ShaderResource::DynamicStorage(dynamic.state()))
                .add_variable(
                    "heightmap",
                    ShaderResource::StorageBuffer(heightmap_buffer.into()),
                )
                .add_variable(
                    "meshlets",
                    ShaderResource::StorageBuffer(meshlet_buffer.into()),
                )
                .build(ctx)
                .unwrap(),
        );

        let shaders = compile_terrain_shaders();
        let mut pso_builder = PSOBuilder::new()
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .set_attachment_format(0, info.color_format)
            .add_table_variable_with_resources(
                "instance_data",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::StorageBuffer(instance_buffer.into()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "params",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::DynamicStorage(dynamic.state()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "heightmap",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::StorageBuffer(heightmap_buffer.into()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "meshlets",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::StorageBuffer(meshlet_buffer.into()),
                    slot: 0,
                }],
            );

        pso_builder = pso_builder
            .add_reserved_table_variables(state)
            .unwrap();

        if info.use_depth {
            pso_builder = pso_builder.add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: info.sample_count,
            });
        }

        let depth_test = if info.use_depth {
            Some(dashi::DepthInfo {
                should_test: true,
                should_write: false,
                ..Default::default()
            })
        } else {
            None
        };

        let pipeline = pso_builder
            .set_details(dashi::GraphicsPipelineDetails {
                color_blend_states: vec![Default::default(); 1],
                sample_count: info.sample_count,
                depth_test,
                ..Default::default()
            })
            .build(ctx)
            .expect("Failed to build terrain PSO");

        state.register_pso_tables(&pipeline);

        Self {
            pipeline,
            compute_pipeline,
            clipmap_buffer,
            draw_args_buffer,
            instance_buffer,
            heightmap_buffer,
            meshlet_buffer,
            patch_size: terrain_info.patch_size,
            lod_levels: terrain_info.lod_levels,
            clipmap_resolution: terrain_info.clipmap_resolution,
            max_tiles: terrain_info.max_tiles,
            camera_position: Vec3::ZERO,
            camera_far: 0.0,
            frustum_planes: None,
            view_projection: None,
            use_depth: info.use_depth,
            deferred: None,
            static_geometry: None,
            lod_sources: HashMap::new(),
            active_chunk_lods: HashMap::new(),
            active_chunk_lod_levels: HashMap::new(),
            context: NonNull::new(ctx),
            terrain_rdb: None,
            terrain_project_key: None,
            terrain_settings: None,
            terrain_settings_dirty: true,
            terrain_render_objects: HashMap::new(),
            terrain_dirty: true,
            refresh_frame_index: 0,
            last_refresh_frame: 0,
            last_settings_poll_frame: 0,
            last_frame_camera_position: None,
            last_refresh_camera_position: None,
            last_refresh_view_projection: None,
            last_refresh_chunk_coords: None,
            last_base_chunk_hashes: HashMap::new(),
            missing_lod_artifacts: HashSet::new(),
            visibility_cache_selection_hash: 0,
            visibility_cache_view_projection: None,
            visibility_cache_camera_position: None,
            visibility_cache_camera_far: 0.0,
            visibility_cache_keys: HashSet::new(),
            camera_velocity: 0.0,
            pending_selection: None,
            pending_selection_keys: HashMap::new(),
            pending_selection_lod_levels: HashMap::new(),
            pending_refresh: None,
            texture_data_cache: HashMap::new(),
            texture_work_requests,
            texture_work_results,
            texture_work_pending: HashSet::new(),
            texture_worker_running,
            texture_worker_handle,
            clipmap_buffers,
            clipmap_slots,
            clipmap_buffers_dirty,
            clipmap_cache_dirty: clipmap_buffers_dirty,
            pending_selection_tiles: HashMap::new(),
            active_selection_tiles: HashMap::new(),
        }
    }

    fn spawn_texture_worker(
        requests: Arc<SegQueue<TerrainTextureWorkItem>>,
        results: Arc<SegQueue<TerrainTextureBuildResult>>,
        running: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            while running.load(Ordering::Acquire) {
                if let Some(item) = requests.pop() {
                    let grid_x = item.work_key.grid[0];
                    let grid_y = item.work_key.grid[1];
                    let artifact = &item.artifact;
                    let textures = vec![
                        (
                            TerrainTextureCacheKey {
                                kind: "height",
                                hash: item.work_key.hash,
                                grid: item.work_key.grid,
                            },
                            Arc::new(Self::build_heightmap_data(artifact, grid_x, grid_y)),
                        ),
                        (
                            TerrainTextureCacheKey {
                                kind: "normal",
                                hash: item.work_key.hash,
                                grid: item.work_key.grid,
                            },
                            Arc::new(Self::build_normalmap_data(artifact, grid_x, grid_y)),
                        ),
                        (
                            TerrainTextureCacheKey {
                                kind: "blend",
                                hash: item.work_key.hash,
                                grid: item.work_key.grid,
                            },
                            Arc::new(Self::build_blendmap_data(artifact, grid_x, grid_y)),
                        ),
                        (
                            TerrainTextureCacheKey {
                                kind: "blend_ids",
                                hash: item.work_key.hash,
                                grid: item.work_key.grid,
                            },
                            Arc::new(Self::build_blend_ids_data(artifact, grid_x, grid_y)),
                        ),
                        (
                            TerrainTextureCacheKey {
                                kind: "hole_mask",
                                hash: item.work_key.hash,
                                grid: item.work_key.grid,
                            },
                            Arc::new(Self::build_hole_mask_data(artifact, grid_x, grid_y)),
                        ),
                    ];
                    results.push(TerrainTextureBuildResult {
                        work_key: item.work_key,
                        textures,
                    });
                } else {
                    thread::sleep(Duration::from_millis(1));
                }
            }
        })
    }

    pub fn pre_compute(&mut self) -> CommandStream<Executable> {
        let mut stream = CommandStream::new().begin();
        if let Some(deferred) = &mut self.deferred {
            stream = stream.combine(deferred.draw_builder.pre_compute());
        }
        stream.end()
    }

    pub fn post_compute(&mut self) -> CommandStream<Executable> {
        let mut stream = CommandStream::new().begin();
        if let Some(deferred) = &mut self.deferred {
            stream = stream.combine(deferred.draw_builder.post_compute());
        }
        stream.end()
    }

    pub fn initialize_deferred(
        &mut self,
        ctx: &mut Context,
        state: &mut BindlessState,
        sample_count: SampleCount,
        cull_results: Handle<Buffer>,
        bin_counts: Handle<Buffer>,
        _num_bins: u32,
        dynamic: &DynamicAllocator,
    ) {
        let draw_builder = GPUDrawBuilder::new(
            &GPUDrawBuilderInfo {
                name: "[MESHI] Deferred Terrain Draw Builder",
                ctx,
                cull_results,
                bin_counts,
                num_bins: 0,
                ..Default::default()
            },
            state,
        );

        let Some(clipmap_buffers) = self.clipmap_buffers.as_ref() else {
            warn!("Terrain clipmap buffers missing; deferred terrain will be disabled.");
            return;
        };

        let pipeline = Self::build_deferred_pipeline(
            ctx,
            state,
            sample_count,
            &draw_builder,
            dynamic,
            clipmap_buffers,
        );

        self.deferred = Some(TerrainDeferredResources {
            draw_builder,
            pipeline,
            objects: Default::default(),
            db: None,
        });
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        if let Some(deferred) = &mut self.deferred {
            deferred.db = Some(NonNull::new(db).expect("terrain db"));
        }
    }

    pub fn set_rdb(&mut self, rdb: &mut RDBFile, project_key: &str) {
        self.terrain_rdb = Some(NonNull::new(rdb).expect("terrain rdb"));
        self.terrain_project_key = Some(project_key.to_string());
        self.terrain_settings = None;
        self.terrain_settings_dirty = true;
        self.terrain_render_objects.clear();
        self.lod_sources.clear();
        self.active_chunk_lods.clear();
        self.active_chunk_lod_levels.clear();
        self.last_refresh_chunk_coords = None;
        self.last_base_chunk_hashes.clear();
        self.missing_lod_artifacts.clear();
        self.visibility_cache_keys.clear();
        self.visibility_cache_selection_hash = 0;
        self.visibility_cache_view_projection = None;
        self.visibility_cache_camera_position = None;
        self.visibility_cache_camera_far = 0.0;
        self.last_frame_camera_position = None;
        self.camera_velocity = 0.0;
        self.terrain_dirty = true;
        self.pending_selection = None;
        self.pending_selection_keys.clear();
        self.pending_selection_lod_levels.clear();
        self.pending_refresh = None;
        self.texture_data_cache.clear();
        self.texture_work_pending.clear();
        self.clipmap_slots.iter_mut().for_each(|slot| *slot = None);
        self.clipmap_buffers_dirty = true;
        self.clipmap_cache_dirty = true;
        self.pending_selection_tiles.clear();
        self.active_selection_tiles.clear();
    }

    pub fn set_project_key(&mut self, project_key: &str) {
        self.terrain_project_key = Some(project_key.to_string());
        self.terrain_settings = None;
        self.terrain_settings_dirty = true;
        self.terrain_render_objects.clear();
        self.lod_sources.clear();
        self.active_chunk_lods.clear();
        self.active_chunk_lod_levels.clear();
        self.last_refresh_chunk_coords = None;
        self.last_base_chunk_hashes.clear();
        self.missing_lod_artifacts.clear();
        self.visibility_cache_keys.clear();
        self.visibility_cache_selection_hash = 0;
        self.visibility_cache_view_projection = None;
        self.visibility_cache_camera_position = None;
        self.visibility_cache_camera_far = 0.0;
        self.last_frame_camera_position = None;
        self.camera_velocity = 0.0;
        self.terrain_dirty = true;
        self.pending_selection = None;
        self.pending_selection_keys.clear();
        self.pending_selection_lod_levels.clear();
        self.pending_refresh = None;
        self.texture_data_cache.clear();
        self.texture_work_pending.clear();
        self.clipmap_slots.iter_mut().for_each(|slot| *slot = None);
        self.clipmap_buffers_dirty = true;
        self.clipmap_cache_dirty = true;
        self.pending_selection_tiles.clear();
        self.active_selection_tiles.clear();
    }

    pub fn update(&mut self, camera: Handle<Camera>, state: &mut BindlessState) {
        let bump = crate::render::global_bump().get();
        let _frame_marker = bump.alloc(0u8);
        self.refresh_frame_index = self.refresh_frame_index.saturating_add(1);
        let previous_frame_position = self
            .last_frame_camera_position
            .unwrap_or(self.camera_position);
        self.update_camera_state(camera, state);
        self.camera_velocity = self.camera_position.distance(previous_frame_position);
        self.last_frame_camera_position = Some(self.camera_position);
        if self.drain_texture_work() {
            self.clipmap_cache_dirty = true;
        }
        if self.terrain_project_key.is_some() {
            let settings_poll_due = self
                .refresh_frame_index
                .saturating_sub(self.last_settings_poll_frame)
                >= TERRAIN_SETTINGS_POLL_INTERVAL;
            if settings_poll_due {
                self.poll_terrain_settings();
                self.last_settings_poll_frame = self.refresh_frame_index;
            }

            if self.should_refresh_terrain() {
                self.refresh_rdb_objects(camera);
                self.last_refresh_frame = self.refresh_frame_index;
                self.last_settings_poll_frame = self.refresh_frame_index;
                self.last_refresh_camera_position = Some(self.camera_position);
                self.last_refresh_view_projection = self.view_projection;
                self.last_refresh_chunk_coords = self
                    .terrain_settings
                    .as_ref()
                    .and_then(|settings| self.camera_chunk_coords(settings));
                self.terrain_dirty = false;
            }
            self.poll_missing_lod_artifacts();
        }
        if self.terrain_settings_dirty {
            if let Some(settings) = self.terrain_settings.clone() {
                self.ensure_static_geometry(&settings, state);
                self.ensure_clipmap_buffers(&settings);
            }
            self.terrain_settings_dirty = false;
        }
        if self.lod_sources.is_empty() {
            return;
        }

        let (selection, selection_tiles) = self.select_lod_objects();
        let selection_keys = Self::selection_key_map(&selection);

        let Some(mut deferred) = self.deferred.take() else {
            return;
        };

        let mut selection_changed = selection_keys != self.active_chunk_lods;
        if !selection_changed {
            selection_changed = selection.iter().any(|object| {
                deferred
                    .objects
                    .get(&object.key)
                    .map(|entry| entry.content_hash != object.artifact.content_hash)
                    .unwrap_or(true)
            });
        }

        if selection_changed {
            self.pending_selection = Some(selection.clone());
            self.pending_selection_keys = selection_keys;
            self.pending_selection_tiles = selection_tiles.clone();
            self.pending_selection_lod_levels = selection
                .iter()
                .map(|object| {
                    (
                        TerrainChunkKey {
                            project_key: object.artifact.project_key.clone(),
                            coords: object.artifact.chunk_coords,
                        },
                        object.artifact.lod,
                    )
                })
                .collect();
        }

        if let Some(pending) = self.pending_selection.take() {
            let tile_map = std::mem::take(&mut self.pending_selection_tiles);
            let completed = self.apply_render_objects(
                &pending,
                &tile_map,
                &mut deferred,
                state,
                TERRAIN_UPDATE_BUDGET_PER_FRAME,
            );
            if completed {
                self.active_chunk_lods = std::mem::take(&mut self.pending_selection_keys);
                self.active_selection_tiles = tile_map;
                self.active_chunk_lod_levels =
                    std::mem::take(&mut self.pending_selection_lod_levels);
            } else {
                self.pending_selection = Some(pending);
                self.pending_selection_tiles = tile_map;
            }
        }

        if selection_changed || self.clipmap_cache_dirty {
            self.refresh_clipmap_tiles(&selection, &selection_tiles);
            self.clipmap_cache_dirty = false;
        }

        self.update_visibility(&selection, &mut deferred);

        self.deferred = Some(deferred);
    }

    pub fn build_deferred_draws(&mut self, bin: u32, view: u32) -> CommandStream<Executable> {
        let Some(deferred) = &mut self.deferred else {
            return CommandStream::new().begin().end();
        };

        deferred.draw_builder.build_draws(bin, view)
    }

    pub fn draw_builder(&self) -> Option<&GPUDrawBuilder> {
        self.deferred
            .as_ref()
            .map(|deferred| &deferred.draw_builder)
    }

    pub fn draw_info(&self) -> Option<TerrainDrawInfo> {
        self.deferred.as_ref().map(|deferred| TerrainDrawInfo {
            per_draw_data: deferred.draw_builder.per_draw_data(),
            draw_list: deferred.draw_builder.draw_list(),
            draw_count: deferred.draw_builder.draw_count(),
        })
    }

    fn group_lod_sources(
        objects: &[TerrainRenderObject],
    ) -> HashMap<TerrainChunkKey, Vec<TerrainRenderObject>> {
        let mut sources: HashMap<TerrainChunkKey, Vec<TerrainRenderObject>> = HashMap::new();
        for object in objects {
            let key = TerrainChunkKey {
                project_key: object.artifact.project_key.clone(),
                coords: object.artifact.chunk_coords,
            };
            sources.entry(key).or_default().push(object.clone());
        }
        for values in sources.values_mut() {
            values.sort_by_key(|source| source.artifact.lod);
        }
        sources
    }

    fn process_pending_refresh(&mut self, pending: &mut PendingTerrainRefresh) -> bool {
        let mut remaining = TERRAIN_REFRESH_CHUNK_BUDGET_PER_FRAME;
        match &mut pending.kind {
            PendingRefreshKind::Full(state) => {
                while state.index < state.base_artifacts.len() && remaining > 0 {
                    let base_artifact = state.base_artifacts[state.index].clone();
                    state.index += 1;
                    remaining = remaining.saturating_sub(1);
                    self.process_full_refresh_chunk(state, &base_artifact);
                }

                if state.index >= state.base_artifacts.len() {
                    self.terrain_render_objects = std::mem::take(&mut state.next_objects);
                    self.lod_sources = Self::group_lod_sources(&state.ordered_objects);
                    self.last_base_chunk_hashes = std::mem::take(&mut state.next_base_hashes);
                    info!(
                        "Terrain refresh: loaded_artifacts={}, total_objects={}",
                        state.loaded_artifacts,
                        self.terrain_render_objects.len()
                    );
                    return true;
                }
            }
            PendingRefreshKind::Delta(state) => {
                while state.update_index < state.updates.len() && remaining > 0 {
                    let (key, base_artifact) = state.updates[state.update_index].clone();
                    state.update_index += 1;
                    remaining = remaining.saturating_sub(1);
                    self.process_delta_refresh_chunk(state, &key, &base_artifact);
                }

                while state.removal_index < state.removals.len() && remaining > 0 {
                    let key = &state.removals[state.removal_index];
                    state.removal_index += 1;
                    remaining = remaining.saturating_sub(1);
                    if let Some(objects) = self.lod_sources.remove(key) {
                        for object in objects {
                            self.terrain_render_objects.remove(&object.key);
                        }
                    }
                }

                if state.update_index >= state.updates.len()
                    && state.removal_index >= state.removals.len()
                {
                    self.last_base_chunk_hashes = std::mem::take(&mut state.next_base_hashes);
                    if state.updated_chunks > 0 || !state.removals.is_empty() {
                        info!(
                            "Terrain refresh: updated_chunks={}, loaded_artifacts={}",
                            state.updated_chunks, state.loaded_artifacts
                        );
                    }
                    return true;
                }
            }
        }

        false
    }

    fn process_full_refresh_chunk(
        &mut self,
        state: &mut PendingFullRefresh,
        base_artifact: &TerrainChunkArtifact,
    ) {
        let coord_key = chunk_coord_key(
            base_artifact.chunk_coords[0],
            base_artifact.chunk_coords[1],
        );
        for lod in 0..state.lod_levels {
            let entry = chunk_artifact_entry(&state.project_key, &coord_key, &lod_key(lod));
            let mut artifact = if lod == base_artifact.lod {
                base_artifact.clone()
            } else if self.missing_lod_artifacts.contains(&entry) {
                let mut fallback = base_artifact.clone();
                fallback.lod = lod;
                fallback
            } else {
                match self.fetch_lod_artifact(&entry) {
                    Some(found) => {
                        self.missing_lod_artifacts.remove(&entry);
                        found
                    }
                    None => {
                        self.missing_lod_artifacts.insert(entry.clone());
                        let mut fallback = base_artifact.clone();
                        fallback.lod = lod;
                        fallback
                    }
                }
            };
            artifact.lod = lod;
            self.queue_texture_build(&artifact);

            let object = if let Some(existing) = self.terrain_render_objects.get(&entry).cloned() {
                if existing.artifact.content_hash == artifact.content_hash {
                    let mut updated = existing.clone();
                    updated.transform = terrain_loader::terrain_chunk_transform(
                        &state.settings,
                        updated.artifact.chunk_coords,
                        updated.artifact.bounds_min,
                    );
                    updated
                } else {
                    terrain_loader::terrain_render_object_from_artifact(
                        &state.settings,
                        entry.clone(),
                        artifact,
                    )
                }
            } else {
                terrain_loader::terrain_render_object_from_artifact(
                    &state.settings,
                    entry.clone(),
                    artifact,
                )
            };

            if !state.next_objects.contains_key(&entry) {
                state.loaded_artifacts += 1;
            }
            state.next_objects.insert(entry.clone(), object.clone());
            state.ordered_objects.push(object);
        }
    }

    fn process_delta_refresh_chunk(
        &mut self,
        state: &mut PendingDeltaRefresh,
        key: &TerrainChunkKey,
        base_artifact: &TerrainChunkArtifact,
    ) {
        let coord_key = chunk_coord_key(
            base_artifact.chunk_coords[0],
            base_artifact.chunk_coords[1],
        );
        let mut sources = Vec::with_capacity(state.lod_levels as usize);

        for lod in 0..state.lod_levels {
            let entry = chunk_artifact_entry(&state.project_key, &coord_key, &lod_key(lod));
            let mut artifact = if lod == base_artifact.lod {
                base_artifact.clone()
            } else if self.missing_lod_artifacts.contains(&entry) {
                let mut fallback = base_artifact.clone();
                fallback.lod = lod;
                fallback
            } else {
                match self.fetch_lod_artifact(&entry) {
                    Some(found) => {
                        self.missing_lod_artifacts.remove(&entry);
                        found
                    }
                    None => {
                        self.missing_lod_artifacts.insert(entry.clone());
                        let mut fallback = base_artifact.clone();
                        fallback.lod = lod;
                        fallback
                    }
                }
            };
            artifact.lod = lod;
            self.queue_texture_build(&artifact);

            let object = if let Some(existing) = self.terrain_render_objects.get(&entry).cloned() {
                if existing.artifact.content_hash == artifact.content_hash {
                    let mut updated = existing.clone();
                    updated.transform = terrain_loader::terrain_chunk_transform(
                        &state.settings,
                        updated.artifact.chunk_coords,
                        updated.artifact.bounds_min,
                    );
                    updated
                } else {
                    terrain_loader::terrain_render_object_from_artifact(
                        &state.settings,
                        entry.clone(),
                        artifact,
                    )
                }
            } else {
                terrain_loader::terrain_render_object_from_artifact(
                    &state.settings,
                    entry.clone(),
                    artifact,
                )
            };

            if !self.terrain_render_objects.contains_key(&entry) {
                state.loaded_artifacts += 1;
            }
            self.terrain_render_objects.insert(entry.clone(), object.clone());
            sources.push(object);
        }

        sources.sort_by_key(|source| source.artifact.lod);
        self.lod_sources.insert(key.clone(), sources);
        state.updated_chunks += 1;
    }

    fn refresh_rdb_objects(&mut self, camera: Handle<Camera>) {
        let Some(project_key) = self.terrain_project_key.clone() else {
            return;
        };
        let Some(mut db_ptr) = self.deferred.as_ref().and_then(|deferred| deferred.db) else {
            warn!("Terrain refresh skipped: no terrain DB available.");
            return;
        };
        if let Some(mut pending) = self.pending_refresh.take() {
            let completed = self.process_pending_refresh(&mut pending);
            if !completed {
                self.pending_refresh = Some(pending);
            }
            return;
        }

        let db = unsafe { db_ptr.as_mut() };
        let Some(settings) = self.load_terrain_settings(&project_key, db) else {
            return;
        };

        let settings_changed = self
            .terrain_settings
            .as_ref()
            .map(|cached| cached != &settings)
            .unwrap_or(true);
        if settings_changed {
            info!(
                "Loaded terrain settings for project '{}' ({:?})",
                project_key, settings
            );
            self.terrain_settings = Some(settings.clone());
            self.terrain_settings_dirty = true;
            self.missing_lod_artifacts.clear();
        }

        let chunks = match db.fetch_terrain_chunks_from_view(&settings, &project_key, camera) {
            Ok(chunks) => chunks,
            Err(err) => {
                warn!("Terrain refresh: failed to fetch chunks: {err:?}");
                return;
            }
        };

        let mut next_base_hashes = HashMap::with_capacity(chunks.len());
        let mut base_artifacts = HashMap::with_capacity(chunks.len());
        for base_artifact in chunks {
            let key = TerrainChunkKey {
                project_key: project_key.clone(),
                coords: base_artifact.chunk_coords,
            };
            next_base_hashes.insert(key.clone(), base_artifact.content_hash);
            base_artifacts.insert(key, base_artifact);
        }

        let base_hashes_changed = next_base_hashes != self.last_base_chunk_hashes;
        let needs_full_rebuild =
            settings_changed || self.terrain_render_objects.is_empty() || self.terrain_dirty;
        if !needs_full_rebuild {
            if !base_hashes_changed {
                self.last_base_chunk_hashes = next_base_hashes;
                return;
            }

            let lod_levels = self.lod_levels.min(u32::from(u8::MAX)) as u8;
            let mut removed_keys = Vec::new();
            for key in self.last_base_chunk_hashes.keys() {
                if !next_base_hashes.contains_key(key) {
                    removed_keys.push(key.clone());
                }
            }

            let mut updates = Vec::new();
            for (key, base_artifact) in base_artifacts {
                let needs_update = self
                    .last_base_chunk_hashes
                    .get(&key)
                    .map(|hash| *hash != base_artifact.content_hash)
                    .unwrap_or(true);
                if needs_update {
                    updates.push((key, base_artifact));
                }
            }

            if updates.is_empty() && removed_keys.is_empty() {
                self.last_base_chunk_hashes = next_base_hashes;
                return;
            }

            let mut pending = PendingTerrainRefresh {
                kind: PendingRefreshKind::Delta(PendingDeltaRefresh {
                    project_key,
                    settings,
                    updates,
                    removals: removed_keys,
                    next_base_hashes,
                    lod_levels,
                    updated_chunks: 0,
                    loaded_artifacts: 0,
                    update_index: 0,
                    removal_index: 0,
                }),
            };
            let completed = self.process_pending_refresh(&mut pending);
            if !completed {
                self.pending_refresh = Some(pending);
            }
            return;
        }

        let lod_levels = self.lod_levels.min(u32::from(u8::MAX)) as u8;
        if !settings_changed && !base_hashes_changed && !self.terrain_dirty {
            let expected_objects = base_artifacts.len() * lod_levels as usize;
            if expected_objects == self.terrain_render_objects.len() {
                return;
            }
        }

        let mut pending = PendingTerrainRefresh {
            kind: PendingRefreshKind::Full(PendingFullRefresh {
                project_key,
                settings,
                base_artifacts: base_artifacts.into_values().collect(),
                next_base_hashes,
                lod_levels,
                next_objects: HashMap::new(),
                ordered_objects: Vec::new(),
                loaded_artifacts: 0,
                index: 0,
            }),
        };
        let completed = self.process_pending_refresh(&mut pending);
        if !completed {
            self.pending_refresh = Some(pending);
        }
    }

    fn poll_missing_lod_artifacts(&mut self) {
        if self.missing_lod_artifacts.is_empty() {
            return;
        }
        let Some(settings) = self.terrain_settings.clone() else {
            return;
        };
        let entries: Vec<String> = self
            .missing_lod_artifacts
            .iter()
            .take(TERRAIN_MISSING_LOD_POLL_LIMIT)
            .cloned()
            .collect();
        if entries.is_empty() {
            return;
        }

        let mut resolved = Vec::new();
        for entry in entries {
            if let Some(artifact) = self.fetch_lod_artifact(&entry) {
                resolved.push((entry, artifact));
            }
        }

        if resolved.is_empty() {
            return;
        }

        for (entry, artifact) in resolved {
            self.missing_lod_artifacts.remove(&entry);
            let object = terrain_loader::terrain_render_object_from_artifact(
                &settings,
                entry.clone(),
                artifact.clone(),
            );
            self.queue_texture_build(&artifact);
            self.terrain_render_objects.insert(entry.clone(), object.clone());

            let key = TerrainChunkKey {
                project_key: artifact.project_key.clone(),
                coords: artifact.chunk_coords,
            };
            let sources = self.lod_sources.entry(key).or_default();
            if let Some(existing) = sources
                .iter_mut()
                .find(|source| source.artifact.lod == artifact.lod)
            {
                *existing = object.clone();
            } else {
                sources.push(object.clone());
                sources.sort_by_key(|source| source.artifact.lod);
            }
        }

        self.terrain_dirty = true;
    }

    fn poll_terrain_settings(&mut self) {
        let Some(project_key) = self.terrain_project_key.clone() else {
            return;
        };
        let Some(mut db_ptr) = self.deferred.as_ref().and_then(|deferred| deferred.db) else {
            return;
        };
        let db = unsafe { db_ptr.as_mut() };
        let Some(settings) = self.load_terrain_settings(&project_key, db) else {
            return;
        };

        let settings_changed = self
            .terrain_settings
            .as_ref()
            .map(|cached| cached != &settings)
            .unwrap_or(true);
        if settings_changed {
            info!(
                "Loaded terrain settings for project '{}' ({:?})",
                project_key, settings
            );
            self.terrain_settings = Some(settings.clone());
            self.terrain_settings_dirty = true;
            self.terrain_dirty = true;
        }
    }

    fn terrain_frustum(&self, plane_z: f32) -> Option<TerrainFrustum> {
        let view_projection = self.view_projection?;
        let inverse = view_projection.inverse();
        let ndc_corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        let mut frustum = [[0.0; 2]; 4];

        for (idx, (x, y)) in ndc_corners.iter().enumerate() {
            let clip = Vec4::new(*x, *y, 1.0, 1.0);
            let mut world = inverse * clip;
            if world.w.abs() > f32::EPSILON {
                world /= world.w;
            }
            let point = Vec3::new(world.x, world.y, world.z);
            let dir = point - self.camera_position;
            let projected = if dir.z.abs() > 1e-4 {
                let t = (plane_z - self.camera_position.z) / dir.z;
                if t.is_finite() && t > 0.0 {
                    self.camera_position + dir * t
                } else {
                    point
                }
            } else {
                point
            };
            frustum[idx] = [projected.x, projected.y];
        }

        Some(frustum)
    }

    fn fallback_frustum(camera_position: Vec3, extent: f32) -> TerrainFrustum {
        [
            [camera_position.x - extent, camera_position.y - extent],
            [camera_position.x + extent, camera_position.y - extent],
            [camera_position.x + extent, camera_position.y + extent],
            [camera_position.x - extent, camera_position.y + extent],
        ]
    }

    fn clamp_frustum_to_bounds(frustum: &mut TerrainFrustum, settings: &TerrainProjectSettings) {
        let min_x = settings.world_bounds_min[0];
        let min_y = settings.world_bounds_min[1];
        let max_x = settings.world_bounds_max[0];
        let max_y = settings.world_bounds_max[1];
        for corner in frustum.iter_mut() {
            corner[0] = corner[0].clamp(min_x, max_x);
            corner[1] = corner[1].clamp(min_y, max_y);
        }
    }


    fn chunk_center_world(settings: &TerrainProjectSettings, coords: [i32; 2]) -> Vec3 {
        let chunk_size_x = settings.tiles_per_chunk[0] as f32 * settings.tile_size;
        let chunk_size_y = settings.tiles_per_chunk[1] as f32 * settings.tile_size;
        let origin_x = settings.world_bounds_min[0] + coords[0] as f32 * chunk_size_x;
        let origin_y = settings.world_bounds_min[1] + coords[1] as f32 * chunk_size_y;
        let center_z = (settings.world_bounds_min[2] + settings.world_bounds_max[2]) * 0.5;
        Vec3::new(
            origin_x + chunk_size_x * 0.5,
            origin_y + chunk_size_y * 0.5,
            center_z,
        )
    }

    fn camera_chunk_coords(&self, settings: &TerrainProjectSettings) -> Option<[i32; 2]> {
        if settings.tile_size <= 0.0 {
            return None;
        }
        let tiles_x = settings.tiles_per_chunk[0].max(1) as f32;
        let tiles_y = settings.tiles_per_chunk[1].max(1) as f32;
        let chunk_size_x = tiles_x * settings.tile_size;
        let chunk_size_y = tiles_y * settings.tile_size;
        if chunk_size_x <= 0.0 || chunk_size_y <= 0.0 {
            return None;
        }
        let local_x = (self.camera_position.x - settings.world_bounds_min[0]) / chunk_size_x;
        let local_y = (self.camera_position.y - settings.world_bounds_min[1]) / chunk_size_y;
        Some([local_x.floor() as i32, local_y.floor() as i32])
    }

    fn select_lod_objects(&mut self) -> (Vec<TerrainRenderObject>, HashMap<TerrainChunkKey, u32>) {
        let Some(settings) = self.terrain_settings.as_ref() else {
            return (Vec::new(), HashMap::new());
        };
        let Some(center_coords) = self.camera_chunk_coords(settings) else {
            return (Vec::new(), HashMap::new());
        };
        let Some(project_key) = self.terrain_project_key.clone() else {
            return (Vec::new(), HashMap::new());
        };

        let resolution = self.clipmap_resolution.max(1) as i32;
        let half = resolution / 2;
        let mut selected = Vec::new();

        for grid_y in 0..resolution {
            for grid_x in 0..resolution {
                let offset_x = grid_x - half;
                let offset_y = grid_y - half;
                let coords = [center_coords[0] + offset_x, center_coords[1] + offset_y];
                let key = TerrainChunkKey {
                    project_key: project_key.clone(),
                    coords,
                };
                let Some(sources) = self.lod_sources.get(&key) else {
                    continue;
                };
                let ring = offset_x.abs().max(offset_y.abs()) as u32;
                let target_lod = if ring <= 1 {
                    0
                } else {
                    ring.ilog2().min(self.lod_levels.saturating_sub(1))
                } as u8;
                let object = Self::select_clipmap_source(sources, target_lod);
                selected.push(object);
            }
        }

        let tile_indices = self.assign_clipmap_tiles(&selected);
        (selected, tile_indices)
    }

    fn assign_clipmap_tiles(
        &self,
        selection: &[TerrainRenderObject],
    ) -> HashMap<TerrainChunkKey, u32> {
        let tile_count = (self.clipmap_resolution.max(1) * self.clipmap_resolution.max(1)) as usize;
        let mut used = vec![false; tile_count];
        let mut tile_indices = HashMap::with_capacity(selection.len());

        for object in selection {
            let key = TerrainChunkKey {
                project_key: object.artifact.project_key.clone(),
                coords: object.artifact.chunk_coords,
            };
            if let Some(&slot) = self.active_selection_tiles.get(&key) {
                let slot_index = slot as usize;
                if slot_index < tile_count && !used[slot_index] {
                    used[slot_index] = true;
                    tile_indices.insert(key, slot);
                }
            }
        }

        let mut next_slot = 0usize;
        for object in selection {
            let key = TerrainChunkKey {
                project_key: object.artifact.project_key.clone(),
                coords: object.artifact.chunk_coords,
            };
            if tile_indices.contains_key(&key) {
                continue;
            }
            while next_slot < tile_count && used[next_slot] {
                next_slot += 1;
            }
            let slot = if next_slot < tile_count {
                used[next_slot] = true;
                let slot = next_slot as u32;
                next_slot += 1;
                slot
            } else {
                0
            };
            tile_indices.insert(key, slot);
        }

        tile_indices
    }

    fn select_clipmap_source(
        sources: &[TerrainRenderObject],
        target_lod: u8,
    ) -> TerrainRenderObject {
        let mut best = sources[0].clone();
        let mut best_delta = best.artifact.lod.abs_diff(target_lod);
        for source in sources.iter().skip(1) {
            let delta = source.artifact.lod.abs_diff(target_lod);
            if delta < best_delta
                || (delta == best_delta && source.artifact.lod < best.artifact.lod)
            {
                best = source.clone();
                best_delta = delta;
            }
        }
        best
    }

    fn update_camera_state(&mut self, camera: Handle<Camera>, state: &mut BindlessState) {
        if !camera.valid() {
            return;
        }

        let mut camera_position = self.camera_position;
        let mut camera_far = self.camera_far;
        let mut view_projection = self.view_projection;

        if let Ok(cameras) = state.reserved::<ReservedBindlessCamera>("meshi_bindless_cameras") {
            let camera_data = cameras.camera(camera);
            camera_position = camera_data.position();
            camera_far = camera_data.far;
            let view = camera_data.world_from_camera.inverse();
            view_projection = Some(camera_data.projection * view);
        } else {
            warn!("TerrainRenderer failed to access bindless cameras for refresh state.");
        }

        self.camera_position = camera_position;
        self.camera_far = camera_far;
        self.view_projection = view_projection;
        self.frustum_planes = view_projection.map(Self::extract_frustum_planes);
    }

    fn should_refresh_terrain(&self) -> bool {
        let refresh_interval = if self.camera_velocity <= TERRAIN_CAMERA_VELOCITY_EPSILON {
            1
        } else {
            TERRAIN_REFRESH_FRAME_INTERVAL
        };
        let refresh_due = self
            .refresh_frame_index
            .saturating_sub(self.last_refresh_frame)
            >= refresh_interval;

        if self.terrain_dirty {
            return true;
        }

        if !refresh_due {
            if let Some(settings) = self.terrain_settings.as_ref() {
                if let Some(current_chunk) = self.camera_chunk_coords(settings) {
                    if self
                        .last_refresh_chunk_coords
                        .map_or(true, |last| last != current_chunk)
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        let camera_moved = self.last_refresh_camera_position.map_or(true, |last| {
            self.camera_position.distance(last) > TERRAIN_CAMERA_POSITION_EPSILON
        });
        let view_changed = match (self.last_refresh_view_projection, self.view_projection) {
            (Some(last), Some(current)) => {
                Self::mat4_max_delta(last, current) > TERRAIN_VIEW_PROJECTION_EPSILON
            }
            (None, Some(_)) | (Some(_), None) => true,
            (None, None) => false,
        };

        if self.terrain_render_objects.is_empty() {
            if let Some(settings) = self.terrain_settings.as_ref() {
                if let Some(current_chunk) = self.camera_chunk_coords(settings) {
                    return self
                        .last_refresh_chunk_coords
                        .map_or(true, |last| last != current_chunk);
                }
            }
        }

        camera_moved || view_changed
    }

    fn mat4_max_delta(a: Mat4, b: Mat4) -> f32 {
        let a = a.to_cols_array();
        let b = b.to_cols_array();
        a.iter()
            .zip(b.iter())
            .map(|(left, right)| (left - right).abs())
            .fold(0.0, f32::max)
    }

    fn fetch_lod_artifact(&mut self, entry: &str) -> Option<TerrainChunkArtifact> {
        let Some(mut rdb_ptr) = self.terrain_rdb else {
            return None;
        };
        let rdb = unsafe { rdb_ptr.as_mut() };
        rdb.fetch::<TerrainChunkArtifact>(entry).ok()
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

    fn world_bounds(object: &TerrainRenderObject) -> (Vec3, Vec3) {
        let bounds_min = Vec3::from(object.artifact.bounds_min);
        let bounds_max = Vec3::from(object.artifact.bounds_max);
        let center = (bounds_min + bounds_max) * 0.5;
        let extent = (bounds_max - bounds_min) * 0.5;
        (center, extent)
    }

    fn chunk_visible(&self, object: &TerrainRenderObject) -> bool {
        let (center, extent) = Self::world_bounds(object);
        let radius = extent.length();
        if self.camera_far > 0.0
            && (center - self.camera_position).length() - radius > self.camera_far
        {
            return false;
        }

        let Some(planes) = &self.frustum_planes else {
            return true;
        };

        for plane in planes {
            let normal = Vec3::new(plane.x, plane.y, plane.z);
            let distance = normal.dot(center) + plane.w;
            let projection_radius = extent.dot(normal.abs());
            if distance + projection_radius < 0.0 {
                return false;
            }
        }

        true
    }

    fn extract_frustum_planes(view_projection: Mat4) -> [Vec4; 6] {
        let cols = view_projection.to_cols_array_2d();
        let row = |index: usize| {
            Vec4::new(
                cols[0][index],
                cols[1][index],
                cols[2][index],
                cols[3][index],
            )
        };
        let row0 = row(0);
        let row1 = row(1);
        let row2 = row(2);
        let row3 = row(3);

        let mut planes = [
            row3 + row0,
            row3 - row0,
            row3 + row1,
            row3 - row1,
            row3 + row2,
            row3 - row2,
        ];

        for plane in &mut planes {
            let normal = Vec3::new(plane.x, plane.y, plane.z);
            let length = normal.length();
            if length > 0.0 {
                *plane /= length;
            }
        }

        planes
    }

    fn selection_key_map(selection: &[TerrainRenderObject]) -> HashMap<TerrainChunkKey, String> {
        let mut map = HashMap::with_capacity(selection.len());
        for object in selection {
            let key = TerrainChunkKey {
                project_key: object.artifact.project_key.clone(),
                coords: object.artifact.chunk_coords,
            };
            map.insert(key, object.key.clone());
        }
        map
    }

    fn visibility_selection_hash(selection: &[TerrainRenderObject]) -> u64 {
        let mut hasher = DefaultHasher::new();
        for object in selection {
            object.key.hash(&mut hasher);
            object.artifact.content_hash.hash(&mut hasher);
            object.artifact.lod.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn apply_render_objects(
        &mut self,
        objects: &[TerrainRenderObject],
        tile_indices: &HashMap<TerrainChunkKey, u32>,
        deferred: &mut TerrainDeferredResources,
        state: &mut BindlessState,
        update_budget: usize,
    ) -> bool {
        let mut remaining_budget = update_budget;
        let mut next_keys = HashSet::with_capacity(objects.len());
        for object in objects {
            next_keys.insert(object.key.clone());
        }

        let mut pending_work = false;
        for object in objects {
            let key = TerrainChunkKey {
                project_key: object.artifact.project_key.clone(),
                coords: object.artifact.chunk_coords,
            };
            let tile_index = tile_indices.get(&key).copied().unwrap_or(0);
            if let Some(entry) = deferred.objects.get_mut(&object.key) {
                self.update_terrain_transform(entry.transform_handle, object.transform, state);
                if entry.content_hash == object.artifact.content_hash {
                    if entry.clipmap_tile_index != tile_index {
                        if remaining_budget == 0 {
                            pending_work = true;
                            continue;
                        }
                        remaining_budget = remaining_budget.saturating_sub(1);
                        entry.clipmap_tile_index = tile_index;
                        for instance in &mut entry.draw_instances {
                            instance.clipmap_tile_index = tile_index;
                        }
                        TerrainRenderer::release_terrain_draws(entry, &mut deferred.draw_builder);
                        entry.draws = self.register_draw_instances(
                            &entry.draw_instances,
                            entry.transform_handle,
                            &mut deferred.draw_builder,
                        );
                    }
                    continue;
                }
            }

            if remaining_budget == 0 {
                pending_work = true;
                continue;
            }
            remaining_budget = remaining_budget.saturating_sub(1);

            let Some(entry_build) =
                self.build_terrain_entry(&object.artifact, tile_index, state)
            else {
                warn!("Failed to build terrain render object '{}'.", object.key);
                continue;
            };

            let transform_handle = self.allocate_terrain_transform(object.transform, state);
            let draw_instances = entry_build.draw_instances.clone();
            let draws = self.register_draw_instances(
                &draw_instances,
                transform_handle,
                &mut deferred.draw_builder,
            );

            if let Some(entry) = deferred.objects.remove(&object.key) {
                self.release_terrain_entry(&entry, deferred, state);
            }

            deferred.objects.insert(
                object.key.clone(),
                TerrainObjectEntry {
                    transform_handle,
                    draws,
                    draw_instances,
                    content_hash: object.artifact.content_hash,
                    material_handle: entry_build.material_handle,
                    clipmap_tile_index: entry_build.clipmap_tile_index,
                },
            );
        }

        let mut removals = Vec::new();
        for (key, entry) in &deferred.objects {
            if !next_keys.contains(key) {
                removals.push((key.clone(), entry.clone()));
            }
        }

        for (key, entry) in removals {
            if remaining_budget == 0 {
                pending_work = true;
                break;
            }
            remaining_budget = remaining_budget.saturating_sub(1);
            self.release_terrain_entry(&entry, deferred, state);
            deferred.objects.remove(&key);
        }

        !pending_work
    }

    fn update_visibility(
        &mut self,
        objects: &[TerrainRenderObject],
        deferred: &mut TerrainDeferredResources,
    ) {
        let selection_hash = Self::visibility_selection_hash(objects);
        let view_projection_stable = match (self.visibility_cache_view_projection, self.view_projection) {
            (Some(last), Some(current)) => {
                Self::mat4_max_delta(last, current) <= TERRAIN_VIEW_PROJECTION_EPSILON
            }
            (None, None) => true,
            _ => false,
        };
        let camera_position_stable = self.visibility_cache_camera_position.map_or(false, |last| {
            self.camera_position.distance(last) <= TERRAIN_CAMERA_POSITION_EPSILON
        });
        let camera_far_stable =
            (self.visibility_cache_camera_far - self.camera_far).abs() <= f32::EPSILON;
        let use_cache = self.visibility_cache_selection_hash == selection_hash
            && view_projection_stable
            && camera_position_stable
            && camera_far_stable;

        let mut visible_keys = if use_cache {
            self.visibility_cache_keys.clone()
        } else {
            HashSet::with_capacity(objects.len())
        };
        let mut to_register = Vec::new();
        for object in objects {
            if !use_cache {
                if !self.chunk_visible(object) {
                    continue;
                }
                visible_keys.insert(object.key.clone());
            }
            if let Some(entry) = deferred.objects.get(&object.key) {
                if entry.draws.is_empty() {
                    to_register.push(object.key.clone());
                }
            }
        }

        if !use_cache {
            self.visibility_cache_selection_hash = selection_hash;
            self.visibility_cache_view_projection = self.view_projection;
            self.visibility_cache_camera_position = Some(self.camera_position);
            self.visibility_cache_camera_far = self.camera_far;
            self.visibility_cache_keys = visible_keys.clone();
        }

        for key in to_register {
            let Some(entry) = deferred.objects.get_mut(&key) else {
                continue;
            };
            let instances = entry.draw_instances.clone();
            let transform_handle = entry.transform_handle;
            entry.draws = self.register_draw_instances(
                &instances,
                transform_handle,
                &mut deferred.draw_builder,
            );
        }

        let mut to_release = Vec::new();
        for (key, entry) in deferred.objects.iter_mut() {
            if !visible_keys.contains(key) && !entry.draws.is_empty() {
                to_release.push(key.clone());
            }
        }

        for key in to_release {
            let Some(entry) = deferred.objects.get_mut(&key) else {
                continue;
            };
            TerrainRenderer::release_terrain_draws(entry, &mut deferred.draw_builder);
        }
    }

    pub fn record_compute(&mut self, dynamic: &mut DynamicAllocator) -> CommandStream<Executable> {
        let stream = CommandStream::new().begin();

        if let Some(pipeline) = self.compute_pipeline.as_ref() {
            let mut alloc = dynamic
                .bump()
                .expect("Failed to allocate terrain compute params");

            let params = &mut alloc.slice::<TerrainComputeParams>()[0];
            *params = TerrainComputeParams {
                camera_position: self.camera_position,
                lod_levels: self.lod_levels,
                patch_size: self.patch_size,
                max_tiles: self.max_tiles,
                clipmap_resolution: self.clipmap_resolution,
                height_scale: 5.0,
                _padding: [0.0; 2],
            };
            let group_count_x = (self.max_tiles + 63) / 64;

            return stream
                .prepare_buffer(self.clipmap_buffer, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(self.draw_args_buffer, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(self.instance_buffer, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(self.heightmap_buffer, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(self.meshlet_buffer, UsageBits::COMPUTE_SHADER)
                .dispatch(&Dispatch {
                    x: group_count_x.max(1),
                    y: 1,
                    z: 1,
                    pipeline: pipeline.handle,
                    bind_tables: pipeline.tables(),
                    dynamic_buffers: [None, Some(alloc), None, None],
                })
                .unbind_pipeline()
                .end();
        }
        stream.end()
    }

    pub fn record_deferred_draws(
        &mut self,
        viewport: &Viewport,
        dynamic: &mut DynamicAllocator,
        camera: Handle<Camera>,
        indices_handle: Handle<Buffer>,
    ) -> CommandStream<PendingGraphics> {
        let sync = self.sync_clipmap_buffers();
        let Some(deferred) = &mut self.deferred else {
            return CommandStream::<PendingGraphics>::subdraw();
        };

        #[repr(C)]
        struct PerSceneData {
            camera: Handle<Camera>,
            clipmap_grid_size: [u32; 2],
            clipmap_tile_texel_count: u32,
            _padding: u32,
        }

        let mut alloc = dynamic
            .bump()
            .expect("Failed to allocate terrain per-scene data");
        let clipmap_info = self.clipmap_buffers.as_ref().map(|buffers| {
            (
                buffers.grid_size,
                buffers.tile_texel_count,
            )
        });
        let (grid_size, tile_texel_count) = clipmap_info.unwrap_or(([0, 0], 0));
        alloc.slice::<PerSceneData>()[0] = PerSceneData {
            camera,
            clipmap_grid_size: grid_size,
            clipmap_tile_texel_count: tile_texel_count,
            _padding: 0,
        };

        let mut stream = CommandStream::<PendingGraphics>::subdraw();
        if let Some(sync) = sync {
            stream = stream.combine(sync);
        }
        stream
            .bind_graphics_pipeline(deferred.pipeline.handle)
            .update_viewport(viewport)
            .draw_indexed_indirect(&DrawIndexedIndirect {
                indices: indices_handle,
                indirect: deferred.draw_builder.draw_list(),
                bind_tables: deferred.pipeline.tables(),
                dynamic_buffers: [None, None, Some(alloc), None],
                draw_count: deferred.draw_builder.draw_count(),
                ..Default::default()
            })
            .unbind_graphics_pipeline()
    }

    pub fn record_draws(
        &mut self,
        viewport: &Viewport,
        dynamic: &mut DynamicAllocator,
    ) -> CommandStream<PendingGraphics> {
        let mut alloc = dynamic
            .bump()
            .expect("Failed to allocate terrain draw params");

        let params = &mut alloc.slice::<TerrainDrawParams>()[0];
        *params = TerrainDrawParams {
            camera_position: self.camera_position,
            lod_levels: self.lod_levels,
            patch_size: self.patch_size,
            max_tiles: self.max_tiles,
            clipmap_resolution: self.clipmap_resolution,
            height_scale: 5.0,
            _padding: [0.0; 2],
        };

        CommandStream::<PendingGraphics>::subdraw()
            .bind_graphics_pipeline(self.pipeline.handle)
            .update_viewport(viewport)
            .draw(&Draw {
                bind_tables: self.pipeline.tables(),
                dynamic_buffers: [None, Some(alloc), None, None],
                instance_count: self.max_tiles.max(1),
                count: 6,
                ..Default::default()
            })
            .unbind_graphics_pipeline()
    }

    fn build_deferred_pipeline(
        ctx: &mut Context,
        state: &mut BindlessState,
        sample_count: SampleCount,
        draw_builder: &GPUDrawBuilder,
        dynamic: &DynamicAllocator,
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
                    resource: ShaderResource::DynamicStorage(dynamic.state()),
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

    fn release_terrain_entry(
        &mut self,
        entry: &TerrainObjectEntry,
        deferred: &mut TerrainDeferredResources,
        state: &mut BindlessState,
    ) {
        self.release_terrain_transform(entry.transform_handle, state);
        for draw in &entry.draws {
            deferred.draw_builder.release_draw(*draw);
        }

        if entry.material_handle.valid() {
            state
                .reserved_mut::<ReservedBindlessMaterials, _>(
                    "meshi_bindless_materials",
                    |materials| materials.remove_material(entry.material_handle),
                )
                .expect("Failed to release terrain material");
        }
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

    fn build_terrain_entry(
        &mut self,
        artifact: &TerrainChunkArtifact,
        clipmap_tile_index: u32,
        state: &mut BindlessState,
    ) -> Option<TerrainEntryBuild> {
        let static_geometry = self.static_geometry.as_ref()?;
        let geometry = static_geometry
            .lods
            .get(artifact.lod as usize)
            .copied()
            .or_else(|| static_geometry.lods.last().copied())?;
        let (material_handle, _material) = self.allocate_terrain_material(state);
        let draw_instances = vec![TerrainDrawInstance {
            material: material_handle,
            vertex_id: geometry.vertex_id,
            vertex_count: geometry.vertex_count,
            index_id: geometry.index_id,
            index_count: geometry.index_count,
            clipmap_tile_index,
        }];

        Some(TerrainEntryBuild {
            draw_instances,
            material_handle,
            clipmap_tile_index,
        })
    }

    fn ensure_static_geometry(
        &mut self,
        settings: &TerrainProjectSettings,
        state: &mut BindlessState,
    ) {
        let settings_hash = Self::static_geometry_hash(settings, self.lod_levels);
        if let Some(existing) = &self.static_geometry {
            if existing.settings_hash == settings_hash {
                return;
            }
        }

        let lod_levels = self.lod_levels.max(1).min(u32::from(u8::MAX)) as u8;
        let mut lods = Vec::with_capacity(lod_levels as usize);
        for lod in 0..lod_levels {
            if let Some(geometry) = self.cached_plane_geometry(settings, state, lod, lod_levels) {
                lods.push(geometry);
            }
        }

        self.static_geometry = Some(TerrainStaticGeometry {
            lods,
            settings_hash,
        });
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

    fn cached_plane_geometry(
        &mut self,
        settings: &TerrainProjectSettings,
        state: &mut BindlessState,
        lod: u8,
        lod_levels: u8,
    ) -> Option<TerrainPlaneGeometry> {
        let cache_key = TerrainGeometryCacheKey {
            state_id: state as *const _ as usize,
            tiles_per_chunk: settings.tiles_per_chunk,
            tile_size_bits: settings.tile_size.to_bits(),
            lod,
            lod_levels,
        };
        if let Ok(cache) = Self::geometry_cache().lock() {
            if let Some(geometry) = cache.get(&cache_key).copied() {
                return Some(geometry);
            }
        }

        let geometry = self.build_plane_geometry(settings, state, lod, lod_levels)?;
        if let Ok(mut cache) = Self::geometry_cache().lock() {
            cache.insert(cache_key, geometry);
        }
        Some(geometry)
    }

    fn ensure_clipmap_buffers(&mut self, settings: &TerrainProjectSettings) {
        let grid_x = settings.tiles_per_chunk[0].saturating_add(1).max(1);
        let grid_y = settings.tiles_per_chunk[1].saturating_add(1).max(1);
        let grid_size = [grid_x, grid_y];
        if let Some(buffers) = self.clipmap_buffers.as_ref() {
            if buffers.grid_size != grid_size {
                warn!(
                    "Terrain clipmap grid size mismatch (settings {:?} vs startup {:?}).",
                    grid_size, buffers.grid_size
                );
            }
            return;
        }

        self.build_clipmap_buffers(grid_size);
    }

    fn build_clipmap_buffers(&mut self, grid_size: [u32; 2]) {
        let Some(mut ctx_ptr) = self.context else {
            return;
        };
        let ctx = unsafe { ctx_ptr.as_mut() };
        let buffers = Self::allocate_clipmap_buffers(ctx, self.clipmap_resolution, grid_size);
        let tile_count = buffers.tile_count;
        self.clipmap_buffers = Some(buffers);
        self.clipmap_slots = vec![None; tile_count as usize];
        self.clipmap_buffers_dirty = true;
        self.clipmap_cache_dirty = true;
    }

    fn allocate_clipmap_buffers(
        ctx: &mut Context,
        clipmap_resolution: u32,
        grid_size: [u32; 2],
    ) -> TerrainClipmapBuffers {
        let tile_count = clipmap_resolution.max(1) * clipmap_resolution.max(1);
        let tile_texel_count = grid_size[0].saturating_mul(grid_size[1]).max(1);
        let texel_total = tile_count.saturating_mul(tile_texel_count).max(1);
        let byte_size = texel_total
            .saturating_mul(4)
            .saturating_mul(std::mem::size_of::<f32>() as u32)
            .max(256);

        let mut height = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[MESHI GFX TERRAIN] Clipmap Heightmap",
                byte_size,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        );
        let mut normal = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[MESHI GFX TERRAIN] Clipmap Normals",
                byte_size,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        );
        let mut blend = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[MESHI GFX TERRAIN] Clipmap Blend Weights",
                byte_size,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        );
        let mut blend_ids = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[MESHI GFX TERRAIN] Clipmap Blend Ids",
                byte_size,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        );
        let mut hole_mask = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[MESHI GFX TERRAIN] Clipmap Hole Masks",
                byte_size,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        );

        Self::fill_default_height(height.as_slice_mut::<f32>());
        Self::fill_default_normal(normal.as_slice_mut::<f32>());
        Self::fill_default_blend(blend.as_slice_mut::<f32>());
        Self::fill_default_blend_ids(blend_ids.as_slice_mut::<f32>());
        Self::fill_default_hole_mask(hole_mask.as_slice_mut::<f32>());

        TerrainClipmapBuffers {
            grid_size,
            tile_texel_count,
            tile_count,
            height,
            normal,
            blend,
            blend_ids,
            hole_mask,
        }
    }

    fn refresh_clipmap_tiles(
        &mut self,
        selection: &[TerrainRenderObject],
        tile_indices: &HashMap<TerrainChunkKey, u32>,
    ) {
        if self.clipmap_buffers.is_none() {
            if let Some(settings) = self.terrain_settings.clone() {
                self.ensure_clipmap_buffers(&settings);
            }
        }
        let Some(buffers) = self.clipmap_buffers.as_mut() else {
            return;
        };

        let tile_texel_count = buffers.tile_texel_count as usize;
        let tile_data_len = tile_texel_count * 4;
        let height_slice = buffers.height.as_slice_mut::<f32>();
        let normal_slice = buffers.normal.as_slice_mut::<f32>();
        let blend_slice = buffers.blend.as_slice_mut::<f32>();
        let blend_ids_slice = buffers.blend_ids.as_slice_mut::<f32>();
        let hole_mask_slice = buffers.hole_mask.as_slice_mut::<f32>();

        for object in selection {
            let key = TerrainChunkKey {
                project_key: object.artifact.project_key.clone(),
                coords: object.artifact.chunk_coords,
            };
            let Some(&tile_index) = tile_indices.get(&key) else {
                continue;
            };
            let slot_index = tile_index as usize;
            if slot_index >= self.clipmap_slots.len() {
                continue;
            }
            let slot = self.clipmap_slots[slot_index];
            let needs_update = slot
                .map(|slot| {
                    slot.hash != object.artifact.content_hash
                        || slot.lod != object.artifact.lod
                        || slot.grid_size != object.artifact.grid_size
                })
                .unwrap_or(true);
            if !needs_update && !self.clipmap_cache_dirty {
                continue;
            }

            let range_start = slot_index * tile_data_len;
            let range_end = range_start + tile_data_len;
            let mut updated = true;
            updated &= self.copy_cached_tile(
                "height",
                object.artifact.content_hash,
                object.artifact.grid_size,
                &mut height_slice[range_start..range_end],
            );
            updated &= self.copy_cached_tile(
                "normal",
                object.artifact.content_hash,
                object.artifact.grid_size,
                &mut normal_slice[range_start..range_end],
            );
            updated &= self.copy_cached_tile(
                "blend",
                object.artifact.content_hash,
                object.artifact.grid_size,
                &mut blend_slice[range_start..range_end],
            );
            updated &= self.copy_cached_tile(
                "blend_ids",
                object.artifact.content_hash,
                object.artifact.grid_size,
                &mut blend_ids_slice[range_start..range_end],
            );
            updated &= self.copy_cached_tile(
                "hole_mask",
                object.artifact.content_hash,
                object.artifact.grid_size,
                &mut hole_mask_slice[range_start..range_end],
            );

            if !updated {
                if slot.is_none() {
                    Self::fill_default_height(&mut height_slice[range_start..range_end]);
                    Self::fill_default_normal(&mut normal_slice[range_start..range_end]);
                    Self::fill_default_blend(&mut blend_slice[range_start..range_end]);
                    Self::fill_default_blend_ids(&mut blend_ids_slice[range_start..range_end]);
                    Self::fill_default_hole_mask(&mut hole_mask_slice[range_start..range_end]);
                    self.clipmap_buffers_dirty = true;
                }
                self.queue_texture_build(&object.artifact);
                continue;
            }

            self.clipmap_slots[slot_index] = Some(TerrainClipmapSlot {
                hash: object.artifact.content_hash,
                lod: object.artifact.lod,
                grid_size: object.artifact.grid_size,
            });
            self.clipmap_buffers_dirty = true;
        }
    }

    fn sync_clipmap_buffers(&mut self) -> Option<CommandStream<Executable>> {
        if !self.clipmap_buffers_dirty {
            return None;
        }
        let buffers = self.clipmap_buffers.as_mut()?;
        self.clipmap_buffers_dirty = false;
        let mut stream = CommandStream::new().begin();
        stream = stream
            .combine(buffers.height.sync_up())
            .combine(buffers.normal.sync_up())
            .combine(buffers.blend.sync_up())
            .combine(buffers.blend_ids.sync_up())
            .combine(buffers.hole_mask.sync_up());
        Some(stream.end())
    }

    fn copy_cached_tile(
        &self,
        kind: &'static str,
        hash: u64,
        grid: [u32; 2],
        dest: &mut [f32],
    ) -> bool {
        let Some(data) = self.texture_data_cache.get(&TerrainTextureCacheKey {
            kind,
            hash,
            grid,
        }) else {
            return false;
        };
        if data.len() != dest.len() {
            return false;
        }
        dest.copy_from_slice(data);
        true
    }

    fn fill_default_height(dest: &mut [f32]) {
        for chunk in dest.chunks_exact_mut(4) {
            chunk[0] = 0.0;
            chunk[1] = 0.0;
            chunk[2] = 0.0;
            chunk[3] = 1.0;
        }
    }

    fn fill_default_normal(dest: &mut [f32]) {
        for chunk in dest.chunks_exact_mut(4) {
            chunk[0] = 0.0;
            chunk[1] = 1.0;
            chunk[2] = 0.0;
            chunk[3] = 1.0;
        }
    }

    fn fill_default_blend(dest: &mut [f32]) {
        for chunk in dest.chunks_exact_mut(4) {
            chunk[0] = 1.0;
            chunk[1] = 0.0;
            chunk[2] = 0.0;
            chunk[3] = 0.0;
        }
    }

    fn fill_default_blend_ids(dest: &mut [f32]) {
        for chunk in dest.chunks_exact_mut(4) {
            chunk[0] = 0.0;
            chunk[1] = 0.0;
            chunk[2] = 0.0;
            chunk[3] = 0.0;
        }
    }

    fn fill_default_hole_mask(dest: &mut [f32]) {
        for chunk in dest.chunks_exact_mut(4) {
            chunk[0] = 0.0;
            chunk[1] = 0.0;
            chunk[2] = 0.0;
            chunk[3] = 0.0;
        }
    }

    fn build_heightmap_data(
        artifact: &TerrainChunkArtifact,
        grid_x: u32,
        grid_y: u32,
    ) -> Vec<f32> {
        let mut data = Vec::with_capacity((grid_x * grid_y * 4) as usize);
        if artifact.heights.len() == (grid_x * grid_y) as usize {
            for height in &artifact.heights {
                data.extend_from_slice(&[*height, 0.0, 0.0, 1.0]);
            }
        } else {
            data.resize((grid_x * grid_y * 4) as usize, 0.0);
        }
        data
    }

    fn build_normalmap_data(
        artifact: &TerrainChunkArtifact,
        grid_x: u32,
        grid_y: u32,
    ) -> Vec<f32> {
        let mut data = Vec::with_capacity((grid_x * grid_y * 4) as usize);
        if artifact.normals.len() == (grid_x * grid_y) as usize {
            for normal in &artifact.normals {
                data.extend_from_slice(&[normal[0], normal[1], normal[2], 1.0]);
            }
        } else {
            data.resize((grid_x * grid_y * 4) as usize, 0.0);
            for chunk in data.chunks_exact_mut(4) {
                chunk[0] = 0.0;
                chunk[1] = 1.0;
                chunk[2] = 0.0;
                chunk[3] = 1.0;
            }
        }
        data
    }

    fn build_blendmap_data(
        artifact: &TerrainChunkArtifact,
        grid_x: u32,
        grid_y: u32,
    ) -> Vec<f32> {
        let mut data = Vec::with_capacity((grid_x * grid_y * 4) as usize);
        let expected = (grid_x * grid_y) as usize;
        if let Some(weights) = artifact.material_weights.as_deref() {
            if weights.len() == expected {
                for weight in weights {
                    data.extend_from_slice(weight);
                }
            }
        }
        if data.is_empty() {
            data.resize((grid_x * grid_y * 4) as usize, 0.0);
            for chunk in data.chunks_exact_mut(4) {
                chunk[0] = 1.0;
            }
        }
        data
    }

    fn build_blend_ids_data(
        artifact: &TerrainChunkArtifact,
        grid_x: u32,
        grid_y: u32,
    ) -> Vec<f32> {
        let mut data = Vec::with_capacity((grid_x * grid_y * 4) as usize);
        let expected = (grid_x * grid_y) as usize;
        if let Some(ids) = artifact.material_ids.as_deref() {
            if ids.len() == expected {
                for id in ids {
                    data.extend_from_slice(&[
                        id[0] as f32,
                        id[1] as f32,
                        id[2] as f32,
                        id[3] as f32,
                    ]);
                }
            }
        }
        if data.is_empty() {
            data.resize((grid_x * grid_y * 4) as usize, 0.0);
        }
        data
    }

    fn build_hole_mask_data(
        artifact: &TerrainChunkArtifact,
        grid_x: u32,
        grid_y: u32,
    ) -> Vec<f32> {
        let mut data = Vec::with_capacity((grid_x * grid_y * 4) as usize);
        let expected = (grid_x * grid_y) as usize;
        if artifact.hole_masks.len() == expected {
            for mask in &artifact.hole_masks {
                let value = if *mask == 0 { 0.0 } else { 1.0 };
                data.extend_from_slice(&[value, 0.0, 0.0, 0.0]);
            }
        }
        if data.is_empty() {
            data.resize((grid_x * grid_y * 4) as usize, 0.0);
        }
        data
    }

    fn queue_texture_build(&mut self, artifact: &TerrainChunkArtifact) {
        let grid_x = artifact.grid_size[0];
        let grid_y = artifact.grid_size[1];
        if grid_x == 0 || grid_y == 0 {
            return;
        }
        let work_key = TerrainTextureWorkKey {
            hash: artifact.content_hash,
            grid: [grid_x, grid_y],
        };
        if self.texture_work_pending.contains(&work_key) {
            return;
        }
        let kinds_ready = ["height", "normal", "blend", "blend_ids", "hole_mask"]
            .iter()
            .all(|kind| {
                self.texture_data_cache.contains_key(&TerrainTextureCacheKey {
                    kind,
                    hash: work_key.hash,
                    grid: work_key.grid,
                })
            });
        if kinds_ready {
            return;
        }

        self.texture_work_pending.insert(work_key);
        self.texture_work_requests.push(TerrainTextureWorkItem {
            artifact: Arc::new(artifact.clone()),
            work_key,
        });
    }

    fn drain_texture_work(&mut self) -> bool {
        let mut had_results = false;
        while let Some(result) = self.texture_work_results.pop() {
            for (key, data) in result.textures {
                self.texture_data_cache.entry(key).or_insert(data);
            }
            self.texture_work_pending.remove(&result.work_key);
            had_results = true;
        }
        if had_results {
            self.terrain_dirty = true;
        }
        had_results
    }

    fn allocate_terrain_material(
        &mut self,
        state: &mut BindlessState,
    ) -> (Handle<Material>, Material) {
        let mut material_handle = Handle::default();
        let mut material = Material::default();
        material.base_color_texture_id = u32::MAX;
        material.normal_texture_id = u32::MAX;
        material.metallic_roughness_texture_id = u32::MAX;
        material.occlusion_texture_id = u32::MAX;
        material.emissive_texture_id = u32::MAX;
        material.material_flags |= MATERIAL_FLAG_VERTEX_COLOR as u32;
        state
            .reserved_mut::<ReservedBindlessMaterials, _>("meshi_bindless_materials", |materials| {
                material_handle = materials.add_material();
                *materials.material_mut(material_handle) = material;
            })
            .expect("Failed to allocate terrain material");

        (material_handle, material)
    }

    fn allocate_terrain_transform(
        &self,
        transform: Mat4,
        state: &mut BindlessState,
    ) -> Handle<Transformation> {
        let mut handle = Handle::default();
        state
            .reserved_mut::<ReservedBindlessTransformations, _>(
                "meshi_bindless_transformations",
                |transforms| {
                    handle = transforms.add_transform();
                    transforms.transform_mut(handle).transform = transform;
                },
            )
            .expect("allocate terrain transform");
        handle
    }

    fn update_terrain_transform(
        &self,
        handle: Handle<Transformation>,
        transform: Mat4,
        state: &mut BindlessState,
    ) {
        if !handle.valid() {
            return;
        }
        state
            .reserved_mut::<ReservedBindlessTransformations, _>(
                "meshi_bindless_transformations",
                |transforms| {
                    transforms.transform_mut(handle).transform = transform;
                },
            )
            .expect("update terrain transform");
    }

    fn release_terrain_transform(&self, handle: Handle<Transformation>, state: &mut BindlessState) {
        if !handle.valid() {
            return;
        }
        state
            .reserved_mut::<ReservedBindlessTransformations, _>(
                "meshi_bindless_transformations",
                |transforms| {
                    transforms.remove_transform(handle);
                },
            )
            .expect("release terrain transform");
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

    fn geometry_cache() -> &'static Mutex<HashMap<TerrainGeometryCacheKey, TerrainPlaneGeometry>> {
        static CACHE: OnceLock<Mutex<HashMap<TerrainGeometryCacheKey, TerrainPlaneGeometry>>> =
            OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

}

impl Drop for TerrainRenderer {
    fn drop(&mut self) {
        self.texture_worker_running.store(false, Ordering::Release);
        if let Some(handle) = self.texture_worker_handle.take() {
            let _ = handle.join();
        }
    }
}
