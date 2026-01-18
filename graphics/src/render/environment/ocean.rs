use super::EnvironmentRendererInfo;
use bento::builder::{AttachmentDesc, CSOBuilder, PSO, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use dashi::cmd::{Executable, PendingGraphics};
use dashi::driver::command::{Dispatch, Draw};
use dashi::execution::CommandRing;
use dashi::{
    Buffer, BufferInfo, BufferUsage, CommandQueueInfo2, CommandStream, Context, DynamicAllocator,
    Format, Handle, MemoryVisibility, ShaderResource, UsageBits, Viewport,
};
use furikake::BindlessState;
use furikake::PSOBuilderFurikakeExt;
use furikake::types::Camera;
use glam::Vec2;

#[derive(Clone, Copy)]
pub struct OceanInfo {
    pub fft_size: u32,
    pub patch_size: f32,
    pub vertex_resolution: u32,
}

impl Default for OceanInfo {
    fn default() -> Self {
        Self {
            fft_size: 64,
            patch_size: 100.0,
            vertex_resolution: 128,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct OceanFrameSettings {
    pub wind_dir: Vec2,
    pub wind_speed: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct OceanComputeParams {
    fft_size: u32,
    time: f32,
    wind_dir: Vec2,
    wind_speed: f32,
    _padding: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct OceanDrawParams {
    fft_size: u32,
    vertex_resolution: u32,
    patch_size: f32,
    time: f32,
    wind_dir: Vec2,
    wind_speed: f32,
    camera_index: u32,
    _padding: f32,
}

pub struct OceanRenderer {
    pipeline: PSO,
    compute_pipeline: Option<bento::builder::CSO>,
    wave_buffer: Handle<Buffer>,
    fft_size: u32,
    patch_size: f32,
    vertex_resolution: u32,
    wind_dir: Vec2,
    wind_speed: f32,
    queue: CommandRing,
    use_depth: bool,
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
    ) -> Self {
        let ocean_info = info.ocean;
        let wave_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[MESHI GFX OCEAN] Wave Buffer",
                byte_size: ocean_info.fft_size * ocean_info.fft_size * 16,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            })
            .expect("Failed to create ocean wave buffer");

        let compute_pipeline = CSOBuilder::new()
            .shader(Some(
                include_str!("shaders/environment_ocean.comp.glsl").as_bytes(),
            ))
            .add_variable(
                "ocean_waves",
                ShaderResource::StorageBuffer(wave_buffer.into()),
            )
            .add_variable(
                "ocean_params",
                ShaderResource::DynamicStorage(dynamic.state()),
            )
            .build(ctx)
            .ok();

        let shaders = compile_ocean_shaders();
        let mut pso_builder = PSOBuilder::new()
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .set_attachment_format(0, info.color_format)
            .add_table_variable_with_resources(
                "ocean_waves",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::StorageBuffer(wave_buffer.into()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "params",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::DynamicStorage(dynamic.state()),
                    slot: 0,
                }],
            );

        pso_builder = pso_builder
            .add_reserved_table_variable(state, "meshi_bindless_cameras")
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

        let queue = ctx
            .make_command_ring(&CommandQueueInfo2 {
                debug_name: "[OCEAN SIM]",
                parent: None,
                queue_type: dashi::QueueType::Compute,
            })
            .expect("create ocean compute ring");

        Self {
            pipeline,
            compute_pipeline,
            wave_buffer,
            fft_size: ocean_info.fft_size,
            patch_size: ocean_info.patch_size,
            vertex_resolution: ocean_info.vertex_resolution,
            wind_dir: Vec2::new(1.0, 0.0),
            wind_speed: 12.0,
            queue,
            use_depth: info.use_depth,
        }
    }

    pub fn update(&mut self, settings: OceanFrameSettings) {
        self.wind_dir = settings.wind_dir;
        self.wind_speed = settings.wind_speed;
    }

    pub fn record_draws(
        &mut self,
        viewport: &Viewport,
        dynamic: &mut DynamicAllocator,
        camera: Handle<Camera>,
        time: f32,
    ) -> CommandStream<PendingGraphics> {
        if let Some(pipeline) = self.compute_pipeline.as_ref() {
            let mut alloc = dynamic
                .bump()
                .expect("Failed to allocate ocean compute params");
            let params = &mut alloc.slice::<OceanComputeParams>()[0];
            *params = OceanComputeParams {
                fft_size: self.fft_size,
                time,
                wind_dir: self.wind_dir,
                wind_speed: self.wind_speed,
                _padding: 0.0,
            };

            self.queue
                .record(|c| {
                    CommandStream::new()
                        .begin()
                        .prepare_buffer(self.wave_buffer, UsageBits::COMPUTE_SHADER)
                        .dispatch(&Dispatch {
                            x: (self.fft_size + 7) / 8,
                            y: (self.fft_size + 7) / 8,
                            z: 1,
                            pipeline: pipeline.handle,
                            bind_tables: pipeline.tables(),
                            dynamic_buffers: [None, Some(alloc), None, None],
                        })
                        .unbind_pipeline()
                        .end()
                        .append(c)
                        .expect("Failed to record ocean compute");
                })
                .expect("record ocean compute");

            self.queue
                .submit(&Default::default())
                .expect("submit ocean compute");
            self.queue.wait_all().expect("wait ocean compute");
        }

        let mut alloc = dynamic
            .bump()
            .expect("Failed to allocate ocean draw params");

        let params = &mut alloc.slice::<OceanDrawParams>()[0];
        *params = OceanDrawParams {
            fft_size: self.fft_size,
            vertex_resolution: self.vertex_resolution,
            patch_size: self.patch_size,
            time,
            wind_dir: self.wind_dir,
            wind_speed: self.wind_speed,
            camera_index: camera.slot as u32,
            _padding: 0.0,
        };

        let grid_resolution = self.vertex_resolution.max(2);
        let quad_count = (grid_resolution - 1) * (grid_resolution - 1);
        let vertex_count = quad_count * 6;

        CommandStream::<PendingGraphics>::subdraw()
            .bind_graphics_pipeline(self.pipeline.handle)
            .update_viewport(viewport)
            .draw(&Draw {
                bind_tables: self.pipeline.tables(),
                dynamic_buffers: [None, Some(alloc), None, None],
                instance_count: 1,
                count: vertex_count,
                ..Default::default()
            })
            .unbind_graphics_pipeline()
    }
}
