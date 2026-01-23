use super::EnvironmentRendererInfo;
use bento::builder::{AttachmentDesc, CSOBuilder, PSOBuilder, PSO};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use dashi::cmd::{Executable, PendingGraphics};
use dashi::driver::command::{Dispatch, Draw};
use dashi::{
    Buffer, BufferInfo, BufferUsage, CommandStream, Context, DynamicAllocator, Format, Handle,
    MemoryVisibility, ShaderResource, UsageBits, Viewport,
};
use furikake::BindlessState;
use furikake::PSOBuilderFurikakeExt;
use glam::{Mat4, Vec3};
use noren::rdb::terrain::TerrainChunkArtifact;

#[derive(Clone, Copy)]
pub struct TerrainInfo {
    pub patch_size: f32,
    pub lod_levels: u32,
    pub clipmap_resolution: u32,
    pub max_tiles: u32,
}

impl Default for TerrainInfo {
    fn default() -> Self {
        Self {
            patch_size: 64.0,
            lod_levels: 4,
            clipmap_resolution: 8,
            max_tiles: 16,
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
    height_scale: f32,
    _padding: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TerrainDrawParams {
    camera_position: Vec3,
    lod_levels: u32,
    patch_size: f32,
    max_tiles: u32,
    height_scale: f32,
    _padding: f32,
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
    max_tiles: u32,
    camera_position: Vec3,
    use_depth: bool,
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
            max_tiles: terrain_info.max_tiles,
            camera_position: Vec3::ZERO,
            use_depth: info.use_depth,
        }
    }

    pub fn update(&mut self, settings: TerrainFrameSettings) {
        self.camera_position = settings.camera_position;
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
                height_scale: 5.0,
                _padding: 0.0,
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
            height_scale: 5.0,
            _padding: 0.0,
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
}
