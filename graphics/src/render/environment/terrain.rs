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
use glam::{Mat3, Mat4, Vec2, Vec3, Vec4};
use noren::DB;
use noren::RDBFile;
use noren::meta::{DeviceMaterial, DeviceMesh, DeviceModel};
use noren::rdb::primitives::Vertex;
use noren::rdb::terrain::{
    TerrainCameraInfo, TerrainChunk, TerrainChunkArtifact, TerrainFrustum, TerrainProjectSettings,
    chunk_artifact_entry, chunk_coord_key, lod_key, project_settings_entry,
};
use noren::rdb::{DeviceGeometry, DeviceGeometryLayer, HostGeometry};
use std::collections::{HashMap, HashSet};
use std::ptr::NonNull;
use tracing::{info, warn};

use crate::render::deferred::PerDrawData;
use crate::render::gpu_draw_builder::{GPUDrawBuilder, GPUDrawBuilderInfo};
use crate::terrain_loader;
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
}

pub const TERRAIN_DRAW_BIN: u32 = 0;

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
    lod_sources: HashMap<TerrainChunkKey, Vec<TerrainRenderObject>>,
    active_chunk_lods: HashMap<TerrainChunkKey, String>,
    terrain_rdb: Option<NonNull<RDBFile>>,
    terrain_project_key: Option<String>,
    terrain_settings: Option<TerrainProjectSettings>,
    terrain_render_objects: HashMap<String, TerrainRenderObject>,
}

#[derive(Clone)]
struct TerrainObjectEntry {
    transform_handle: Handle<Transformation>,
    draws: Vec<Handle<PerDrawData>>,
    draw_instances: Vec<TerrainDrawInstance>,
    content_hash: u64,
    geometry_entry: String,
    material_handle: Handle<Material>,
}

