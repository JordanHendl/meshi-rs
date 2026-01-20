use bento::builder::CSOBuilder;
use dashi::{
    BufferInfo, BufferUsage, CommandStream, Context, MemoryVisibility, ShaderResource, UsageBits,
};
use dashi::cmd::Executable;
use tare::utils::StagedBuffer;

const CLOUD_RESOLUTION: u32 = 64;
const CLOUD_WORKGROUP_SIZE: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CloudParams {
    time: f32,
    delta_time: f32,
    resolution: u32,
    _padding: u32,
}

pub struct CloudSimulation {
    params: StagedBuffer,
    pipeline: Option<bento::builder::CSO>,
}

impl CloudSimulation {
    pub fn new(ctx: &mut Context) -> Self {
        let params = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[SKY] Cloud Params",
                byte_size: (std::mem::size_of::<CloudParams>() as u32).max(256),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::UNIFORM,
                initial_data: None,
            },
        );

        let state_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[SKY] Cloud State",
                byte_size: CLOUD_RESOLUTION * CLOUD_RESOLUTION * 4 * 4,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            })
            .expect("create cloud state buffer");

        let pipeline = CSOBuilder::new()
            .shader(Some(
                include_str!("shaders/sky_clouds.comp.glsl").as_bytes(),
            ))
            .add_variable(
                "cloud_state",
                ShaderResource::StorageBuffer(state_buffer.into()),
            )
            .add_variable("params", ShaderResource::ConstBuffer(params.device().into()))
            .build(ctx)
            .ok();

        Self {
            params,
            pipeline,
        }
    }

    pub fn record_compute(&mut self, time: f32, delta_time: f32) -> CommandStream<Executable> {
        let params = &mut self.params.as_slice_mut::<CloudParams>()[0];
        params.time = time;
        params.delta_time = delta_time;
        params.resolution = CLOUD_RESOLUTION;

        let Some(pipeline) = self.pipeline.as_ref() else {
            return CommandStream::new().begin().end();
        };

        CommandStream::new()
            .begin()
            .combine(self.params.sync_up())
            .prepare_buffer(self.params.device().handle, UsageBits::COMPUTE_SHADER)
            .dispatch(&dashi::driver::command::Dispatch {
                x: (CLOUD_RESOLUTION + CLOUD_WORKGROUP_SIZE - 1) / CLOUD_WORKGROUP_SIZE,
                y: (CLOUD_RESOLUTION + CLOUD_WORKGROUP_SIZE - 1) / CLOUD_WORKGROUP_SIZE,
                z: 1,
                pipeline: pipeline.handle,
                bind_tables: pipeline.tables(),
                dynamic_buffers: Default::default(),
            })
            .unbind_pipeline()
            .end()
    }
}
