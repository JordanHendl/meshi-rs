pub mod cloud_assets;
pub mod cloud_pass_shadow;
pub mod cloud_pass_raymarch;
pub mod cloud_pass_temporal;
pub mod cloud_pass_composite;

use cloud_assets::{CloudAssets, CloudNoiseSizes};
use cloud_pass_composite::CloudCompositePass;
use cloud_pass_raymarch::{CloudRaymarchPass, CloudSamplingSettings};
use cloud_pass_shadow::CloudShadowPass;
use cloud_pass_temporal::{CloudTemporalPass, TemporalSettings};
use crate::structs::{CloudResolutionScale, CloudSettings};
use dashi::cmd::Executable;
use dashi::{CommandStream, Context, Handle, Viewport};
use furikake::reservations::bindless_camera::ReservedBindlessCamera;
use furikake::BindlessState;
use glam::Mat4;
use tracing::warn;

const TIMER_SHADOW: u32 = 0;
const TIMER_RAYMARCH: u32 = 1;
const TIMER_TEMPORAL: u32 = 2;
const TIMER_COMPOSITE: u32 = 3;
const TIMER_COUNT: usize = 4;

#[derive(Clone, Copy, Default)]
pub struct CloudTimingResult {
    pub shadow_ms: f32,
    pub raymarch_ms: f32,
    pub temporal_ms: f32,
    pub composite_ms: f32,
}

pub struct CloudRenderer {
    assets: CloudAssets,
    settings: CloudSettings,
    shadow_pass: CloudShadowPass,
    raymarch_pass: CloudRaymarchPass,
    temporal_pass: CloudTemporalPass,
    composite_pass: CloudCompositePass,
    low_resolution: [u32; 2],
    frame_index: u32,
    prev_view_proj: Mat4,
    time: f32,
    timings: CloudTimingResult,
    depth_view: dashi::ImageView,
    sample_count: dashi::SampleCount,
    weather_map_dirty: bool,
}