#[derive(Clone)]
struct TerrainDrawInstance {
    material: Handle<Material>,
    vertex_id: u32,
    vertex_count: u32,
    index_id: u32,
    index_count: u32,
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
            lod_sources: HashMap::new(),
            active_chunk_lods: HashMap::new(),
            terrain_rdb: None,
            terrain_project_key: None,
            terrain_settings: None,
            terrain_render_objects: HashMap::new(),
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
        self.terrain_render_objects.clear();
        self.lod_sources.clear();
        self.active_chunk_lods.clear();
    }

    pub fn set_project_key(&mut self, project_key: &str) {
        self.terrain_project_key = Some(project_key.to_string());
        self.terrain_settings = None;
        self.terrain_render_objects.clear();
        self.lod_sources.clear();
        self.active_chunk_lods.clear();
    }

    pub fn update(&mut self, camera: Handle<Camera>, state: &mut BindlessState) {
        let bump = crate::render::global_bump().get();
        let _frame_marker = bump.alloc(0u8);
        if self.terrain_project_key.is_some() {
            self.refresh_rdb_objects(camera);
        }
        if self.lod_sources.is_empty() {
            return;
        }

        let selection = self.select_lod_objects();
        let selection_keys = Self::selection_key_map(&selection);

        let Some(mut deferred) = self.deferred.take() else {
            return;
        };

        if selection_keys != self.active_chunk_lods {
            self.active_chunk_lods = selection_keys;
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
                "Loaded terrain settings for project '{}' (tile_size={}, tiles_per_chunk={:?})",
                project_key, settings.tile_size, settings.tiles_per_chunk
            );
            self.terrain_settings = Some(settings.clone());
        }

        let chunks = match db.fetch_terrain_chunks_from_view(&settings, &project_key, camera)
        {
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

        for mut artifact in chunks {
            let lod = 0;
            let coord_key = chunk_coord_key(artifact.chunk_coords[0], artifact.chunk_coords[1]);
            let entry = chunk_artifact_entry(&project_key, &coord_key, &lod_key(lod));
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
        for sources in self.lod_sources.values() {
            if sources.is_empty() {
                continue;
            }
            selected.push(self.select_lod_for_chunk(sources));
        }
        selected
    }

    fn select_lod_for_chunk(&self, sources: &[TerrainRenderObject]) -> TerrainRenderObject {
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
        let target_lod = ((distance / lod_step).floor() as u8).min(max_lod);

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

    fn world_bounds(object: &TerrainRenderObject) -> (Vec3, Vec3) {
        let bounds_min = Vec3::from(object.artifact.bounds_min);
        let bounds_max = Vec3::from(object.artifact.bounds_max);
        let center_local = (bounds_min + bounds_max) * 0.5;
        let extent_local = (bounds_max - bounds_min) * 0.5;
        let world_center = object.transform.transform_point3(center_local);
        let basis = Mat3::from_mat4(object.transform);
        let abs_basis = Mat3::from_cols(basis.x_axis.abs(), basis.y_axis.abs(), basis.z_axis.abs());
        let world_extent = abs_basis * extent_local;
        (world_center, world_extent)
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

            let Some((model, geometry_entry, material_handle)) =
                self.build_terrain_model(&object.key, &object.artifact, deferred, state)
            else {
                warn!("Failed to build terrain render object '{}'.", object.key);
                continue;
            };

            let transform_handle = self.allocate_terrain_transform(object.transform, state);
            let draw_instances = Self::build_draw_instances(&model);
            let draws = self.register_draw_instances(
                &draw_instances,
                transform_handle,
                &mut deferred.draw_builder,
            );

            deferred.objects.insert(
                object.key.clone(),
                TerrainObjectEntry {
                    transform_handle,
                    draws,
                    draw_instances,
                    content_hash: object.artifact.content_hash,
                    geometry_entry,
                    material_handle,
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
            if !self.chunk_visible(object) {
                continue;
            }
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
        let shaders = miso::gpudeferred(&[]);

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

        if entry.material_handle.valid() {
            state
                .reserved_mut::<ReservedBindlessMaterials, _>(
                    "meshi_bindless_materials",
                    |materials| materials.remove_material(entry.material_handle),
                )
                .expect("Failed to release terrain material");
        }

        let Some(mut db) = deferred.db else {
            return;
        };
        if let Err(err) = unsafe { db.as_mut() }
            .geometry_mut()
            .unref_entry(&entry.geometry_entry)
        {
            warn!(
                "Failed to release terrain geometry '{}': {err:?}",
                entry.geometry_entry
            );
        }
    }

    fn release_terrain_draws(entry: &mut TerrainObjectEntry, draw_builder: &mut GPUDrawBuilder) {
        for draw in entry.draws.drain(..) {
            draw_builder.release_draw(draw);
        }
    }

    fn build_draw_instances(model: &DeviceModel) -> Vec<TerrainDrawInstance> {
        model
            .meshes
            .iter()
            .map(|mesh| TerrainDrawInstance {
                material: mesh
                    .material
                    .as_ref()
                    .and_then(|material| material.furikake_material_handle)
                    .unwrap_or_default(),
                vertex_id: mesh.geometry.base.furikake_vertex_id.unwrap(),
                vertex_count: mesh.geometry.base.vertex_count,
                index_id: mesh.geometry.base.furikake_index_id.unwrap(),
                index_count: mesh.geometry.base.index_count.unwrap(),
            })
            .collect()
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
                ))
            })
            .collect()
    }

    fn build_terrain_model(
        &mut self,
        key: &str,
        artifact: &TerrainChunkArtifact,
        deferred: &mut TerrainDeferredResources,
        state: &mut BindlessState,
    ) -> Option<(DeviceModel, String, Handle<Material>)> {
        if artifact.vertices.is_empty() || artifact.indices.is_empty() {
            warn!("Terrain artifact '{key}' has no geometry data.");
            return None;
        }

        let geometry_entry = format!(
            "terrain/runtime/{key}/lod{}-{:016x}",
            artifact.lod, artifact.content_hash
        );
        let host_geometry = HostGeometry {
            vertices: artifact.vertices.clone(),
            indices: Some(artifact.indices.clone()),
            ..Default::default()
        }
        .with_counts();

        let Some(mut db) = deferred.db else {
            warn!("No database available for terrain upload.");
            return None;
        };
        let db = unsafe { db.as_mut() };
        let geometry_store = db.geometry_mut();
        let _ = geometry_store.unref_entry(&geometry_entry);
        let mut geometry =
            match geometry_store.enter_gpu_geometry(&geometry_entry, host_geometry.clone()) {
                Ok(geometry) => geometry,
                Err(err) => {
                    warn!("Failed to upload terrain geometry '{geometry_entry}': {err:?}");
                    return None;
                }
            };

        if !self.register_furikake_geometry(&mut geometry, &host_geometry, state) {
            warn!("Failed to register furikake geometry for terrain '{geometry_entry}'.");
            return None;
        }

        let (material_handle, material) = self.allocate_terrain_material(state);
        let device_material = DeviceMaterial::new(Vec::new(), material, Some(material_handle));
        let mesh = DeviceMesh::new(geometry, Vec::new(), Some(device_material));
        let model = DeviceModel {
            name: format!("terrain/{key}"),
            meshes: vec![mesh],
            rig: None,
        };

        Some((model, geometry_entry, material_handle))
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

    fn register_furikake_geometry(
        &mut self,
        geometry: &mut DeviceGeometry,
        host: &HostGeometry,
        state: &mut BindlessState,
    ) -> bool {
        if !self.register_furikake_geometry_layer(
            &mut geometry.base,
            &host.vertices,
            host.indices.as_deref(),
            state,
        ) {
            return false;
        }

        if geometry.lods.len() != host.lods.len() {
            warn!("Terrain geometry lod count mismatch.");
            return false;
        }

        for (layer, source) in geometry.lods.iter_mut().zip(&host.lods) {
            if !self.register_furikake_geometry_layer(
                layer,
                &source.vertices,
                source.indices.as_deref(),
                state,
            ) {
                return false;
            }
        }

        true
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
        let uses_skinning = vertices.iter().any(|vertex| {
            vertex.joint_weights.iter().any(|weight| *weight != 0.0)
                || vertex.joint_indices.iter().any(|index| *index != 0)
        });

        if uses_skinning {
            VertexBufferSlot::Skeleton
        } else {
            VertexBufferSlot::Skeleton
        }
    }
}
