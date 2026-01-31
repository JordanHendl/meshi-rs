pub mod cloud_assets;
pub mod cloud_pass_composite;
pub mod cloud_pass_raymarch;
pub mod cloud_pass_shadow;
pub mod cloud_pass_temporal;

use crate::gui::debug::{
    debug_register_int_with_description, debug_register_radial_with_description,
    debug_register_with_description, DebugRadialOption, DebugRegistryValue, PageType,
};
use crate::gui::Slider;
use crate::structs::{CloudResolutionScale, CloudSettings};
use cloud_assets::{CloudAssets, CloudNoiseSizes};
use cloud_pass_composite::CloudCompositePass;
use cloud_pass_raymarch::{CloudLayerSampling, CloudRaymarchPass, CloudSamplingSettings};
use cloud_pass_shadow::CloudShadowPass;
use cloud_pass_temporal::{CloudTemporalPass, TemporalSettings};
use dashi::cmd::Executable;
use dashi::driver::command::BlitImage;
use dashi::{
    Buffer, CommandStream, Context, Filter, Handle, ImageView, Rect2D, SubresourceRange, Viewport,
};
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
    shadow_map_info: Option<CloudShadowMapInfo>,
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

#[derive(Clone, Copy)]
pub struct CloudShadowMapInfo {
    pub shadow_buffer: Handle<Buffer>,
    pub shadow_resolution: u32,
    pub shadow_cascade_count: u32,
    pub shadow_cascade_resolutions: [u32; 4],
    pub shadow_cascade_offsets: [u32; 4],
    pub shadow_cascade_extents: [f32; 4],
    pub shadow_cascade_splits: [f32; 4],
}

