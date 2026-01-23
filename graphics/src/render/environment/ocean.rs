use super::EnvironmentRendererInfo;
use bento::builder::{AttachmentDesc, CSOBuilder, PSO, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use dashi::cmd::{Executable, PendingGraphics};
use dashi::driver::command::{Dispatch, Draw};
use dashi::{
    Buffer, BufferInfo, BufferUsage, CommandStream, Context, DynamicAllocator, Format, Handle,
    ImageView, MemoryVisibility, Sampler, SamplerInfo, ShaderResource, UsageBits, Viewport,
};
use furikake::BindlessState;
use furikake::PSOBuilderFurikakeExt;
use furikake::types::Camera;
use glam::Vec2;
use tracing::warn;

#[derive(Clone, Copy)]
pub struct OceanInfo {
    /// World-space half-size of a single ocean patch in meters.
    pub patch_size: f32,
    /// Tessellation resolution for each patch; higher values add detail at higher cost.
    pub vertex_resolution: u32,
    /// FFT grid sizes for near, mid, and far cascades.
    pub cascade_fft_sizes: [u32; 3],
    /// Patch size multipliers for near, mid, and far cascades.
    pub cascade_patch_scales: [f32; 3],
    /// Base tile radius (1 -> 3x3 grid).
    pub base_tile_radius: u32,
    /// Maximum tile radius for far-field coverage.
    pub max_tile_radius: u32,
    /// Camera-height step (meters) before expanding tiles.
    pub tile_height_step: f32,
}

impl Default for OceanInfo {
    fn default() -> Self {
        Self {
            patch_size: 200.0,
            vertex_resolution: 128,
            cascade_fft_sizes: [256, 128, 64],
            cascade_patch_scales: [0.5, 1.0, 4.0],
            base_tile_radius: 1,
            max_tile_radius: 3,
            tile_height_step: 1.0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct OceanFrameSettings {
    pub enabled: bool,
    /// Normalized wind direction used to align the wave spectrum.
    pub wind_dir: Vec2,
    /// Wind speed in meters per second; higher values create taller, faster waves.
    pub wind_speed: f32,
    /// Scales overall wave height, slope, and velocity (1.0 = default).
    pub wave_amplitude: f32,
    /// Scales high-frequency capillary detail (1.0 = default).
    pub capillary_strength: f32,
    /// Time multiplier for wave evolution; values above 1.0 speed up the animation.
    pub time_scale: f32,
}

impl Default for OceanFrameSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            wind_dir: Vec2::new(0.9, 0.2),
            wind_speed: 2.0,
            wave_amplitude: 2.0,
            capillary_strength: 1.0,
            time_scale: 1.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct OceanComputeParams {
    fft_size: u32,
    time: f32,
    time_scale: f32,
    wave_amplitude: f32,
    wind_dir: Vec2,
    wind_speed: f32,
    capillary_strength: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct OceanDrawParams {
    cascade_fft_sizes: [u32; 4],
    cascade_patch_sizes: [f32; 4],
    cascade_blend_ranges: [f32; 4],
    vertex_resolution: u32,
    camera_index: u32,
    base_tile_radius: u32,
    max_tile_radius: u32,
    tile_height_step: f32,
    time: f32,
    wind_dir: Vec2,
    wind_speed: f32,
    wave_amplitude: f32,
    _padding1: f32,
}

#[derive(Debug)]
struct OceanCascade {
    fft_size: u32,
    patch_size: f32,
    wave_buffer: Handle<Buffer>,
    compute_pipeline: Option<bento::builder::CSO>,
}

pub struct OceanRenderer {
    pipeline: PSO,
    cascades: [OceanCascade; 3],
    vertex_resolution: u32,
    base_tile_radius: u32,
    max_tile_radius: u32,
    tile_height_step: f32,
    wind_dir: Vec2,
    wind_speed: f32,
    wave_amplitude: f32,
    capillary_strength: f32,
    time_scale: f32,
    use_depth: bool,
    environment_sampler: Handle<Sampler>,
    enabled: bool,
}

fn compile_ocean_shaders() -> [bento::CompilationResult; 2] {
    let compiler = Compiler::new().expect("Failed to create shader compiler");
    let base_request = Request {
        name: Some("environment_ocean".to_string()),
        lang: ShaderLang::Glsl,
        optimization: OptimizationLevel::Performance,
        debug_symbols: true,
        ..Default::default()
    };

    let vertex = compiler
        .compile(
            include_str!("shaders/environment_ocean.vert.glsl").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Vertex,
                ..base_request.clone()
            },
        )
        .expect("Failed to compile ocean vertex shader");

    let fragment = compiler
        .compile(
            include_str!("shaders/environment_ocean.frag.glsl").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Fragment,
                ..base_request
            },
        )
        .expect("Failed to compile ocean fragment shader");

    [vertex, fragment]
}

impl OceanRenderer {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        info: &EnvironmentRendererInfo,
        dynamic: &DynamicAllocator,
        environment_map: ImageView,
    ) -> Self {
        let ocean_info = info.ocean;
        let mut cascades = Vec::with_capacity(3);
        for (index, fft_size) in ocean_info.cascade_fft_sizes.iter().enumerate() {
            let patch_scale = ocean_info
                .cascade_patch_scales
                .get(index)
                .copied()
                .unwrap_or(1.0);
            let patch_size = ocean_info.patch_size * patch_scale;
            let wave_buffer = ctx
                .make_buffer(&BufferInfo {
                    debug_name: "[MESHI GFX OCEAN] Wave Buffer",
                    byte_size: fft_size * fft_size * 16,
                    visibility: MemoryVisibility::Gpu,
                    usage: BufferUsage::STORAGE,
                    initial_data: None,
                })
                .expect("Failed to create ocean wave buffer");

            let compute_pipeline = match CSOBuilder::new()
                .shader(Some(
                    include_str!("shaders/environment_ocean.comp.glsl").as_bytes(),
                ))
                .set_debug_name("[MESHI] Ocean Compute")
                .add_reserved_table_variables(state)
                .unwrap()
                .add_variable(
                    "ocean_waves",
                    ShaderResource::StorageBuffer(wave_buffer.into()),
                )
                .add_variable("params", ShaderResource::DynamicStorage(dynamic.state()))
                .build(ctx)
            {
                Ok(pipeline) => Some(pipeline),
                Err(err) => {
                    warn!(
                        "Ocean compute pipeline creation failed: {err}. Falling back to static waves."
                    );
                    None
                }
            };

            cascades.push(OceanCascade {
                fft_size: *fft_size,
                patch_size,
                wave_buffer,
                compute_pipeline,
            });
        }

        let cascades: [OceanCascade; 3] = cascades
            .try_into()
            .expect("Expected three ocean cascades");

        let shaders = compile_ocean_shaders();
        let environment_sampler = ctx
            .make_sampler(&SamplerInfo::default())
            .expect("Failed to create ocean environment sampler");
        let wave_resources = cascades
            .iter()
            .enumerate()
            .map(|(slot, cascade)| dashi::IndexedResource {
                resource: ShaderResource::StorageBuffer(cascade.wave_buffer.into()),
                slot: slot as u32,
            })
            .collect();
        let mut pso_builder = PSOBuilder::new()
            .set_debug_name("[MESHI] Ocean")
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .set_attachment_format(0, info.color_format)
            .add_reserved_table_variables(state).unwrap()
            .add_table_variable_with_resources(
                "ocean_waves",
                wave_resources,
            )
            .add_table_variable_with_resources(
                "params",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::DynamicStorage(dynamic.state()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "ocean_env_map",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::Image(environment_map),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "ocean_env_sampler",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::Sampler(environment_sampler),
                    slot: 0,
                }],
            );

        pso_builder = pso_builder
            .add_reserved_table_variables(state).unwrap()
            .add_reserved_table_variable(state, "meshi_bindless_cameras")
            .unwrap()
            .add_reserved_table_variable(state, "meshi_bindless_lights")
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
            .expect("Failed to build ocean PSO");

        state.register_pso_tables(&pipeline);
        let default_frame = OceanFrameSettings::default();
        let base_tile_radius = ocean_info.base_tile_radius.max(1);
        let max_tile_radius = ocean_info.max_tile_radius.max(base_tile_radius);
        Self {
            pipeline,
            cascades,
            vertex_resolution: ocean_info.vertex_resolution,
            base_tile_radius,
            max_tile_radius,
            tile_height_step: ocean_info.tile_height_step.max(1.0),
            wind_dir: default_frame.wind_dir,
            wind_speed: default_frame.wind_speed,
            wave_amplitude: default_frame.wave_amplitude,
            capillary_strength: default_frame.capillary_strength,
            time_scale: default_frame.time_scale,
            use_depth: info.use_depth,
            environment_sampler,
            enabled: default_frame.enabled,
        }
    }

    pub fn update(&mut self, settings: OceanFrameSettings) {
        self.enabled = settings.enabled;
        self.wind_dir = settings.wind_dir;
        self.wind_speed = settings.wind_speed;
        self.wave_amplitude = settings.wave_amplitude;
        self.capillary_strength = settings.capillary_strength;
        self.time_scale = settings.time_scale;
    }

    pub fn set_environment_map(&mut self, view: ImageView) {
        self.pipeline.update_table(
            "ocean_env_map",
            dashi::IndexedResource {
                resource: ShaderResource::Image(view),
                slot: 0,
            },
        );
        self.pipeline.update_table(
            "ocean_env_sampler",
            dashi::IndexedResource {
                resource: ShaderResource::Sampler(self.environment_sampler),
                slot: 0,
            },
        );
    }

    pub fn record_compute(
        &mut self,
        dynamic: &mut DynamicAllocator,
        time: f32,
    ) -> CommandStream<Executable> {
        if !self.enabled {
            return CommandStream::new().begin().end();
        }

        let mut stream = CommandStream::new().begin();
        for cascade in &self.cascades {
            let Some(pipeline) = cascade.compute_pipeline.as_ref() else {
                continue;
            };
            let mut alloc = dynamic
                .bump()
                .expect("Failed to allocate ocean compute params");
            let params = &mut alloc.slice::<OceanComputeParams>()[0];
            *params = OceanComputeParams {
                fft_size: cascade.fft_size,
                time,
                time_scale: self.time_scale,
                wave_amplitude: self.wave_amplitude,
                wind_dir: self.wind_dir,
                wind_speed: self.wind_speed,
                capillary_strength: self.capillary_strength,
            };

            stream = stream
                .prepare_buffer(cascade.wave_buffer, UsageBits::COMPUTE_SHADER)
                .dispatch(&Dispatch {
                    x: (cascade.fft_size + 7) / 8,
                    y: (cascade.fft_size + 7) / 8,
                    z: 1,
                    pipeline: pipeline.handle,
                    bind_tables: pipeline.tables(),
                    dynamic_buffers: [None, Some(alloc), None, None],
                })
                .unbind_pipeline();
        }

        stream.end()
    }

    pub fn record_draws(
        &mut self,
        viewport: &Viewport,
        dynamic: &mut DynamicAllocator,
        camera: Handle<Camera>,
        time: f32,
    ) -> CommandStream<PendingGraphics> {
        if !self.enabled {
            return CommandStream::subdraw();
        }

        let mut alloc = dynamic
            .bump()
            .expect("Failed to allocate ocean draw params");

        let params = &mut alloc.slice::<OceanDrawParams>()[0];
        let mut cascade_fft_sizes = [0u32; 4];
        let mut cascade_patch_sizes = [0.0f32; 4];
        for (index, cascade) in self.cascades.iter().enumerate() {
            cascade_fft_sizes[index] = cascade.fft_size;
            cascade_patch_sizes[index] = cascade.patch_size;
        }
        let blend_ranges = [
            cascade_patch_sizes[0] * 6.0,
            cascade_patch_sizes[1] * 10.0,
            cascade_patch_sizes[2] * 12.0,
            0.0,
        ];
        *params = OceanDrawParams {
            cascade_fft_sizes,
            cascade_patch_sizes,
            cascade_blend_ranges: blend_ranges,
            vertex_resolution: self.vertex_resolution,
            camera_index: camera.slot as u32,
            base_tile_radius: self.base_tile_radius,
            max_tile_radius: self.max_tile_radius,
            tile_height_step: self.tile_height_step,
            time,
            wind_dir: self.wind_dir,
            wind_speed: self.wind_speed,
            wave_amplitude: self.wave_amplitude,
            _padding1: 0.0,
        };

        let grid_resolution = self.vertex_resolution.max(2);
        let quad_count = (grid_resolution - 1) * (grid_resolution - 1);
        let vertex_count = quad_count * 6;
        let max_tile_count = self.max_tile_radius.max(1) * 2 + 1;
        let instance_count = max_tile_count * max_tile_count;

        CommandStream::<PendingGraphics>::subdraw()
            .bind_graphics_pipeline(self.pipeline.handle)
            .update_viewport(viewport)
            .draw(&Draw {
                bind_tables: self.pipeline.tables(),
                dynamic_buffers: [None, Some(alloc), None, None],
                instance_count,
                count: vertex_count,
                ..Default::default()
            })
            .unbind_graphics_pipeline()
    }
}
