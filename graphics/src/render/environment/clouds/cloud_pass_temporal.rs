use super::cloud_pass_raymarch::CloudSamplingSettings;
use bento::builder::CSOBuilder;
use bytemuck::cast_slice;
use dashi::UsageBits;
use dashi::cmd::Executable;
use dashi::driver::command::Dispatch;
use dashi::{
    BufferInfo, BufferUsage, CommandStream, Context, Handle, MemoryVisibility, ShaderResource,
};
use furikake::BindlessState;
use furikake::PSOBuilderFurikakeExt;
use tare::utils::StagedBuffer;

const TEMPORAL_WORKGROUP_SIZE: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CloudTemporalParams {
    pub output_resolution: [u32; 2],
    pub prev_view_proj: [[f32; 4]; 4],
    pub camera_index: u32,
    pub _padding: [u32; 3],
    pub blend_factor: f32,
    pub clamp_strength: f32,
    pub depth_sigma: f32,
    pub _padding_1: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TemporalSettings {
    pub blend_factor: f32,
    pub clamp_strength: f32,
    pub depth_sigma: f32,
}

pub struct CloudTemporalPass {
    pub history_color: [Handle<dashi::Buffer>; 2],
    pub history_transmittance: [Handle<dashi::Buffer>; 2],
    pub history_depth: [Handle<dashi::Buffer>; 2],
    pub history_weight: [Handle<dashi::Buffer>; 2],
    current_color: Handle<dashi::Buffer>,
    current_transmittance: Handle<dashi::Buffer>,
    current_depth: Handle<dashi::Buffer>,
    params: StagedBuffer,
    pipelines: [Option<bento::builder::CSO>; 2],
    timer_index: u32,
    output_resolution: [u32; 2],
    history_index: usize,
}

impl CloudTemporalPass {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        output_resolution: [u32; 2],
        current_color: Handle<dashi::Buffer>,
        current_transmittance: Handle<dashi::Buffer>,
        current_depth: Handle<dashi::Buffer>,
        timer_index: u32,
    ) -> Self {
        let params = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[CLOUD] Temporal Params",
                byte_size: (std::mem::size_of::<CloudTemporalParams>() as u32).max(256),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::UNIFORM,
                initial_data: None,
            },
        );

        let buffers = create_history_buffers(ctx, output_resolution);

        let pipelines = [
            build_pipeline(
                ctx,
                state,
                &params,
                current_color,
                current_transmittance,
                current_depth,
                buffers.history_color[0],
                buffers.history_transmittance[0],
                buffers.history_depth[0],
                buffers.history_weight[0],
                buffers.history_color[1],
                buffers.history_transmittance[1],
                buffers.history_depth[1],
                buffers.history_weight[1],
            ),
            build_pipeline(
                ctx,
                state,
                &params,
                current_color,
                current_transmittance,
                current_depth,
                buffers.history_color[1],
                buffers.history_transmittance[1],
                buffers.history_depth[1],
                buffers.history_weight[1],
                buffers.history_color[0],
                buffers.history_transmittance[0],
                buffers.history_depth[0],
                buffers.history_weight[0],
            ),
        ];

        Self {
            history_color: buffers.history_color,
            history_transmittance: buffers.history_transmittance,
            history_depth: buffers.history_depth,
            history_weight: buffers.history_weight,
            current_color,
            current_transmittance,
            current_depth,
            params,
            pipelines,
            timer_index,
            output_resolution,
            history_index: 0,
        }
    }

    pub fn update_params(
        &mut self,
        settings: CloudSamplingSettings,
        temporal: TemporalSettings,
        prev_view_proj: [[f32; 4]; 4],
    ) {
        self.output_resolution = settings.output_resolution;
        let params = &mut self.params.as_slice_mut::<CloudTemporalParams>()[0];
        *params = CloudTemporalParams {
            output_resolution: settings.output_resolution,
            prev_view_proj,
            camera_index: settings.camera_index,
            _padding: [0; 3],
            blend_factor: temporal.blend_factor,
            clamp_strength: temporal.clamp_strength,
            depth_sigma: temporal.depth_sigma,
            _padding_1: 0.0,
        };
    }

    pub fn record(&mut self) -> CommandStream<Executable> {
        let Some(pipeline) = self.pipelines[self.history_index].as_ref() else {
            return CommandStream::new().begin().end();
        };

        let output_resolution = self.output_resolution;
        let history_read = self.history_index;
        let history_write = 1 - self.history_index;

        let cmd = CommandStream::new()
            .begin()
            .combine(self.params.sync_up())
            .prepare_buffer(self.current_color, UsageBits::COMPUTE_SHADER)
            .prepare_buffer(self.current_transmittance, UsageBits::COMPUTE_SHADER)
            .prepare_buffer(self.current_depth, UsageBits::COMPUTE_SHADER)
            .prepare_buffer(self.history_color[history_read], UsageBits::COMPUTE_SHADER)
            .prepare_buffer(
                self.history_transmittance[history_read],
                UsageBits::COMPUTE_SHADER,
            )
            .prepare_buffer(self.history_depth[history_read], UsageBits::COMPUTE_SHADER)
            .prepare_buffer(self.history_weight[history_read], UsageBits::COMPUTE_SHADER)
            .prepare_buffer(self.history_color[history_write], UsageBits::COMPUTE_SHADER)
            .prepare_buffer(
                self.history_transmittance[history_write],
                UsageBits::COMPUTE_SHADER,
            )
            .prepare_buffer(self.history_depth[history_write], UsageBits::COMPUTE_SHADER)
            .prepare_buffer(
                self.history_weight[history_write],
                UsageBits::COMPUTE_SHADER,
            )
            .gpu_timer_begin(self.timer_index)
            .dispatch(&Dispatch {
                x: (output_resolution[0] + TEMPORAL_WORKGROUP_SIZE - 1) / TEMPORAL_WORKGROUP_SIZE,
                y: (output_resolution[1] + TEMPORAL_WORKGROUP_SIZE - 1) / TEMPORAL_WORKGROUP_SIZE,
                z: 1,
                pipeline: pipeline.handle,
                bind_tables: pipeline.tables(),
                dynamic_buffers: Default::default(),
            })
            .gpu_timer_end(self.timer_index)
            .unbind_pipeline()
            .end();

        self.history_index = history_write;
        cmd
    }

    pub fn history_index(&self) -> usize {
        self.history_index
    }

    pub fn output_color(&self) -> Handle<dashi::Buffer> {
        self.history_color[self.history_index]
    }

    pub fn output_transmittance(&self) -> Handle<dashi::Buffer> {
        self.history_transmittance[self.history_index]
    }

    pub fn output_depth(&self) -> Handle<dashi::Buffer> {
        self.history_depth[self.history_index]
    }

    pub fn output_weight(&self) -> Handle<dashi::Buffer> {
        self.history_weight[self.history_index]
    }
}