pub fn register_debug(settings: &mut CloudSettings) {
    unsafe {
        debug_register_radial_with_description(
            PageType::Clouds,
            "Clouds Enabled",
            DebugRegistryValue::Bool(&mut settings.enabled),
            &[
                DebugRadialOption {
                    label: "On",
                    value: 1.0,
                },
                DebugRadialOption {
                    label: "Off",
                    value: 0.0,
                },
            ],
            Some("Toggle volumetric cloud rendering."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer A Base Alt", 0.0, 3000.0, 0.0),
            &mut settings.layer_a.base_altitude as *mut f32,
            "Layer A Base Alt",
            Some("Sets the base altitude of cloud layer A in meters."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer A Top Alt", 100.0, 6000.0, 0.0),
            &mut settings.layer_a.top_altitude as *mut f32,
            "Layer A Top Alt",
            Some("Sets the top altitude of cloud layer A in meters."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer A Density", 0.0, 2.0, 0.0),
            &mut settings.layer_a.density_scale as *mut f32,
            "Layer A Density",
            Some("Scales the density of cloud layer A."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer A Noise Scale", 0.1, 2.0, 0.0),
            &mut settings.layer_a.noise_scale as *mut f32,
            "Layer A Noise Scale",
            Some("Adjusts the base noise scale for layer A."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer A Wind X", -5.0, 5.0, 0.0),
            &mut settings.layer_a.wind.x as *mut f32,
            "Layer A Wind X",
            Some("X component of layer A wind direction."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer A Wind Y", -5.0, 5.0, 0.0),
            &mut settings.layer_a.wind.y as *mut f32,
            "Layer A Wind Y",
            Some("Y component of layer A wind direction."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer A Wind Speed", 0.0, 5.0, 0.0),
            &mut settings.layer_a.wind_speed as *mut f32,
            "Layer A Wind Speed",
            Some("Controls the wind speed for cloud layer A."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer B Base Alt", 0.0, 12000.0, 0.0),
            &mut settings.layer_b.base_altitude as *mut f32,
            "Layer B Base Alt",
            Some("Sets the base altitude of cloud layer B in meters."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer B Top Alt", 100.0, 20000.0, 0.0),
            &mut settings.layer_b.top_altitude as *mut f32,
            "Layer B Top Alt",
            Some("Sets the top altitude of cloud layer B in meters."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer B Density", 0.0, 2.0, 0.0),
            &mut settings.layer_b.density_scale as *mut f32,
            "Layer B Density",
            Some("Scales the density of cloud layer B."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer B Noise Scale", 0.1, 2.0, 0.0),
            &mut settings.layer_b.noise_scale as *mut f32,
            "Layer B Noise Scale",
            Some("Adjusts the base noise scale for layer B."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer B Wind X", -5.0, 5.0, 0.0),
            &mut settings.layer_b.wind.x as *mut f32,
            "Layer B Wind X",
            Some("X component of layer B wind direction."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer B Wind Y", -5.0, 5.0, 0.0),
            &mut settings.layer_b.wind.y as *mut f32,
            "Layer B Wind Y",
            Some("Y component of layer B wind direction."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Layer B Wind Speed", 0.0, 5.0, 0.0),
            &mut settings.layer_b.wind_speed as *mut f32,
            "Layer B Wind Speed",
            Some("Controls the wind speed for cloud layer B."),
        );
        debug_register_int_with_description(
            PageType::Clouds,
            Slider::new_int(0, "Step Count", 8.0, 256.0, 0.0),
            &mut settings.step_count as *mut u32,
            "Step Count",
            Some("Number of raymarch steps for primary cloud tracing."),
        );
        debug_register_int_with_description(
            PageType::Clouds,
            Slider::new_int(0, "Light Step Count", 4.0, 128.0, 0.0),
            &mut settings.light_step_count as *mut u32,
            "Light Step Count",
            Some("Number of raymarch steps for cloud lighting."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Phase G", -0.2, 0.9, 0.0),
            &mut settings.phase_g as *mut f32,
            "Phase G",
            Some("Anisotropy term for the cloud phase function."),
        );
        debug_register_radial_with_description(
            PageType::Clouds,
            "Low Res Scale",
            DebugRegistryValue::CloudResolutionScale(&mut settings.low_res_scale),
            &[
                DebugRadialOption {
                    label: "Half",
                    value: 0.0,
                },
                DebugRadialOption {
                    label: "Quarter",
                    value: 1.0,
                },
            ],
            Some("Select the low-resolution rendering scale for clouds."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Coverage Power", 0.1, 4.0, 0.0),
            &mut settings.coverage_power as *mut f32,
            "Coverage Power",
            Some("Adjusts the power curve for cloud coverage."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Detail Strength", 0.0, 2.0, 0.0),
            &mut settings.detail_strength as *mut f32,
            "Detail Strength",
            Some("Controls the amount of high-frequency detail."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Curl Strength", 0.0, 2.0, 0.0),
            &mut settings.curl_strength as *mut f32,
            "Curl Strength",
            Some("Strength of curl noise applied to the clouds."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Jitter Strength", 0.0, 2.0, 0.0),
            &mut settings.jitter_strength as *mut f32,
            "Jitter Strength",
            Some("Controls the amount of raymarch jittering."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Epsilon", 0.0001, 0.1, 0.0),
            &mut settings.epsilon as *mut f32,
            "Epsilon",
            Some("Minimum density threshold for skipping samples."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Multi Scatter Strength", 0.0, 2.0, 0.0),
            &mut settings.multi_scatter_strength as *mut f32,
            "Multi Scatter Strength",
            Some("Controls the strength of multiple scattering."),
        );
        debug_register_radial_with_description(
            PageType::Clouds,
            "Multi Scatter Shadowed",
            DebugRegistryValue::Bool(&mut settings.multi_scatter_respects_shadow),
            &[
                DebugRadialOption {
                    label: "Off",
                    value: 0.0,
                },
                DebugRadialOption {
                    label: "On",
                    value: 1.0,
                },
            ],
            Some("Toggle whether multiple scattering respects shadows."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Sun Radiance R", 0.0, 10.0, 0.0),
            &mut settings.sun_radiance.x as *mut f32,
            "Sun Radiance R",
            Some("Red channel of the sun radiance tint."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Sun Radiance G", 0.0, 10.0, 0.0),
            &mut settings.sun_radiance.y as *mut f32,
            "Sun Radiance G",
            Some("Green channel of the sun radiance tint."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Sun Radiance B", 0.0, 10.0, 0.0),
            &mut settings.sun_radiance.z as *mut f32,
            "Sun Radiance B",
            Some("Blue channel of the sun radiance tint."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Sun Dir X", -1.0, 1.0, 0.0),
            &mut settings.sun_direction.x as *mut f32,
            "Sun Dir X",
            Some("X component of the sun direction."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Sun Dir Y", -1.0, 1.0, 0.0),
            &mut settings.sun_direction.y as *mut f32,
            "Sun Dir Y",
            Some("Y component of the sun direction."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Sun Dir Z", -1.0, 1.0, 0.0),
            &mut settings.sun_direction.z as *mut f32,
            "Sun Dir Z",
            Some("Z component of the sun direction."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Atmos View Strength", 0.0, 1.0, 0.0),
            &mut settings.atmosphere_view_strength as *mut f32,
            "Atmos View Strength",
            Some("Strength of view-ray atmospheric extinction."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Atmos View Extinction", 0.0, 0.005, 0.0),
            &mut settings.atmosphere_view_extinction as *mut f32,
            "Atmos View Extinction",
            Some("Extinction coefficient for the view ray."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Atmos Light Trans", 0.0, 1.0, 0.0),
            &mut settings.atmosphere_light_transmittance as *mut f32,
            "Atmos Light Trans",
            Some("Transmittance applied to incoming light."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Atmos Haze Strength", 0.0, 1.0, 0.0),
            &mut settings.atmosphere_haze_strength as *mut f32,
            "Atmos Haze Strength",
            Some("Strength of atmospheric haze tint."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Atmos Haze R", 0.0, 1.5, 0.0),
            &mut settings.atmosphere_haze_color.x as *mut f32,
            "Atmos Haze R",
            Some("Red channel of the haze color."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Atmos Haze G", 0.0, 1.5, 0.0),
            &mut settings.atmosphere_haze_color.y as *mut f32,
            "Atmos Haze G",
            Some("Green channel of the haze color."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Atmos Haze B", 0.0, 1.5, 0.0),
            &mut settings.atmosphere_haze_color.z as *mut f32,
            "Atmos Haze B",
            Some("Blue channel of the haze color."),
        );
        debug_register_radial_with_description(
            PageType::Shadow,
            "Cloud Shadow Enabled",
            DebugRegistryValue::Bool(&mut settings.shadow.enabled),
            &[
                DebugRadialOption {
                    label: "Off",
                    value: 0.0,
                },
                DebugRadialOption {
                    label: "On",
                    value: 1.0,
                },
            ],
            Some("Toggle cloud shadow map rendering."),
        );
        debug_register_int_with_description(
            PageType::Shadow,
            Slider::new_int(0, "Cloud Shadow Resolution", 64.0, 2048.0, 0.0),
            &mut settings.shadow.resolution as *mut u32,
            "Cloud Shadow Resolution",
            Some("Resolution of the cloud shadow map."),
        );
        debug_register_with_description(
            PageType::Shadow,
            Slider::new(0, "Cloud Shadow Extent", 1000.0, 200000.0, 0.0),
            &mut settings.shadow.extent as *mut f32,
            "Cloud Shadow Extent",
            Some("World-space size covered by cloud shadows."),
        );
        debug_register_with_description(
            PageType::Shadow,
            Slider::new(0, "Cloud Shadow Strength", 0.0, 2.0, 0.0),
            &mut settings.shadow.strength as *mut f32,
            "Cloud Shadow Strength",
            Some("Darkening strength of cloud shadows."),
        );
        debug_register_radial_with_description(
            PageType::Shadow,
            "Cloud Shadow Cascades",
            DebugRegistryValue::U32(&mut settings.shadow.cascades.cascade_count),
            &[
                DebugRadialOption {
                    label: "1",
                    value: 1.0,
                },
                DebugRadialOption {
                    label: "2",
                    value: 2.0,
                },
                DebugRadialOption {
                    label: "3",
                    value: 3.0,
                },
                DebugRadialOption {
                    label: "4",
                    value: 4.0,
                },
            ],
            Some("Select how many cloud shadow cascades are active."),
        );
        debug_register_with_description(
            PageType::Shadow,
            Slider::new(0, "Cloud Shadow Split Lambda", 0.0, 1.0, 0.0),
            &mut settings.shadow.cascades.split_lambda as *mut f32,
            "Cloud Shadow Split Lambda",
            Some("Balances cloud shadow cascade split distribution."),
        );
        debug_register_with_description(
            PageType::Shadow,
            Slider::new(0, "Cloud Shadow Cascade 0 Extent", 1000.0, 200000.0, 0.0),
            &mut settings.shadow.cascades.cascade_extents[0] as *mut f32,
            "Cloud Shadow Cascade 0 Extent",
            Some("Sets the coverage radius for the nearest cloud shadow cascade."),
        );
        debug_register_with_description(
            PageType::Shadow,
            Slider::new(0, "Cloud Shadow Cascade 1 Extent", 1000.0, 200000.0, 0.0),
            &mut settings.shadow.cascades.cascade_extents[1] as *mut f32,
            "Cloud Shadow Cascade 1 Extent",
            Some("Sets the coverage radius for the second cloud shadow cascade."),
        );
        debug_register_with_description(
            PageType::Shadow,
            Slider::new(0, "Cloud Shadow Cascade 2 Extent", 1000.0, 200000.0, 0.0),
            &mut settings.shadow.cascades.cascade_extents[2] as *mut f32,
            "Cloud Shadow Cascade 2 Extent",
            Some("Sets the coverage radius for the third cloud shadow cascade."),
        );
        debug_register_with_description(
            PageType::Shadow,
            Slider::new(0, "Cloud Shadow Cascade 3 Extent", 1000.0, 200000.0, 0.0),
            &mut settings.shadow.cascades.cascade_extents[3] as *mut f32,
            "Cloud Shadow Cascade 3 Extent",
            Some("Sets the coverage radius for the furthest cloud shadow cascade."),
        );
        debug_register_int_with_description(
            PageType::Shadow,
            Slider::new_int(0, "Cloud Shadow Cascade 0 Resolution", 64.0, 2048.0, 0.0),
            &mut settings.shadow.cascades.cascade_resolutions[0] as *mut u32,
            "Cloud Shadow Cascade 0 Resolution",
            Some("Overrides the resolution of the nearest cloud shadow cascade."),
        );
        debug_register_int_with_description(
            PageType::Shadow,
            Slider::new_int(0, "Cloud Shadow Cascade 1 Resolution", 64.0, 2048.0, 0.0),
            &mut settings.shadow.cascades.cascade_resolutions[1] as *mut u32,
            "Cloud Shadow Cascade 1 Resolution",
            Some("Overrides the resolution of the second cloud shadow cascade."),
        );
        debug_register_int_with_description(
            PageType::Shadow,
            Slider::new_int(0, "Cloud Shadow Cascade 2 Resolution", 64.0, 2048.0, 0.0),
            &mut settings.shadow.cascades.cascade_resolutions[2] as *mut u32,
            "Cloud Shadow Cascade 2 Resolution",
            Some("Overrides the resolution of the third cloud shadow cascade."),
        );
        debug_register_int_with_description(
            PageType::Shadow,
            Slider::new_int(0, "Cloud Shadow Cascade 3 Resolution", 64.0, 2048.0, 0.0),
            &mut settings.shadow.cascades.cascade_resolutions[3] as *mut u32,
            "Cloud Shadow Cascade 3 Resolution",
            Some("Overrides the resolution of the furthest cloud shadow cascade."),
        );
        debug_register_with_description(
            PageType::Shadow,
            Slider::new(0, "Cloud Shadow Cascade 0 Strength", 0.0, 2.0, 0.0),
            &mut settings.shadow.cascades.cascade_strengths[0] as *mut f32,
            "Cloud Shadow Cascade 0 Strength",
            Some("Multiplier for the nearest cloud shadow cascade strength."),
        );
        debug_register_with_description(
            PageType::Shadow,
            Slider::new(0, "Cloud Shadow Cascade 1 Strength", 0.0, 2.0, 0.0),
            &mut settings.shadow.cascades.cascade_strengths[1] as *mut f32,
            "Cloud Shadow Cascade 1 Strength",
            Some("Multiplier for the second cloud shadow cascade strength."),
        );
        debug_register_with_description(
            PageType::Shadow,
            Slider::new(0, "Cloud Shadow Cascade 2 Strength", 0.0, 2.0, 0.0),
            &mut settings.shadow.cascades.cascade_strengths[2] as *mut f32,
            "Cloud Shadow Cascade 2 Strength",
            Some("Multiplier for the third cloud shadow cascade strength."),
        );
        debug_register_with_description(
            PageType::Shadow,
            Slider::new(0, "Cloud Shadow Cascade 3 Strength", 0.0, 2.0, 0.0),
            &mut settings.shadow.cascades.cascade_strengths[3] as *mut f32,
            "Cloud Shadow Cascade 3 Strength",
            Some("Multiplier for the furthest cloud shadow cascade strength."),
        );
        debug_register_radial_with_description(
            PageType::Shadow,
            "Cloud Multi Scatter Shadowed",
            DebugRegistryValue::Bool(&mut settings.multi_scatter_respects_shadow),
            &[
                DebugRadialOption {
                    label: "On",
                    value: 1.0,
                },
                DebugRadialOption {
                    label: "Off",
                    value: 0.0,
                },
            ],
            Some("Toggle whether multi-scatter lighting respects cloud shadows."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Temporal Blend", 0.0, 1.0, 0.0),
            &mut settings.temporal.blend_factor as *mut f32,
            "Temporal Blend",
            Some("Controls the blend weight for temporal history."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Temporal Clamp", 0.0, 1.0, 0.0),
            &mut settings.temporal.clamp_strength as *mut f32,
            "Temporal Clamp",
            Some("Clamp strength for temporal reprojection."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Temporal Depth Sigma", 0.1, 100.0, 0.0),
            &mut settings.temporal.depth_sigma as *mut f32,
            "Temporal Depth Sigma",
            Some("Depth sigma used for temporal rejection."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Temporal History Scale", 0.0, 4.0, 0.0),
            &mut settings.temporal.history_weight_scale as *mut f32,
            "Temporal History Scale",
            Some("Scales the weight applied to temporal history."),
        );
        debug_register_radial_with_description(
            PageType::Clouds,
            "Debug View",
            DebugRegistryValue::CloudDebugView(&mut settings.debug_view),
            &[
                DebugRadialOption {
                    label: "None",
                    value: 0.0,
                },
                DebugRadialOption {
                    label: "Weather Map",
                    value: 1.0,
                },
                DebugRadialOption {
                    label: "Transmittance",
                    value: 3.0,
                },
                DebugRadialOption {
                    label: "Step Heatmap",
                    value: 4.0,
                },
                DebugRadialOption {
                    label: "Temporal Weight",
                    value: 5.0,
                },
                DebugRadialOption {
                    label: "Stats",
                    value: 6.0,
                },
                DebugRadialOption {
                    label: "Layer A",
                    value: 7.0,
                },
                DebugRadialOption {
                    label: "Layer B",
                    value: 8.0,
                },
                DebugRadialOption {
                    label: "Single Scatter",
                    value: 9.0,
                },
                DebugRadialOption {
                    label: "Multi Scatter",
                    value: 10.0,
                },
            ],
            Some("Selects a diagnostic view of cloud rendering buffers."),
        );
        debug_register_with_description(
            PageType::Clouds,
            Slider::new(0, "Budget (ms)", 0.1, 20.0, 0.0),
            &mut settings.performance_budget_ms as *mut f32,
            "Budget (ms)",
            Some("Target frame time budget for cloud rendering."),
        );
    }
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
        
        tracing::info!("LOW RES: {:?}", low_resolution);
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
            shadow_map_info: None,
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

    pub fn register_debug(&mut self) {
        register_debug(&mut self.settings);
    }

    pub fn timings(&self) -> CloudTimingResult {
        self.timings
    }

    pub fn shadow_map_info(&self) -> Option<CloudShadowMapInfo> {
        self.shadow_map_info
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
        let bump = crate::render::global_bump().get();
        let _frame_marker = bump.alloc(0u8);
        let mut cmd = CommandStream::new().begin();

        if !self.settings.enabled || !self.weather_map_configured {
            self.timings = CloudTimingResult::default();
            self.shadow_map_info = None;
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
        let shadow_cascade_splits = self
            .settings
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
            self.shadow_pass
                .update_settings(sampling, sampling.sun_direction, sampling.time);
            cmd = cmd.combine(self.shadow_pass.record());
            self.shadow_map_info = Some(CloudShadowMapInfo {
                shadow_buffer: self.shadow_pass.shadow_buffer,
                shadow_resolution: max_shadow_resolution,
                shadow_cascade_count: self.settings.shadow.cascades.cascade_count.max(1),
                shadow_cascade_resolutions: self.settings.shadow.cascades.cascade_resolutions,
                shadow_cascade_offsets,
                shadow_cascade_extents: self.settings.shadow.cascades.cascade_extents,
                shadow_cascade_splits,
            });
        } else {
            self.shadow_map_info = None;
        }

        self.raymarch_pass.update_settings(sampling);
        cmd = cmd.combine(self.raymarch_pass.record());

        let temporal_settings = TemporalSettings {
            blend_factor: self.settings.temporal.blend_factor,
            clamp_strength: self.settings.temporal.clamp_strength,
            depth_sigma: self.settings.temporal.depth_sigma,
        };

        self.temporal_pass.update_params(
            sampling,
            temporal_settings,
            self.prev_view_proj.to_cols_array_2d(),
        );
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

    pub fn pre_compute(&mut self) -> CommandStream<Executable> {
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
        cmd.combine(self.shadow_pass.pre_compute())
            .combine(self.raymarch_pass.pre_compute())
            .combine(self.temporal_pass.pre_compute())
            .combine(self.composite_pass.pre_compute())
            .end()
    }

    pub fn post_compute(&mut self) -> CommandStream<Executable> {
        CommandStream::new().begin().end()
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
