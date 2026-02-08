use super::EnvironmentRendererInfo;
use bento::builder::{AttachmentDesc, CSOBuilder, PSO, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use dashi::cmd::{Executable, PendingGraphics};
use dashi::driver::command::{Dispatch, Draw, DrawIndexedIndirect};
use dashi::{
    AspectMask, Buffer, BufferInfo, BufferUsage, CommandStream, Context, DynamicAllocator, Format,
    Handle, ImageInfo, ImageViewType, MemoryVisibility, SampleCount, ShaderResource,
    SubresourceRange, UsageBits, Viewport,
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
use std::collections::{HashMap, HashSet};
use std::ptr::NonNull;
use tracing::{info, warn};

use crate::render::deferred::PerDrawData;
use crate::render::gpu_draw_builder::{GPUDrawBuilder, GPUDrawBuilderInfo};
use crate::render::image_pager::{ImagePager, ImagePagerBackend, ImagePagerKey, InlineImageKey};
use crate::terrain_loader;
use furikake::reservations::bindless_camera::ReservedBindlessCamera;
use furikake::reservations::bindless_indices::ReservedBindlessIndices;
use furikake::reservations::bindless_materials::ReservedBindlessMaterials;
use furikake::reservations::bindless_textures::ReservedBindlessTextures;
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
}

pub const TERRAIN_DRAW_BIN: u32 = 0;
const TERRAIN_REFRESH_FRAME_INTERVAL: u64 = 4;
const TERRAIN_SETTINGS_POLL_INTERVAL: u64 = 30;
const TERRAIN_CAMERA_POSITION_EPSILON: f32 = 0.25;
const TERRAIN_VIEW_PROJECTION_EPSILON: f32 = 1e-3;
const TERRAIN_LOD_HYSTERESIS_RATIO: f32 = 0.25;

impl Default for TerrainInfo {
    fn default() -> Self {
        let clipmap_resolution = 8;
        Self {
            patch_size: 64.0,
            lod_levels: 4,
            clipmap_resolution,
            max_tiles: clipmap_resolution * clipmap_resolution,
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
    image_pager: ImagePager,
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
    last_refresh_camera_position: Option<Vec3>,
    last_refresh_view_projection: Option<Mat4>,
}

#[derive(Clone)]
struct TerrainObjectEntry {
    transform_handle: Handle<Transformation>,
    draws: Vec<Handle<PerDrawData>>,
    draw_instances: Vec<TerrainDrawInstance>,
    content_hash: u64,
    material_handle: Handle<Material>,
    textures: TerrainTextureSet,
}

#[derive(Clone)]
struct TerrainDrawInstance {
    material: Handle<Material>,
    vertex_id: u32,
    vertex_count: u32,
    index_id: u32,
    index_count: u32,
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

#[derive(Clone)]
struct TerrainTexture {
    key: ImagePagerKey,
    handle: u32,
}

#[derive(Clone, Default)]
struct TerrainTextureSet {
    height: Option<TerrainTexture>,
    normal: Option<TerrainTexture>,
    blend: Option<TerrainTexture>,
    blend_ids: Option<TerrainTexture>,
}

impl TerrainTextureSet {
    fn height_handle(&self) -> u32 {
        self.height.as_ref().map(|tex| tex.handle).unwrap_or(u32::MAX)
    }

    fn normal_handle(&self) -> u32 {
        self.normal.as_ref().map(|tex| tex.handle).unwrap_or(u32::MAX)
    }

    fn blend_handle(&self) -> u32 {
        self.blend.as_ref().map(|tex| tex.handle).unwrap_or(u32::MAX)
    }

    fn blend_ids_handle(&self) -> u32 {
        self.blend_ids
            .as_ref()
            .map(|tex| tex.handle)
            .unwrap_or(u32::MAX)
    }
}

struct TerrainEntryBuild {
    draw_instances: Vec<TerrainDrawInstance>,
    material_handle: Handle<Material>,
    textures: TerrainTextureSet,
}

struct TerrainImageBackend<'a> {
    state: &'a mut BindlessState,
}

impl ImagePagerBackend for TerrainImageBackend<'_> {
    fn reserve_handle(&mut self) -> u32 {
        u32::MAX
    }

    fn register_image(&mut self, _handle: u32, view: dashi::ImageView) -> u32 {
        self.state.add_texture(view) as u32
    }

    fn release_image(&mut self, handle: u32) {
        if handle == u32::MAX {
            return;
        }
        let _ = self.state.reserved_mut::<ReservedBindlessTextures, _>(
            "meshi_bindless_textures",
            |textures| {
                textures.remove_texture(handle as u16);
            },
        );
    }
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
            .add_reserved_table_variable(state, "meshi_bindless_textures")
            .unwrap()
            .add_reserved_table_variable(state, "meshi_bindless_materials")
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
            image_pager: ImagePager::new(),
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
            last_refresh_camera_position: None,
            last_refresh_view_projection: None,
        }
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

        let pipeline =
            Self::build_deferred_pipeline(ctx, state, sample_count, &draw_builder, dynamic);

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
        self.terrain_dirty = true;
    }

    pub fn set_project_key(&mut self, project_key: &str) {
        self.terrain_project_key = Some(project_key.to_string());
        self.terrain_settings = None;
        self.terrain_settings_dirty = true;
        self.terrain_render_objects.clear();
        self.lod_sources.clear();
        self.active_chunk_lods.clear();
        self.active_chunk_lod_levels.clear();
        self.terrain_dirty = true;
    }

    pub fn update(&mut self, camera: Handle<Camera>, state: &mut BindlessState) {
        let bump = crate::render::global_bump().get();
        let _frame_marker = bump.alloc(0u8);
        self.refresh_frame_index = self.refresh_frame_index.saturating_add(1);
        self.update_camera_state(camera, state);
        if self.terrain_project_key.is_some() && self.should_refresh_terrain() {
            self.refresh_rdb_objects(camera);
            self.last_refresh_frame = self.refresh_frame_index;
            self.last_settings_poll_frame = self.refresh_frame_index;
            self.last_refresh_camera_position = Some(self.camera_position);
            self.last_refresh_view_projection = self.view_projection;
            self.terrain_dirty = false;
        }
        if self.terrain_settings_dirty {
            if let Some(settings) = self.terrain_settings.clone() {
                self.ensure_static_geometry(&settings, state);
            }
            self.terrain_settings_dirty = false;
        }
        if self.lod_sources.is_empty() {
            return;
        }

        let selection = self.select_lod_objects();
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
            self.active_chunk_lods = selection_keys;
            self.active_chunk_lod_levels = selection
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
            self.apply_render_objects(&selection, &mut deferred, state);
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

    fn refresh_rdb_objects(&mut self, camera: Handle<Camera>) {
        let Some(project_key) = self.terrain_project_key.clone() else {
            return;
        };
        let Some(mut db_ptr) = self.deferred.as_ref().and_then(|deferred| deferred.db) else {
            warn!("Terrain refresh skipped: no terrain DB available.");
            return;
        };

        let settings_entry = project_settings_entry(&project_key);
        let db = unsafe { db_ptr.as_mut() };
        let settings = match db.terrain_mut().fetch_project_settings(&settings_entry) {
            Ok(settings) => settings,
            Err(err) => {
                warn!(
                    "Failed to load terrain project settings '{}': {err:?}",
                    settings_entry
                );
                if let Some(mut rdb_ptr) = self.terrain_rdb {
                    if let Ok(settings) =
                        unsafe { rdb_ptr.as_mut() }.fetch::<TerrainProjectSettings>(&settings_entry)
                    {
                        settings
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
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
        }

        let chunks = match db.fetch_terrain_chunks_from_view(&settings, &project_key, camera) {
            Ok(chunks) => chunks,
            Err(err) => {
                warn!("Terrain refresh: failed to fetch chunks: {err:?}");
                return;
            }
        };

        let mut next_objects = HashMap::new();
        let mut ordered_objects = Vec::new();
        let mut loaded_artifacts = 0usize;
        let mut changed = settings_changed || chunks.len() != self.terrain_render_objects.len();

        let lod_levels = self.lod_levels.min(u32::from(u8::MAX)) as u8;

        for base_artifact in chunks {
            let coord_key =
                chunk_coord_key(base_artifact.chunk_coords[0], base_artifact.chunk_coords[1]);
            for lod in 0..lod_levels {
                let entry = chunk_artifact_entry(&project_key, &coord_key, &lod_key(lod));
                let mut artifact = if lod == base_artifact.lod {
                    base_artifact.clone()
                } else {
                    self.fetch_lod_artifact(&entry).unwrap_or_else(|| {
                        let mut fallback = base_artifact.clone();
                        fallback.lod = lod;
                        fallback
                    })
                };
                artifact.lod = lod;

                if let Some(existing) = self.terrain_render_objects.get(&entry).cloned() {
                    if existing.artifact.content_hash == artifact.content_hash {
                        let mut updated = existing.clone();
                        updated.transform = terrain_loader::terrain_chunk_transform(
                            &settings,
                            updated.artifact.chunk_coords,
                            updated.artifact.bounds_min,
                        );
                        next_objects.insert(entry.clone(), updated.clone());
                        ordered_objects.push(updated);
                        continue;
                    }
                }

                if !changed {
                    changed = self
                        .terrain_render_objects
                        .get(&entry)
                        .map(|existing| existing.artifact.content_hash != artifact.content_hash)
                        .unwrap_or(true);
                }
                let object = terrain_loader::terrain_render_object_from_artifact(
                    &settings,
                    entry.clone(),
                    artifact,
                );
                next_objects.insert(entry.clone(), object.clone());
                ordered_objects.push(object);
                loaded_artifacts += 1;
            }
        }

        if !changed {
            changed = self
                .terrain_render_objects
                .keys()
                .any(|key| !next_objects.contains_key(key));
        }

        if !changed {
            return;
        }

        self.terrain_render_objects = next_objects;
        self.lod_sources = Self::group_lod_sources(&ordered_objects);
        info!(
            "Terrain refresh: loaded_artifacts={}, total_objects={}",
            loaded_artifacts,
            self.terrain_render_objects.len()
        );
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

    fn select_lod_objects(&self) -> Vec<TerrainRenderObject> {
        let mut selected = Vec::with_capacity(self.lod_sources.len());
        for (key, sources) in &self.lod_sources {
            if sources.is_empty() {
                continue;
            }
            let previous_lod = self.active_chunk_lod_levels.get(key).copied();
            selected.push(self.select_lod_for_chunk(sources, previous_lod));
        }
        selected
    }

    fn select_lod_for_chunk(
        &self,
        sources: &[TerrainRenderObject],
        previous_lod: Option<u8>,
    ) -> TerrainRenderObject {
        let reference = &sources[0];
        let (center_world, extent_world) = Self::world_bounds(reference);
        let distance = (center_world - self.camera_position).length();

        let chunk_extent = extent_world.x.max(extent_world.y).max(1.0);
        let lod_step = (chunk_extent * 2.0).max(1.0);
        let max_lod = sources
            .iter()
            .map(|source| source.artifact.lod)
            .max()
            .unwrap_or(0);
        let mut target_lod = ((distance / lod_step).floor() as u8).min(max_lod);
        if let Some(previous_lod) = previous_lod {
            let hysteresis = lod_step * TERRAIN_LOD_HYSTERESIS_RATIO;
            let min_distance = previous_lod as f32 * lod_step - hysteresis;
            let max_distance = (previous_lod as f32 + 1.0) * lod_step + hysteresis;
            if (min_distance..=max_distance).contains(&distance) {
                target_lod = previous_lod.min(max_lod);
            }
        }

        let mut best = reference.clone();
        let mut best_delta = reference.artifact.lod.abs_diff(target_lod);
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
        let refresh_due = self
            .refresh_frame_index
            .saturating_sub(self.last_refresh_frame)
            >= TERRAIN_REFRESH_FRAME_INTERVAL;
        let settings_poll_due = self
            .refresh_frame_index
            .saturating_sub(self.last_settings_poll_frame)
            >= TERRAIN_SETTINGS_POLL_INTERVAL;

        if self.terrain_dirty {
            return true;
        }

        if self.terrain_render_objects.is_empty() {
            return settings_poll_due || refresh_due;
        }

        if settings_poll_due {
            return true;
        }

        if !refresh_due {
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

        for plane in &planes[..4] {
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

    fn apply_render_objects(
        &mut self,
        objects: &[TerrainRenderObject],
        deferred: &mut TerrainDeferredResources,
        state: &mut BindlessState,
    ) {
        let mut next_keys = HashSet::with_capacity(objects.len());
        for object in objects {
            next_keys.insert(object.key.clone());
        }

        let mut removals = Vec::new();
        for (key, entry) in &deferred.objects {
            if !next_keys.contains(key) {
                removals.push((key.clone(), entry.clone()));
            }
        }

        for (key, entry) in removals {
            self.release_terrain_entry(&entry, deferred, state);
            deferred.objects.remove(&key);
        }

        for object in objects {
            if let Some(entry) = deferred.objects.get(&object.key).cloned() {
                if entry.content_hash == object.artifact.content_hash {
                    self.update_terrain_transform(entry.transform_handle, object.transform, state);
                    continue;
                }

                self.release_terrain_entry(&entry, deferred, state);
                deferred.objects.remove(&object.key);
            }

            let Some(entry_build) =
                self.build_terrain_entry(&object.key, &object.artifact, state)
            else {
                warn!("Failed to build terrain render object '{}'.", object.key);
                continue;
            };

            let transform_handle = self.allocate_terrain_transform(object.transform, state);
            let draw_instances = entry_build.draw_instances.clone();
            let draws = self.register_draw_instances(
                &draw_instances,
                transform_handle,
                &entry_build.textures,
                &mut deferred.draw_builder,
            );

            deferred.objects.insert(
                object.key.clone(),
                TerrainObjectEntry {
                    transform_handle,
                    draws,
                    draw_instances,
                    content_hash: object.artifact.content_hash,
                    material_handle: entry_build.material_handle,
                    textures: entry_build.textures,
                },
            );
        }
    }

    fn update_visibility(
        &mut self,
        objects: &[TerrainRenderObject],
        deferred: &mut TerrainDeferredResources,
    ) {
        let mut visible_keys = HashSet::with_capacity(objects.len());
        let mut to_register = Vec::new();
        for object in objects {
//            if !self.chunk_visible(object) {
//                continue;
//            }
            visible_keys.insert(object.key.clone());
            if let Some(entry) = deferred.objects.get(&object.key) {
                if entry.draws.is_empty() {
                    to_register.push(object.key.clone());
                }
            }
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
                &entry.textures,
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

            return stream
                .prepare_buffer(self.clipmap_buffer, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(self.draw_args_buffer, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(self.instance_buffer, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(self.heightmap_buffer, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(self.meshlet_buffer, UsageBits::COMPUTE_SHADER)
                .dispatch(&Dispatch {
                    x: 1,
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
        let Some(deferred) = &mut self.deferred else {
            return CommandStream::<PendingGraphics>::subdraw();
        };

        #[repr(C)]
        struct PerSceneData {
            camera: Handle<Camera>,
        }

        let mut alloc = dynamic
            .bump()
            .expect("Failed to allocate terrain per-scene data");
        alloc.slice::<PerSceneData>()[0].camera = camera;

        CommandStream::<PendingGraphics>::subdraw()
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
        self.release_terrain_textures(&entry.textures, state);

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
        textures: &TerrainTextureSet,
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
                    textures.height_handle(),
                    textures.normal_handle(),
                    textures.blend_handle(),
                    textures.blend_ids_handle(),
                ))
            })
            .collect()
    }

    fn build_terrain_entry(
        &mut self,
        key: &str,
        artifact: &TerrainChunkArtifact,
        state: &mut BindlessState,
    ) -> Option<TerrainEntryBuild> {
        let Some(settings) = self.terrain_settings.clone() else {
            return None;
        };
        self.ensure_static_geometry(&settings, state);
        let static_geometry = self.static_geometry.as_ref()?;
        let geometry = static_geometry
            .lods
            .get(artifact.lod as usize)
            .copied()
            .or_else(|| static_geometry.lods.last().copied())?;
        let textures = self.load_terrain_textures(key, artifact, state);
        let (material_handle, _material) = self.allocate_terrain_material(state);
        let draw_instances = vec![TerrainDrawInstance {
            material: material_handle,
            vertex_id: geometry.vertex_id,
            vertex_count: geometry.vertex_count,
            index_id: geometry.index_id,
            index_count: geometry.index_count,
        }];

        Some(TerrainEntryBuild {
            draw_instances,
            material_handle,
            textures,
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
            if let Some(geometry) = self.build_plane_geometry(settings, state, lod, lod_levels) {
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

    fn load_terrain_textures(
        &mut self,
        key: &str,
        artifact: &TerrainChunkArtifact,
        state: &mut BindlessState,
    ) -> TerrainTextureSet {
        let Some(mut ctx_ptr) = self.context else {
            return TerrainTextureSet::default();
        };
        let ctx = unsafe { ctx_ptr.as_mut() };
        let mut backend = TerrainImageBackend { state };

        let grid_x = artifact.grid_size[0];
        let grid_y = artifact.grid_size[1];
        if grid_x == 0 || grid_y == 0 {
            return TerrainTextureSet::default();
        }

        let mut textures = TerrainTextureSet::default();
        if let Some(view) = self.build_heightmap_view(ctx, key, artifact, grid_x, grid_y) {
            textures.height = Some(self.register_terrain_texture(
                key,
                "height",
                artifact.content_hash,
                view,
                &mut backend,
            ));
        }

        if let Some(view) = self.build_normalmap_view(ctx, key, artifact, grid_x, grid_y) {
            textures.normal = Some(self.register_terrain_texture(
                key,
                "normal",
                artifact.content_hash,
                view,
                &mut backend,
            ));
        }

        if let Some(view) = self.build_blendmap_view(ctx, key, artifact, grid_x, grid_y) {
            textures.blend = Some(self.register_terrain_texture(
                key,
                "blend",
                artifact.content_hash,
                view,
                &mut backend,
            ));
        }

        if let Some(view) = self.build_blend_ids_view(ctx, key, artifact, grid_x, grid_y) {
            textures.blend_ids = Some(self.register_terrain_texture(
                key,
                "blend_ids",
                artifact.content_hash,
                view,
                &mut backend,
            ));
        }

        textures
    }

    fn register_terrain_texture(
        &mut self,
        key: &str,
        kind: &str,
        hash: u64,
        view: dashi::ImageView,
        backend: &mut TerrainImageBackend<'_>,
    ) -> TerrainTexture {
        let key = ImagePagerKey::Inline(InlineImageKey {
            id: format!("terrain/{key}/{kind}/{hash:016x}"),
        });
        let handle = self
            .image_pager
            .register_inline_image(key.clone(), view, backend);
        TerrainTexture { key, handle }
    }

    fn build_heightmap_view(
        &self,
        ctx: &mut Context,
        key: &str,
        artifact: &TerrainChunkArtifact,
        grid_x: u32,
        grid_y: u32,
    ) -> Option<dashi::ImageView> {
        let mut data = Vec::with_capacity((grid_x * grid_y * 4) as usize);
        if artifact.heights.len() == (grid_x * grid_y) as usize {
            for height in &artifact.heights {
                data.extend_from_slice(&[*height, 0.0, 0.0, 1.0]);
            }
        } else {
            data.resize((grid_x * grid_y * 4) as usize, 0.0);
        }
        self.build_texture_view(ctx, &format!("terrain_{key}_height"), grid_x, grid_y, &data)
    }

    fn build_normalmap_view(
        &self,
        ctx: &mut Context,
        key: &str,
        artifact: &TerrainChunkArtifact,
        grid_x: u32,
        grid_y: u32,
    ) -> Option<dashi::ImageView> {
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
        self.build_texture_view(ctx, &format!("terrain_{key}_normal"), grid_x, grid_y, &data)
    }

    fn build_blendmap_view(
        &self,
        ctx: &mut Context,
        key: &str,
        artifact: &TerrainChunkArtifact,
        grid_x: u32,
        grid_y: u32,
    ) -> Option<dashi::ImageView> {
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
        self.build_texture_view(ctx, &format!("terrain_{key}_blend"), grid_x, grid_y, &data)
    }

    fn build_blend_ids_view(
        &self,
        ctx: &mut Context,
        key: &str,
        artifact: &TerrainChunkArtifact,
        grid_x: u32,
        grid_y: u32,
    ) -> Option<dashi::ImageView> {
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
        self.build_texture_view(ctx, &format!("terrain_{key}_blend_ids"), grid_x, grid_y, &data)
    }

    fn build_texture_view(
        &self,
        ctx: &mut Context,
        name: &str,
        width: u32,
        height: u32,
        data: &[f32],
    ) -> Option<dashi::ImageView> {
        if width == 0 || height == 0 || data.is_empty() {
            return None;
        }
        let info = ImageInfo {
            debug_name: name,
            dim: [width, height, 1],
            layers: 1,
            format: Format::RGBA32F,
            mip_levels: 1,
            initial_data: Some(bytemuck::cast_slice(data)),
            ..Default::default()
        };
        let image = ctx.make_image(&info).ok()?;
        Some(dashi::ImageView {
            img: image,
            aspect: AspectMask::Color,
            view_type: ImageViewType::Type2D,
            range: SubresourceRange::new(0, 1, 0, 1),
        })
    }

    fn release_terrain_textures(&mut self, textures: &TerrainTextureSet, state: &mut BindlessState) {
        let mut backend = TerrainImageBackend { state };
        for texture in [
            textures.height.as_ref(),
            textures.normal.as_ref(),
            textures.blend.as_ref(),
            textures.blend_ids.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            self.image_pager.release_by_key(&texture.key, &mut backend);
        }
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

}
