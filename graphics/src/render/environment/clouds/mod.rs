pub mod cloud_assets;
pub mod cloud_pass_shadow;
pub mod cloud_pass_raymarch;
pub mod cloud_pass_temporal;
pub mod cloud_pass_composite;

use cloud_assets::{CloudAssets, CloudNoiseSizes};
use cloud_pass_composite::CloudCompositePass;
use cloud_pass_raymarch::{CloudLayerSampling, CloudRaymarchPass, CloudSamplingSettings};
use cloud_pass_shadow::CloudShadowPass;
use cloud_pass_temporal::{CloudTemporalPass, TemporalSettings};
use crate::structs::{CloudResolutionScale, CloudSettings};
use dashi::cmd::Executable;
use dashi::driver::command::BlitImage;
use dashi::{CommandStream, Context, Filter, Handle, ImageView, Rect2D, SubresourceRange, Viewport};
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
    pending_weather_map: Option<dashi::ImageView>,
    weather_map_configured: bool,
}

impl CloudRenderer {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        viewport: &Viewport,
        depth_view: dashi::ImageView,
        sample_count: dashi::SampleCount,
        environment_map: ImageView,
    ) -> Self {
        let _ = ctx.init_gpu_timers(TIMER_COUNT);

        let assets = CloudAssets::new(ctx, CloudNoiseSizes::default());
        let settings = CloudSettings::default();
        let low_resolution = calc_low_res(viewport, settings.low_res_scale);

        let shadow_pass = CloudShadowPass::new(
            ctx,
            state,
            &assets,
            settings.shadow.resolution,
            settings.shadow.cascades.cascade_count,
            settings.shadow.cascades.cascade_resolutions,
            TIMER_SHADOW,
        );
        let raymarch_pass = CloudRaymarchPass::new(
            ctx,
            state,
            &assets,
            &shadow_pass,
            environment_map,
            low_resolution,
            TIMER_RAYMARCH,
        );
        let temporal_pass = CloudTemporalPass::new(
            ctx,
            state,
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
            pending_weather_map: None,
            weather_map_configured: true,
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
        _ctx: &mut Context,
        state: &mut BindlessState,
        viewport: &Viewport,
        camera: Handle<furikake::types::Camera>,
        delta_time: f32,
    ) -> CommandStream<Executable> {
        let mut cmd = CommandStream::new().begin();

        if let Some(view) = self.pending_weather_map.as_ref() {
            let size = self.assets.weather_map_size;
            cmd = cmd.blit_images(&BlitImage {
                src: view.img,
                dst: self.assets.weather_map.img,
                src_range: SubresourceRange::new(0, 1, 0, 1),
                dst_range: SubresourceRange::new(0, 1, 0, 1),
                filter: Filter::Linear,
                src_region: Rect2D {
                    x: 0,
                    y: 0,
                    w: size,
                    h: size,
                },
                dst_region: Rect2D {
                    x: 0,
                    y: 0,
                    w: size,
                    h: size,
                },
            });
        }

        if !self.settings.enabled || !self.weather_map_configured {
            self.timings = CloudTimingResult::default();
            return cmd.end();
        }

        let camera_data = match state.reserved::<ReservedBindlessCamera>("meshi_bindless_cameras") {
            Ok(cameras) => cameras.camera(camera),
            Err(_) => {
                warn!("CloudRenderer failed to access bindless cameras");
                return cmd.end();
            }
        };

        let view = camera_data.world_from_camera.inverse();
        let view_proj = camera_data.projection * view;
        let camera_index = camera.slot as u32;

        self.time += delta_time;

        let cascade_count = self.settings.shadow.cascades.cascade_count.max(1);
        let mut shadow_cascade_offsets = [0u32; 4];
        let base_shadow_resolution = self.settings.shadow.resolution.max(1);
        let mut max_shadow_resolution = base_shadow_resolution;
        let mut running_offset = 0u32;
        for cascade_index in 0..cascade_count.min(4) {
            let mut resolution =
                self.settings.shadow.cascades.cascade_resolutions[cascade_index as usize];
            if resolution == 0 {
                resolution = base_shadow_resolution;
            }
            resolution = resolution.max(1);
            shadow_cascade_offsets[cascade_index as usize] = running_offset;
            running_offset = running_offset.saturating_add(resolution.saturating_mul(resolution));
            max_shadow_resolution = max_shadow_resolution.max(resolution);
        }
        let shadow_cascade_splits =
            self.settings
                .shadow
                .cascades
                .compute_splits(camera_data.near, camera_data.far);
        let mut shadow_cascade_strengths = [0.0f32; 4];
        for cascade_index in 0..4 {
            shadow_cascade_strengths[cascade_index] = self.settings.shadow.strength
                * self.settings.shadow.cascades.cascade_strengths[cascade_index];
        }

        let sampling = CloudSamplingSettings {
            output_resolution: self.low_resolution,
            shadow_resolution: max_shadow_resolution,
            base_noise_dims: self.assets.base_noise_dims,
            detail_noise_dims: self.assets.detail_noise_dims,
            weather_map_size: self.assets.weather_map_size,
            layer_a: CloudLayerSampling {
                cloud_base: self.settings.layer_a.base_altitude,
                cloud_top: self.settings.layer_a.top_altitude,
                density_scale: self.settings.layer_a.density_scale,
                noise_scale: self.settings.layer_a.noise_scale,
                wind: self.settings.layer_a.wind * self.settings.layer_a.wind_speed,
                weather_channels: self.assets.weather_layout.layer_a.as_u32(),
            },
            layer_b: CloudLayerSampling {
                cloud_base: self.settings.layer_b.base_altitude,
                cloud_top: self.settings.layer_b.top_altitude,
                density_scale: self.settings.layer_b.density_scale,
                noise_scale: self.settings.layer_b.noise_scale,
                wind: self.settings.layer_b.wind * self.settings.layer_b.wind_speed,
                weather_channels: self.assets.weather_layout.layer_b.as_u32(),
            },
            step_count: self.settings.step_count,
            light_step_count: self.settings.light_step_count,
            phase_g: self.settings.phase_g,
            multi_scatter_strength: self.settings.multi_scatter_strength,
            multi_scatter_respects_shadow: self.settings.multi_scatter_respects_shadow,
            sun_radiance: self.settings.sun_radiance,
            sun_direction: self.settings.sun_direction.normalize_or_zero(),
            coverage_power: self.settings.coverage_power,
            detail_strength: self.settings.detail_strength,
            curl_strength: self.settings.curl_strength,
            jitter_strength: self.settings.jitter_strength,
            shadow_strength: self.settings.shadow.strength,
            shadow_extent: self.settings.shadow.extent,
            shadow_cascade_count: self.settings.shadow.cascades.cascade_count,
            shadow_split_lambda: self.settings.shadow.cascades.split_lambda,
            shadow_cascade_splits,
            shadow_cascade_extents: self.settings.shadow.cascades.cascade_extents,
            shadow_cascade_resolutions: self.settings.shadow.cascades.cascade_resolutions,
            shadow_cascade_offsets,
            shadow_cascade_strengths,
            epsilon: self.settings.epsilon,
            frame_index: self.frame_index,
            time: self.time,
            use_shadow_map: self.settings.shadow.enabled,
            camera_index,
            debug_view: self.settings.debug_view as u32,
            atmosphere_view_strength: self.settings.atmosphere_view_strength,
            atmosphere_view_extinction: self.settings.atmosphere_view_extinction,
            atmosphere_light_transmittance: self.settings.atmosphere_light_transmittance,
        };

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
            self.shadow_pass.shadow_resolution,
            self.temporal_pass.history_index() as u32,
            self.settings.atmosphere_view_strength,
            self.settings.atmosphere_view_extinction,
            self.settings.atmosphere_haze_strength,
            [
                self.settings.atmosphere_haze_color.x,
                self.settings.atmosphere_haze_color.y,
                self.settings.atmosphere_haze_color.z,
                1.0,
            ],
            self.settings.shadow.cascades.cascade_count,
            self.settings.shadow.cascades.cascade_resolutions,
            shadow_cascade_offsets,
        );

        self.timings.shadow_ms = _ctx
            .get_elapsed_gpu_time_ms(TIMER_SHADOW as usize)
            .unwrap_or(0.0);
        self.timings.raymarch_ms = _ctx
            .get_elapsed_gpu_time_ms(TIMER_RAYMARCH as usize)
            .unwrap_or(0.0);
        self.timings.temporal_ms = _ctx
            .get_elapsed_gpu_time_ms(TIMER_TEMPORAL as usize)
            .unwrap_or(0.0);
        self.timings.composite_ms = _ctx
            .get_elapsed_gpu_time_ms(TIMER_COMPOSITE as usize)
            .unwrap_or(0.0);

        cmd.end()
    }

    pub fn record_composite(
        &mut self,
        viewport: &Viewport,
    ) -> dashi::cmd::CommandStream<dashi::cmd::PendingGraphics> {
        if !self.settings.enabled || !self.weather_map_configured {
            return dashi::cmd::CommandStream::<dashi::cmd::PendingGraphics>::subdraw();
        }
        self.composite_pass.record(viewport, TIMER_COMPOSITE)
    }

    pub fn composite_pass(&self) -> &CloudCompositePass {
        &self.composite_pass
    }

    pub fn set_authored_weather_map(&mut self, view: Option<dashi::ImageView>) {
        self.pending_weather_map = view;
        self.weather_map_configured = self.pending_weather_map.is_some();
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
