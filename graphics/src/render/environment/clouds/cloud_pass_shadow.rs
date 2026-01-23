use bento::builder::CSOBuilder;
use dashi::UsageBits;
use dashi::cmd::Executable;
use dashi::driver::command::Dispatch;
use dashi::{
    BufferInfo, BufferUsage, CommandStream, Context, Handle, MemoryVisibility, Sampler,
    SamplerInfo, ShaderResource,
};
use furikake::BindlessState;
use furikake::PSOBuilderFurikakeExt;
use glam::Vec3;
use tare::utils::StagedBuffer;

use super::cloud_assets::CloudAssets;
use super::cloud_pass_raymarch::CloudSamplingSettings;

const SHADOW_WORKGROUP_SIZE: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CloudShadowParams {
    pub shadow_resolution: u32,
    pub base_noise_size: [u32; 3],
    pub detail_noise_size: [u32; 3],
    pub weather_map_size: u32,
    pub camera_index: u32,
    pub cloud_base: f32,
    pub cloud_top: f32,
    pub density_scale: f32,
    pub _padding_1: f32,
    pub wind: [f32; 2],
    pub time: f32,
    pub coverage_power: f32,
    pub sun_direction: [f32; 3],
    pub shadow_strength: f32,
    pub shadow_extent: f32,
}

pub struct CloudShadowPass {
    pub shadow_buffer: Handle<dashi::Buffer>,
    pub shadow_resolution: u32,
    params: StagedBuffer,
    pipeline: Option<bento::builder::CSO>,
    sampler: Handle<Sampler>,
    timer_index: u32,
}

impl CloudShadowPass {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        assets: &CloudAssets,
        shadow_resolution: u32,
        timer_index: u32,
    ) -> Self {
        let params = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[CLOUD] Shadow Params",
                byte_size: (std::mem::size_of::<CloudShadowParams>() as u32).max(256),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::UNIFORM,
                initial_data: None,
            },
        );

        let shadow_buffer = ctx
            .make_buffer(&BufferInfo {
                debug_name: "[CLOUD] Shadow Buffer",
                byte_size: shadow_resolution * shadow_resolution * 4,
                visibility: MemoryVisibility::Gpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            })
            .expect("create shadow buffer");

        let sampler = ctx
            .make_sampler(&SamplerInfo::default())
            .expect("create cloud sampler");

        let pipeline = Some(
            CSOBuilder::new()
                .shader(Some(
                    include_str!("shaders/cloud_shadow.comp.glsl").as_bytes(),
                ))
                .add_reserved_table_variable(state, "meshi_bindless_cameras")
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
                .add_variable(
                    "cloud_shadow_buffer",
                    ShaderResource::StorageBuffer(shadow_buffer.into()),
                )
                .build(ctx)
                .unwrap(),
        );

        Self {
            shadow_buffer,
            shadow_resolution,
            params,
            pipeline,
            sampler,
            timer_index,
        }
    }

    pub fn update_settings(
        &mut self,
        settings: CloudSamplingSettings,
        sun_direction: Vec3,
        time: f32,
    ) {
        let params = &mut self.params.as_slice_mut::<CloudShadowParams>()[0];
        params.shadow_resolution = self.shadow_resolution;
        params.base_noise_size = [
            settings.base_noise_dims.x,
            settings.base_noise_dims.y,
            settings.base_noise_dims.z,
        ];
        params.detail_noise_size = [
            settings.detail_noise_dims.x,
            settings.detail_noise_dims.y,
            settings.detail_noise_dims.z,
        ];
        params.weather_map_size = settings.weather_map_size;
        params.camera_index = settings.camera_index;
        params.cloud_base = settings.cloud_base;
        params.cloud_top = settings.cloud_top;
        params.density_scale = settings.density_scale;
        params.wind = [settings.wind.x, settings.wind.y];
        params.time = time;
        params.coverage_power = settings.coverage_power;
        params.sun_direction = [sun_direction.x, sun_direction.y, sun_direction.z];
        params.shadow_strength = settings.shadow_strength;
        params.shadow_extent = settings.shadow_extent;
    }

    pub fn record(&mut self) -> CommandStream<Executable> {
        let Some(pipeline) = self.pipeline.as_ref() else {
            return CommandStream::new().begin().end();
        };

        let shadow_resolution = self.shadow_resolution;
        CommandStream::new()
            .begin()
            .combine(self.params.sync_up())
            .prepare_buffer(self.shadow_buffer, UsageBits::COMPUTE_SHADER)
            .gpu_timer_begin(self.timer_index)
            .dispatch(&Dispatch {
                x: (shadow_resolution + SHADOW_WORKGROUP_SIZE - 1) / SHADOW_WORKGROUP_SIZE,
                y: (shadow_resolution + SHADOW_WORKGROUP_SIZE - 1) / SHADOW_WORKGROUP_SIZE,
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
