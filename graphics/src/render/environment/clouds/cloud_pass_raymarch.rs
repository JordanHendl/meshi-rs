use bento::builder::CSOBuilder;
use bytemuck::cast_slice;
use dashi::UsageBits;
use dashi::cmd::Executable;
use dashi::driver::command::Dispatch;
use dashi::{
    BufferInfo, BufferUsage, CommandStream, Context, Handle, ImageView, MemoryVisibility, Sampler,
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
    pub output_info: [u32; 4],
    pub base_noise_size: [u32; 4],
    pub detail_noise_size: [u32; 4],
    pub shadow_info: [u32; 4],
    pub shadow_cascade_splits: [f32; 4],
    pub shadow_cascade_extents: [f32; 4],
    pub shadow_cascade_resolutions: [u32; 4],
    pub shadow_cascade_offsets: [u32; 4],
    pub shadow_cascade_strengths: [f32; 4],
    pub weather_channels_a: [u32; 4],
    pub weather_channels_b: [u32; 4],
    pub layer_a: [f32; 4],
    pub wind_a: [f32; 4],
    pub layer_b: [f32; 4],
    pub wind_b: [f32; 4],
    pub step_info: [u32; 4],
    pub scatter_params: [f32; 4],
    pub sun_radiance: [f32; 4],
    pub time_params: [f32; 4],
    pub jitter_params: [f32; 4],
    pub sun_direction: [f32; 4],
    pub atmosphere_params: [f32; 4],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CloudSamplingSettings {
    pub output_resolution: [u32; 2],
    pub shadow_resolution: u32,
    pub base_noise_dims: UVec3,
    pub detail_noise_dims: UVec3,
    pub weather_map_size: u32,
    pub layer_a: CloudLayerSampling,
    pub layer_b: CloudLayerSampling,
    pub step_count: u32,
    pub light_step_count: u32,
    pub phase_g: f32,
    pub multi_scatter_strength: f32,
    pub multi_scatter_respects_shadow: bool,
    pub sun_radiance: Vec3,
    pub sun_direction: Vec3,
    pub coverage_power: f32,
    pub detail_strength: f32,
    pub curl_strength: f32,
    pub jitter_strength: f32,
    pub shadow_strength: f32,
    pub shadow_extent: f32,
    pub shadow_cascade_count: u32,
    pub shadow_split_lambda: f32,
    pub shadow_cascade_splits: [f32; 4],
    pub shadow_cascade_extents: [f32; 4],
    pub shadow_cascade_resolutions: [u32; 4],
    pub shadow_cascade_offsets: [u32; 4],
    pub shadow_cascade_strengths: [f32; 4],
    pub epsilon: f32,
    pub frame_index: u32,
    pub time: f32,
    pub use_shadow_map: bool,
    pub camera_index: u32,
    pub debug_view: u32,
    pub atmosphere_view_strength: f32,
    pub atmosphere_view_extinction: f32,
    pub atmosphere_light_transmittance: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CloudLayerSampling {
    pub cloud_base: f32,
    pub cloud_top: f32,
    pub density_scale: f32,
    pub noise_scale: f32,
    pub wind: Vec2,
    pub weather_channels: [u32; 3],
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
        environment_map: ImageView,
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
                .add_variable(
                    "cloud_environment_map",
                    ShaderResource::Image(environment_map),
                )
                .add_variable(
                    "cloud_environment_sampler",
                    ShaderResource::Sampler(sampler),
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
            output_info: [
                settings.output_resolution[0],
                settings.output_resolution[1],
                settings.weather_map_size,
                settings.frame_index,
            ],
            base_noise_size: [
                settings.base_noise_dims.x,
                settings.base_noise_dims.y,
                settings.base_noise_dims.z,
                0,
            ],
            detail_noise_size: [
                settings.detail_noise_dims.x,
                settings.detail_noise_dims.y,
                settings.detail_noise_dims.z,
                0,
            ],
            shadow_info: [
                settings.shadow_resolution,
                settings.shadow_cascade_count,
                settings.camera_index,
                settings.debug_view,
            ],
            shadow_cascade_splits: settings.shadow_cascade_splits,
            shadow_cascade_extents: settings.shadow_cascade_extents,
            shadow_cascade_resolutions: settings.shadow_cascade_resolutions,
            shadow_cascade_offsets: settings.shadow_cascade_offsets,
            shadow_cascade_strengths: settings.shadow_cascade_strengths,
            weather_channels_a: [
                settings.layer_a.weather_channels[0],
                settings.layer_a.weather_channels[1],
                settings.layer_a.weather_channels[2],
                0,
            ],
            weather_channels_b: [
                settings.layer_b.weather_channels[0],
                settings.layer_b.weather_channels[1],
                settings.layer_b.weather_channels[2],
                0,
            ],
            layer_a: [
                settings.layer_a.cloud_base,
                settings.layer_a.cloud_top,
                settings.layer_a.density_scale,
                settings.layer_a.noise_scale,
            ],
            wind_a: [settings.layer_a.wind.x, settings.layer_a.wind.y, 0.0, 0.0],
            layer_b: [
                settings.layer_b.cloud_base,
                settings.layer_b.cloud_top,
                settings.layer_b.density_scale,
                settings.layer_b.noise_scale,
            ],
            wind_b: [settings.layer_b.wind.x, settings.layer_b.wind.y, 0.0, 0.0],
            step_info: [
                settings.step_count,
                settings.light_step_count,
                settings.multi_scatter_respects_shadow as u32,
                settings.use_shadow_map as u32,
            ],
            scatter_params: [
                settings.phase_g,
                settings.multi_scatter_strength,
                settings.shadow_strength,
                settings.epsilon,
            ],
            sun_radiance: [
                settings.sun_radiance.x,
                settings.sun_radiance.y,
                settings.sun_radiance.z,
                0.0,
            ],
            time_params: [
                settings.time,
                settings.coverage_power,
                settings.detail_strength,
                settings.curl_strength,
            ],
            jitter_params: [
                settings.jitter_strength,
                settings.shadow_extent,
                settings.atmosphere_view_strength,
                settings.atmosphere_view_extinction,
            ],
            sun_direction: [
                settings.sun_direction.x,
                settings.sun_direction.y,
                settings.sun_direction.z,
                0.0,
            ],
            atmosphere_params: [settings.atmosphere_light_transmittance, 0.0, 0.0, 0.0],
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