struct TemporalBuffers {
    history_color: [Handle<dashi::Buffer>; 2],
    history_transmittance: [Handle<dashi::Buffer>; 2],
    history_depth: [Handle<dashi::Buffer>; 2],
    history_weight: [Handle<dashi::Buffer>; 2],
}

fn create_history_buffers(ctx: &mut Context, output_resolution: [u32; 2]) -> TemporalBuffers {
    let pixel_count = output_resolution[0] * output_resolution[1];
    let history_color_init = vec![0.0f32; (pixel_count * 4) as usize];
    let history_trans_init = vec![1.0f32; pixel_count as usize];
    let history_depth_init = vec![0.0f32; pixel_count as usize];
    let history_weight_init = vec![0.0f32; pixel_count as usize];

    let mut buffer = |name: &str, bytes_per_pixel: u32, initial_data: Option<&[u8]>| {
        ctx.make_buffer(&BufferInfo {
            debug_name: name,
            byte_size: pixel_count * bytes_per_pixel,
            visibility: MemoryVisibility::Gpu,
            usage: BufferUsage::STORAGE,
            initial_data,
        })
        .expect("create temporal buffer")
    };

    TemporalBuffers {
        history_color: [
            buffer(
                "[CLOUD] History Color 0",
                16,
                Some(cast_slice(&history_color_init)),
            ),
            buffer(
                "[CLOUD] History Color 1",
                16,
                Some(cast_slice(&history_color_init)),
            ),
        ],
        history_transmittance: [
            buffer(
                "[CLOUD] History Transmittance 0",
                4,
                Some(cast_slice(&history_trans_init)),
            ),
            buffer(
                "[CLOUD] History Transmittance 1",
                4,
                Some(cast_slice(&history_trans_init)),
            ),
        ],
        history_depth: [
            buffer(
                "[CLOUD] History Depth 0",
                4,
                Some(cast_slice(&history_depth_init)),
            ),
            buffer(
                "[CLOUD] History Depth 1",
                4,
                Some(cast_slice(&history_depth_init)),
            ),
        ],
        history_weight: [
            buffer(
                "[CLOUD] History Weight 0",
                4,
                Some(cast_slice(&history_weight_init)),
            ),
            buffer(
                "[CLOUD] History Weight 1",
                4,
                Some(cast_slice(&history_weight_init)),
            ),
        ],
    }
}