impl CloudRenderer {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        viewport: &Viewport,
        depth_view: dashi::ImageView,
        sample_count: dashi::SampleCount,
    ) -> Self {
        let _ = ctx.init_gpu_timers(TIMER_COUNT);

        let assets = CloudAssets::new(ctx, CloudNoiseSizes::default());
        let settings = CloudSettings::default();
        let low_resolution = calc_low_res(viewport, settings.low_res_scale);

        let shadow_pass = CloudShadowPass::new(ctx, &assets, settings.shadow.resolution, TIMER_SHADOW);
        let raymarch_pass = CloudRaymarchPass::new(ctx, &assets, &shadow_pass, low_resolution, TIMER_RAYMARCH);
        let temporal_pass = CloudTemporalPass::new(
            ctx,
            low_resolution,
            raymarch_pass.color_buffer,
            raymarch_pass.transmittance_buffer,
            raymarch_pass.depth_buffer,
            TIMER_TEMPORAL,
        );

        let composite_pass = CloudCompositePass::new(
            ctx,
            &assets,
            temporal_pass.history_color,
            temporal_pass.history_transmittance,
            temporal_pass.history_depth,
            raymarch_pass.steps_buffer,
            temporal_pass.history_weight,
            shadow_pass.shadow_buffer,
            depth_view,
            sample_count,
        );

        state.register_pso_tables(composite_pass.pipeline());

        Self {
            assets,
            settings,
            shadow_pass,
            raymarch_pass,
            temporal_pass,
            composite_pass,
            low_resolution,
            frame_index: 0,
            prev_view_proj: Mat4::IDENTITY,
            time: 0.0,
            timings: CloudTimingResult::default(),
            depth_view,
            sample_count,
            weather_map_dirty: false,
        }
    }

    pub fn settings(&self) -> CloudSettings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: CloudSettings) {
        self.settings = settings;
    }

    pub fn timings(&self) -> CloudTimingResult {
        self.timings
    }

    pub fn timing_overlay_text(&self) -> String {
        format!(
            "Clouds (ms) Shadow {:.2} | Ray {:.2} | Temporal {:.2} | Composite {:.2} | Budget {:.2}",
            self.timings.shadow_ms,
            self.timings.raymarch_ms,
            self.timings.temporal_ms,
            self.timings.composite_ms,
            self.settings.performance_budget_ms,
        )
    }

    pub fn update(
        &mut self,
        ctx: &mut Context,
        state: &mut BindlessState,
        viewport: &Viewport,
        camera: Handle<furikake::types::Camera>,
        delta_time: f32,
    ) -> CommandStream<Executable> {
        if !self.settings.enabled {
            self.timings = CloudTimingResult::default();
            return CommandStream::new().begin().end();
        }

        let low_resolution = calc_low_res(viewport, self.settings.low_res_scale);
        let shadow_res_changed = self.settings.shadow.resolution != self.shadow_pass.shadow_resolution;
        let low_res_changed = low_resolution != self.low_resolution;
        if shadow_res_changed || low_res_changed || self.weather_map_dirty {
            if shadow_res_changed {
                self.shadow_pass = CloudShadowPass::new(
                    ctx,
                    &self.assets,
                    self.settings.shadow.resolution,
                    TIMER_SHADOW,
                );
            }
            if low_res_changed || shadow_res_changed || self.weather_map_dirty {
                self.low_resolution = low_resolution;
                self.raymarch_pass = CloudRaymarchPass::new(
                    ctx,
                    &self.assets,
                    &self.shadow_pass,
                    low_resolution,
                    TIMER_RAYMARCH,
                );
                self.temporal_pass = CloudTemporalPass::new(
                    ctx,
                    low_resolution,
                    self.raymarch_pass.color_buffer,
                    self.raymarch_pass.transmittance_buffer,
                    self.raymarch_pass.depth_buffer,
                    TIMER_TEMPORAL,
                );
            }
            self.composite_pass = CloudCompositePass::new(
                ctx,
                &self.assets,
                self.temporal_pass.history_color,
                self.temporal_pass.history_transmittance,
                self.temporal_pass.history_depth,
                self.raymarch_pass.steps_buffer,
                self.temporal_pass.history_weight,
                self.shadow_pass.shadow_buffer,
                self.depth_view,
                self.sample_count,
            );
            state.register_pso_tables(self.composite_pass.pipeline());
            self.weather_map_dirty = false;
        }

        let camera_data = match state.reserved::<ReservedBindlessCamera>("meshi_bindless_cameras") {
            Ok(cameras) => cameras.camera(camera),
            Err(_) => {
                warn!("CloudRenderer failed to access bindless cameras");
                return CommandStream::new().begin().end();
            }
        };

        let view = camera_data.world_from_camera.inverse();
        let view_proj = camera_data.projection * view;
        let inv_view_proj = view_proj.inverse();
        let camera_position = camera_data.world_from_camera.w_axis.truncate();

        self.time += delta_time;

        let sampling = CloudSamplingSettings {
            output_resolution: self.low_resolution,
            shadow_resolution: self.settings.shadow.resolution,
            base_noise_dims: self.assets.base_noise_dims,
            detail_noise_dims: self.assets.detail_noise_dims,
            weather_map_size: self.assets.weather_map_size,
            cloud_base: self.settings.base_altitude,
            cloud_top: self.settings.top_altitude,
            density_scale: self.settings.density_scale,
            step_count: self.settings.step_count,
            light_step_count: self.settings.light_step_count,
            phase_g: self.settings.phase_g,
            sun_radiance: self.settings.sun_radiance,
            sun_direction: self.settings.sun_direction.normalize_or_zero(),
            wind: self.settings.wind * self.settings.wind_speed,
            coverage_power: self.settings.coverage_power,
            detail_strength: self.settings.detail_strength,
            curl_strength: self.settings.curl_strength,
            jitter_strength: self.settings.jitter_strength,
            shadow_strength: self.settings.shadow.strength,
            shadow_extent: self.settings.shadow.extent,
            epsilon: self.settings.epsilon,
            frame_index: self.frame_index,
            time: self.time,
            use_shadow_map: self.settings.shadow.enabled,
            view_proj: view_proj.to_cols_array_2d(),
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            camera_position,
            camera_near: camera_data.near,
            camera_far: camera_data.far,
        };

        let mut cmd = CommandStream::new().begin();
        if self.settings.shadow.enabled {
            self.shadow_pass.update_settings(sampling, sampling.sun_direction, sampling.time);
            cmd = cmd.combine(self.shadow_pass.record());
        }

        self.raymarch_pass.update_settings(sampling);
        cmd = cmd.combine(self.raymarch_pass.record());

        let temporal_settings = TemporalSettings {
            blend_factor: self.settings.temporal.blend_factor,
            clamp_strength: self.settings.temporal.clamp_strength,
            depth_sigma: self.settings.temporal.depth_sigma,
        };

        self.temporal_pass
            .update_params(sampling, temporal_settings, self.prev_view_proj.to_cols_array_2d());
        cmd = cmd.combine(self.temporal_pass.record());

        self.prev_view_proj = view_proj;
        self.frame_index = self.frame_index.wrapping_add(1);

        self.composite_pass.update_params(
            [viewport.area.w as u32, viewport.area.h as u32],
            self.low_resolution,
            camera_data.near,
            camera_data.far,
            self.settings.temporal.depth_sigma,
            self.settings.debug_view,
            self.settings.temporal.history_weight_scale,
            self.settings.shadow.resolution,
            self.temporal_pass.history_index() as u32,
        );

//        self.timings.shadow_ms = ctx
//            .get_elapsed_gpu_time_ms(TIMER_SHADOW as usize)
//            .unwrap_or(0.0);
//        self.timings.raymarch_ms = ctx
//            .get_elapsed_gpu_time_ms(TIMER_RAYMARCH as usize)
//            .unwrap_or(0.0);
//        self.timings.temporal_ms = ctx
//            .get_elapsed_gpu_time_ms(TIMER_TEMPORAL as usize)
//            .unwrap_or(0.0);
//        self.timings.composite_ms = ctx
//            .get_elapsed_gpu_time_ms(TIMER_COMPOSITE as usize)
//            .unwrap_or(0.0);

        cmd.end()
    }

    pub fn record_composite(
        &mut self,
        viewport: &Viewport,
    ) -> dashi::cmd::CommandStream<dashi::cmd::PendingGraphics> {
        if !self.settings.enabled {
            return dashi::cmd::CommandStream::<dashi::cmd::PendingGraphics>::subdraw();
        }
        self.composite_pass.record(viewport, TIMER_COMPOSITE)
    }

    pub fn composite_pass(&self) -> &CloudCompositePass {
        &self.composite_pass
    }

    pub fn set_authored_weather_map(&mut self, view: Option<dashi::ImageView>) {
        self.assets.set_authored_weather_map(view);
        self.weather_map_dirty = true;
    }

    pub fn transmittance_buffer(&self) -> Handle<dashi::Buffer> {
        self.raymarch_pass.transmittance_buffer
    }
}

fn calc_low_res(viewport: &Viewport, scale: CloudResolutionScale) -> [u32; 2] {
    let divisor = match scale {
        CloudResolutionScale::Half => 2,
        CloudResolutionScale::Quarter => 4,
    };
    [
        ((viewport.area.w as u32).max(1) / divisor).max(1),
        ((viewport.area.h as u32).max(1) / divisor).max(1),
    ]
}
