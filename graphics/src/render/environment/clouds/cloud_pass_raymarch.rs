use bento::builder::CSOBuilder;
use bytemuck::cast_slice;
use dashi::UsageBits;
use dashi::cmd::Executable;
use dashi::driver::command::Dispatch;
use dashi::{
    BufferInfo, BufferUsage, CommandStream, Context, Handle, MemoryVisibility, Sampler,
    SamplerInfo, ShaderResource,
};
use furikake::BindlessState;
use furikake::PSOBuilderFurikakeExt;
use glam::{UVec3, Vec2, Vec3};
use tare::utils::StagedBuffer;

use super::cloud_assets::CloudAssets;
use super::cloud_pass_shadow::CloudShadowPass;

const RAYMARCH_WORKGROUP_SIZE: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CloudRaymarchParams {
    pub output_resolution: [u32; 2],
    pub base_noise_size: [u32; 3],
    pub detail_noise_size: [u32; 3],
    pub weather_map_size: u32,
    pub frame_index: u32,
    pub shadow_resolution: u32,
    pub camera_index: u32,
    pub _padding: [u32; 3],

    pub cloud_base: f32,
    pub cloud_top: f32,
    pub density_scale: f32,
    pub step_count: u32,

    pub light_step_count: u32,
    pub phase_g: f32,
    pub sun_radiance: [f32; 3],
    pub shadow_strength: f32,

    pub wind: [f32; 2],
    pub time: f32,
    pub coverage_power: f32,

    pub detail_strength: f32,
    pub curl_strength: f32,
    pub jitter_strength: f32,
    pub epsilon: f32,

    pub sun_direction: [f32; 3],
    pub use_shadow_map: u32,
    pub shadow_extent: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CloudSamplingSettings {
    pub output_resolution: [u32; 2],
    pub shadow_resolution: u32,
    pub base_noise_dims: UVec3,
    pub detail_noise_dims: UVec3,
    pub weather_map_size: u32,
    pub cloud_base: f32,
    pub cloud_top: f32,
    pub density_scale: f32,
    pub step_count: u32,
    pub light_step_count: u32,
    pub phase_g: f32,
    pub sun_radiance: Vec3,
    pub sun_direction: Vec3,
    pub wind: Vec2,
    pub coverage_power: f32,
    pub detail_strength: f32,
    pub curl_strength: f32,
    pub jitter_strength: f32,
    pub shadow_strength: f32,
    pub shadow_extent: f32,
    pub epsilon: f32,
    pub frame_index: u32,
    pub time: f32,
    pub use_shadow_map: bool,
    pub camera_index: u32,
}

pub struct CloudRaymarchPass {
    pub color_buffer: Handle<dashi::Buffer>,
    pub transmittance_buffer: Handle<dashi::Buffer>,
    pub depth_buffer: Handle<dashi::Buffer>,
    pub steps_buffer: Handle<dashi::Buffer>,
    params: StagedBuffer,
    pipeline: Option<bento::builder::CSO>,
    sampler: Handle<Sampler>,
    timer_index: u32,
    output_resolution: [u32; 2],
}

impl CloudRaymarchPass {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        assets: &CloudAssets,
        shadow_pass: &CloudShadowPass,
        output_resolution: [u32; 2],
        timer_index: u32,
    ) -> Self {
        let params = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[CLOUD] Raymarch Params",
                byte_size: (std::mem::size_of::<CloudRaymarchParams>() as u32).max(256),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::UNIFORM,
                initial_data: None,
            },
        );

        let pixel_count = output_resolution[0] * output_resolution[1];
        let color_init = vec![0.0f32; (pixel_count * 4) as usize];
        let transmittance_init = vec![1.0f32; pixel_count as usize];
        let depth_init = vec![0.0f32; pixel_count as usize];
        let steps_init = vec![0.0f32; pixel_count as usize];

        let color_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[CLOUD] Raymarch Color",
                byte_size: pixel_count * 16,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: Some(cast_slice(&color_init)),
            })
            .expect("create cloud color buffer");
        let transmittance_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[CLOUD] Raymarch Transmittance",
                byte_size: pixel_count * 4,
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: Some(cast_slice(&transmittance_init)),
            })
            .expect("create cloud transmittance buffer");
        let depth_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[CLOUD] Raymarch Depth",
                byte_size: pixel_count * 4,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: Some(cast_slice(&depth_init)),
            })
            .expect("create cloud depth buffer");
        let steps_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[CLOUD] Raymarch Steps",
                byte_size: pixel_count * 4,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: Some(cast_slice(&steps_init)),
            })
            .expect("create cloud steps buffer");

        let sampler = ctx
            .make_sampler(&SamplerInfo::default())
            .expect("create cloud sampler");

        let pipeline = Some(
            CSOBuilder::new()
                .set_debug_name("[MESHI] Cloud Raymarch")
                .shader(Some(
                    include_str!("shaders/cloud_raymarch.comp.glsl").as_bytes(),
                ))
                .add_reserved_table_variable(state, "meshi_bindless_cameras")
                .unwrap()
                .add_reserved_table_variable(state, "meshi_bindless_lights")
                .unwrap()
                .add_variable(
                    "params",
                    ShaderResource::ConstBuffer(params.device().into()),
                )
                .add_variable(
                    "cloud_weather_map",
                    ShaderResource::Image(assets.weather_map_view()),
                )
                .add_variable("cloud_weather_sampler", ShaderResource::Sampler(sampler))
                .add_variable("cloud_base_noise", ShaderResource::Image(assets.base_noise))
                .add_variable("cloud_base_sampler", ShaderResource::Sampler(sampler))
                .add_variable(
                    "cloud_detail_noise",
                    ShaderResource::Image(assets.detail_noise),
                )
                .add_variable("cloud_detail_sampler", ShaderResource::Sampler(sampler))
                .add_variable("cloud_blue_noise", ShaderResource::Image(assets.blue_noise))
                .add_variable("cloud_blue_sampler", ShaderResource::Sampler(sampler))
                .add_variable(
                    "cloud_shadow_buffer",
                    ShaderResource::StorageBuffer(shadow_pass.shadow_buffer.into()),
                )
                .add_variable(
                    "cloud_color_buffer",
                    ShaderResource::StorageBuffer(color_buffer.into()),
                )
                .add_variable(
                    "cloud_transmittance_buffer",
                    ShaderResource::StorageBuffer(transmittance_buffer.into()),
                )
                .add_variable(
                    "cloud_depth_buffer",
                    ShaderResource::StorageBuffer(depth_buffer.into()),
                )
                .add_variable(
                    "cloud_steps_buffer",
                    ShaderResource::StorageBuffer(steps_buffer.into()),
                )
                .build(ctx)
                .unwrap(),
        );

        Self {
            color_buffer,
            transmittance_buffer,
            depth_buffer,
            steps_buffer,
            params,
            pipeline,
            sampler,
            timer_index,
            output_resolution,
        }
    }

    pub fn update_settings(&mut self, settings: CloudSamplingSettings) {
        self.output_resolution = settings.output_resolution;
        let params = &mut self.params.as_slice_mut::<CloudRaymarchParams>()[0];
        *params = CloudRaymarchParams {
            output_resolution: settings.output_resolution,
            base_noise_size: [
                settings.base_noise_dims.x,
                settings.base_noise_dims.y,
                settings.base_noise_dims.z,
            ],
            detail_noise_size: [
                settings.detail_noise_dims.x,
                settings.detail_noise_dims.y,
                settings.detail_noise_dims.z,
            ],
            weather_map_size: settings.weather_map_size,
            frame_index: settings.frame_index,
            shadow_resolution: settings.shadow_resolution,
            camera_index: settings.camera_index,
            cloud_base: settings.cloud_base,
            cloud_top: settings.cloud_top,
            density_scale: settings.density_scale,
            step_count: settings.step_count,
            light_step_count: settings.light_step_count,
            phase_g: settings.phase_g,
            sun_radiance: [
                settings.sun_radiance.x,
                settings.sun_radiance.y,
                settings.sun_radiance.z,
            ],
            shadow_strength: settings.shadow_strength,
            wind: [settings.wind.x, settings.wind.y],
            time: settings.time,
            coverage_power: settings.coverage_power,
            detail_strength: settings.detail_strength,
            curl_strength: settings.curl_strength,
            jitter_strength: settings.jitter_strength,
            epsilon: settings.epsilon,
            sun_direction: [
                settings.sun_direction.x,
                settings.sun_direction.y,
                settings.sun_direction.z,
            ],
            use_shadow_map: settings.use_shadow_map as u32,
            shadow_extent: settings.shadow_extent,
            ..Default::default()
        };
    }

    pub fn record(&mut self) -> CommandStream<Executable> {
        let Some(pipeline) = self.pipeline.as_ref() else {
            return CommandStream::new().begin().end();
        };
        let output_resolution = self.output_resolution;
        CommandStream::new()
            .begin()
            .combine(self.params.sync_up())
            .prepare_buffer(self.color_buffer, UsageBits::COMPUTE_SHADER)
            .prepare_buffer(self.transmittance_buffer, UsageBits::COMPUTE_SHADER)
            .prepare_buffer(self.depth_buffer, UsageBits::COMPUTE_SHADER)
            .prepare_buffer(self.steps_buffer, UsageBits::COMPUTE_SHADER)
            .gpu_timer_begin(self.timer_index)
            .dispatch(&Dispatch {
                x: (output_resolution[0] + RAYMARCH_WORKGROUP_SIZE - 1) / RAYMARCH_WORKGROUP_SIZE,
                y: (output_resolution[1] + RAYMARCH_WORKGROUP_SIZE - 1) / RAYMARCH_WORKGROUP_SIZE,
                z: 1,
                pipeline: pipeline.handle,
                bind_tables: pipeline.tables(),
                dynamic_buffers: Default::default(),
            })
            .gpu_timer_end(self.timer_index)
            .unbind_pipeline()
            .end()
    }

    pub fn sampler(&self) -> Handle<Sampler> {
        self.sampler
    }
}
