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
use glam::{Mat4, Vec3};
use noren::DB;
use noren::meta::{DeviceMaterial, DeviceMesh, DeviceModel};
use noren::rdb::primitives::Vertex;
use noren::rdb::terrain::TerrainChunkArtifact;
use noren::rdb::{DeviceGeometry, DeviceGeometryLayer, HostGeometry};
use std::collections::{HashMap, HashSet};
use std::ptr::NonNull;
use tracing::warn;

use crate::render::deferred::PerDrawData;
use crate::render::gpu_draw_builder::{GPUDrawBuilder, GPUDrawBuilderInfo};
use crate::render::scene::{GPUScene, SceneNodeType, SceneObject, SceneObjectInfo};
use furikake::reservations::bindless_indices::ReservedBindlessIndices;
use furikake::reservations::bindless_materials::ReservedBindlessMaterials;
use furikake::reservations::bindless_vertices::ReservedBindlessVertices;
use furikake::types::{Camera, Material, Transformation, VertexBufferSlot};

#[derive(Clone, Copy)]
pub struct TerrainInfo {
    pub patch_size: f32,
    pub lod_levels: u32,
    pub clipmap_resolution: u32,
    pub max_tiles: u32,
}

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
    use_depth: bool,
    deferred: Option<TerrainDeferredResources>,
}

#[derive(Clone)]
struct TerrainObjectEntry {
    scene_handle: Handle<SceneObject>,
    draws: Vec<Handle<PerDrawData>>,
    content_hash: u64,
    geometry_entry: String,
    material_handle: Handle<Material>,
}

struct TerrainDeferredResources {
    draw_builder: GPUDrawBuilder,
    pipeline: PSO,
    objects: HashMap<String, TerrainObjectEntry>,
    db: Option<NonNull<DB>>,
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

        let compute_pipeline = CSOBuilder::new()
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
            .ok();

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
            use_depth: info.use_depth,
            deferred: None,
        }
    }

    pub fn initialize_deferred(
        &mut self,
        ctx: &mut Context,
        state: &mut BindlessState,
        sample_count: SampleCount,
        cull_results: Handle<Buffer>,
        bin_counts: Handle<Buffer>,
        num_bins: u32,
        dynamic: &DynamicAllocator,
    ) {
        let draw_builder = GPUDrawBuilder::new(
            &GPUDrawBuilderInfo {
                name: "[MESHI] Deferred Terrain Draw Builder",
                ctx,
                cull_results,
                bin_counts,
                num_bins,
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

    pub fn update(&mut self, settings: TerrainFrameSettings) {
        self.camera_position = settings.camera_position;
    }

    pub fn build_deferred_draws(&mut self, bin: u32, view: u32) -> CommandStream<Executable> {
        let Some(deferred) = &mut self.deferred else {
            return CommandStream::new().begin().end();
        };

        deferred.draw_builder.build_draws(bin, view)
    }

    pub fn set_render_objects(
        &mut self,
        objects: &[TerrainRenderObject],
        scene: &mut GPUScene,
        state: &mut BindlessState,
    ) {
        let Some(mut deferred) = self.deferred.take() else {
            return;
        };

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
            self.release_terrain_entry(&entry, &mut deferred, scene, state);
            deferred.objects.remove(&key);
        }

        for object in objects {
            if let Some(entry) = deferred.objects.get(&object.key).cloned() {
                if entry.content_hash == object.artifact.content_hash {
                    scene.set_object_transform(entry.scene_handle, &object.transform);
                    continue;
                }

                self.release_terrain_entry(&entry, &mut deferred, scene, state);
                deferred.objects.remove(&object.key);
            }

            let Some((model, geometry_entry, material_handle)) =
                self.build_terrain_model(&object.key, &object.artifact, &mut deferred, state)
            else {
                warn!("Failed to build terrain render object '{}'.", object.key);
                continue;
            };

            let (scene_handle, transform_handle) = scene.register_object(&SceneObjectInfo {
                local: Default::default(),
                global: Default::default(),
                scene_mask: crate::render::deferred::PassMask::OPAQUE_GEOMETRY as u32
                    | crate::render::deferred::PassMask::SHADOW as u32,
                scene_type: SceneNodeType::Renderable,
            });

            scene.set_object_transform(scene_handle, &object.transform);

            let draws =
                self.register_terrain_draws(&model, scene_handle, transform_handle, &mut deferred);

            deferred.objects.insert(
                object.key.clone(),
                TerrainObjectEntry {
                    scene_handle,
                    draws,
                    content_hash: object.artifact.content_hash,
                    geometry_entry,
                    material_handle,
                },
            );
        }

        self.deferred = Some(deferred);
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
                    x: (self.max_tiles + 63) / 64,
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
        scene: &mut GPUScene,
        state: &mut BindlessState,
    ) {
        scene.release_object(entry.scene_handle);
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

    fn register_terrain_draws(
        &mut self,
        model: &DeviceModel,
        scene_handle: Handle<SceneObject>,
        transform_handle: Handle<Transformation>,
        deferred: &mut TerrainDeferredResources,
    ) -> Vec<Handle<PerDrawData>> {
        model
            .meshes
            .iter()
            .map(|mesh| {
                deferred
                    .draw_builder
                    .register_draw(&PerDrawData::terrain_draw(
                        scene_handle,
                        transform_handle,
                        mesh.material
                            .as_ref()
                            .and_then(|material| material.furikake_material_handle)
                            .unwrap_or_default(),
                        mesh.geometry.base.furikake_vertex_id.unwrap(),
                        mesh.geometry.base.vertex_count,
                        mesh.geometry.base.furikake_index_id.unwrap(),
                        mesh.geometry.base.index_count.unwrap(),
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
        let mut geometry = match geometry_store.enter_gpu_geometry(
            &geometry_entry,
            host_geometry.clone(),
        ) {
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