fn build_pipeline(
    ctx: &mut Context,
    state: &mut BindlessState,
    params: &StagedBuffer,
    current_color: Handle<dashi::Buffer>,
    current_transmittance: Handle<dashi::Buffer>,
    current_depth: Handle<dashi::Buffer>,
    history_color: Handle<dashi::Buffer>,
    history_transmittance: Handle<dashi::Buffer>,
    history_depth: Handle<dashi::Buffer>,
    history_weight: Handle<dashi::Buffer>,
    output_color: Handle<dashi::Buffer>,
    output_transmittance: Handle<dashi::Buffer>,
    output_depth: Handle<dashi::Buffer>,
    output_weight: Handle<dashi::Buffer>,
) -> Option<bento::builder::CSO> {
    Some(
        CSOBuilder::new()
            .set_debug_name("[MESHI] Cloud Temporal")
            .shader(Some(
                include_str!("shaders/cloud_temporal.comp.glsl").as_bytes(),
            ))
            .add_reserved_table_variable(state, "meshi_bindless_cameras")
            .unwrap()
            .add_variable(
                "params",
                ShaderResource::ConstBuffer(params.device().into()),
            )
            .add_variable(
                "cloud_current_color",
                ShaderResource::StorageBuffer(current_color.into()),
            )
            .add_variable(
                "cloud_current_trans",
                ShaderResource::StorageBuffer(current_transmittance.into()),
            )
            .add_variable(
                "cloud_current_depth",
                ShaderResource::StorageBuffer(current_depth.into()),
            )
            .add_variable(
                "cloud_history_color",
                ShaderResource::StorageBuffer(history_color.into()),
            )
            .add_variable(
                "cloud_history_trans",
                ShaderResource::StorageBuffer(history_transmittance.into()),
            )
            .add_variable(
                "cloud_history_depth",
                ShaderResource::StorageBuffer(history_depth.into()),
            )
            .add_variable(
                "cloud_history_weight",
                ShaderResource::StorageBuffer(history_weight.into()),
            )
            .add_variable(
                "cloud_output_color",
                ShaderResource::StorageBuffer(output_color.into()),
            )
            .add_variable(
                "cloud_output_trans",
                ShaderResource::StorageBuffer(output_transmittance.into()),
            )
            .add_variable(
                "cloud_output_depth",
                ShaderResource::StorageBuffer(output_depth.into()),
            )
            .add_variable(
                "cloud_output_weight",
                ShaderResource::StorageBuffer(output_weight.into()),
            )
            .build(ctx)
            .unwrap(),
    )
}
