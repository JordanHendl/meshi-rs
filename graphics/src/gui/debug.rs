use glam::{Vec2, Vec3, Vec4, vec2};
use meshi_ffi_structs::event::{Event, EventSource, EventType, KeyCode};

use crate::gui::{
    GuiContext, GuiDraw, GuiLayer, GuiQuad, GuiTextDraw, MenuRect, Slider, SliderColors,
    SliderLayout, SliderMetrics, SliderRenderOptions, SliderState,
};
use crate::render::environment::ocean::OceanFrameSettings;
use crate::render::environment::sky::{SkyFrameSettings, SkyboxFrameSettings};
use crate::structs::{CloudDebugView, CloudResolutionScale, CloudSettings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DebugTab {
    Graphics,
    Physics,
    Audio,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DebugGraphicsTab {
    Sky,
    Ocean,
    Clouds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragTarget {
    DebugPanel,
}

#[derive(Clone, Copy, Debug)]
struct DebugSliderValues {
    skybox_intensity: f32,
    sun_intensity: f32,
    sun_angular_radius: f32,
    moon_intensity: f32,
    moon_angular_radius: f32,
    ocean_wind_speed: f32,
    ocean_wave_amplitude: f32,
    ocean_gerstner_amplitude: f32,
    ocean_fresnel_bias: f32,
    ocean_fresnel_strength: f32,
    ocean_foam_strength: f32,
    ocean_foam_threshold: f32,
    ocean_capillary_strength: f32,
    ocean_time_scale: f32,
    cloud_enabled: f32,
    cloud_layer_a_base_altitude: f32,
    cloud_layer_a_top_altitude: f32,
    cloud_layer_a_density_scale: f32,
    cloud_layer_a_noise_scale: f32,
    cloud_layer_a_wind_x: f32,
    cloud_layer_a_wind_y: f32,
    cloud_layer_a_wind_speed: f32,
    cloud_layer_b_base_altitude: f32,
    cloud_layer_b_top_altitude: f32,
    cloud_layer_b_density_scale: f32,
    cloud_layer_b_noise_scale: f32,
    cloud_layer_b_wind_x: f32,
    cloud_layer_b_wind_y: f32,
    cloud_layer_b_wind_speed: f32,
    cloud_light_step_count: f32,
    cloud_coverage_power: f32,
    cloud_detail_strength: f32,
    cloud_curl_strength: f32,
    cloud_jitter_strength: f32,
    cloud_epsilon: f32,
    cloud_low_res_scale: f32,
    cloud_phase_g: f32,
    cloud_step_count: f32,
    cloud_sun_radiance_r: f32,
    cloud_sun_radiance_g: f32,
    cloud_sun_radiance_b: f32,
    cloud_sun_direction_x: f32,
    cloud_sun_direction_y: f32,
    cloud_sun_direction_z: f32,
    cloud_shadow_enabled: f32,
    cloud_shadow_resolution: f32,
    cloud_shadow_extent: f32,
    cloud_shadow_strength: f32,
    cloud_shadow_cascade_count: f32,
    cloud_shadow_split_lambda: f32,
    cloud_temporal_blend_factor: f32,
    cloud_temporal_clamp_strength: f32,
    cloud_temporal_depth_sigma: f32,
    cloud_temporal_history_weight_scale: f32,
    cloud_debug_view: f32,
    cloud_performance_budget_ms: f32,
}

pub struct DebugGuiBindings {
    pub debug_mode: *mut bool,
    pub skybox_settings: *mut SkyboxFrameSettings,
    pub sky_settings: *mut SkyFrameSettings,
    pub ocean_settings: *mut OceanFrameSettings,
    pub cloud_settings: *mut CloudSettings,
}

pub struct DebugGuiOutput {
    pub frame: Option<crate::gui::GuiFrame>,
    pub skybox_dirty: bool,
    pub sky_dirty: bool,
    pub ocean_dirty: bool,
    pub cloud_dirty: bool,
}

pub struct DebugGui {
    cursor: Vec2,
    mouse_pressed: bool,
    mouse_down: bool,
    control_down: bool,
    debug_toggle_requested: bool,
    debug_tab: DebugTab,
    debug_graphics_tab: DebugGraphicsTab,
    debug_slider_state: SliderState,
    debug_slider_layout: SliderLayout,
    debug_panel_position: Vec2,
    drag_target: Option<DragTarget>,
    drag_offset: Vec2,
    scroll_offset: f32,
    scroll_delta: f32,
    slider_values: DebugSliderValues,
}

impl DebugGui {
    pub fn new() -> Self {
        Self {
            cursor: Vec2::ZERO,
            mouse_pressed: false,
            mouse_down: false,
            control_down: false,
            debug_toggle_requested: false,
            debug_tab: DebugTab::Graphics,
            debug_graphics_tab: DebugGraphicsTab::Sky,
            debug_slider_state: SliderState::default(),
            debug_slider_layout: SliderLayout::default(),
            debug_panel_position: vec2(560.0, 60.0),
            drag_target: None,
            drag_offset: Vec2::ZERO,
            scroll_offset: 0.0,
            scroll_delta: 0.0,
            slider_values: DebugSliderValues {
                skybox_intensity: 1.0,
                sun_intensity: 1.0,
                sun_angular_radius: 0.0045,
                moon_intensity: 0.1,
                moon_angular_radius: 0.0045,
                ocean_wind_speed: 2.0,
                ocean_wave_amplitude: 2.0,
                ocean_gerstner_amplitude: 0.12,
                ocean_fresnel_bias: 0.02,
                ocean_fresnel_strength: 0.85,
                ocean_foam_strength: 1.0,
                ocean_foam_threshold: 0.55,
                ocean_capillary_strength: 1.0,
                ocean_time_scale: 1.0,
                cloud_enabled: 1.0,
                cloud_layer_a_base_altitude: 300.0,
                cloud_layer_a_top_altitude: 400.0,
                cloud_layer_a_density_scale: 0.5,
                cloud_layer_a_noise_scale: 1.0,
                cloud_layer_a_wind_x: 1.0,
                cloud_layer_a_wind_y: 0.0,
                cloud_layer_a_wind_speed: 0.2,
                cloud_layer_b_base_altitude: 650.0,
                cloud_layer_b_top_altitude: 900.0,
                cloud_layer_b_density_scale: 0.22,
                cloud_layer_b_noise_scale: 0.7,
                cloud_layer_b_wind_x: -0.4,
                cloud_layer_b_wind_y: 0.2,
                cloud_layer_b_wind_speed: 0.35,
                cloud_light_step_count: 18.0,
                cloud_coverage_power: 1.2,
                cloud_detail_strength: 0.6,
                cloud_curl_strength: 0.0,
                cloud_jitter_strength: 1.0,
                cloud_epsilon: 0.01,
                cloud_low_res_scale: 0.0,
                cloud_phase_g: 0.6,
                cloud_step_count: 96.0,
                cloud_sun_radiance_r: 1.0,
                cloud_sun_radiance_g: 1.0,
                cloud_sun_radiance_b: 1.0,
                cloud_sun_direction_x: 0.0,
                cloud_sun_direction_y: -1.0,
                cloud_sun_direction_z: 0.0,
                cloud_shadow_enabled: 0.0,
                cloud_shadow_resolution: 256.0,
                cloud_shadow_extent: 50000.0,
                cloud_shadow_strength: 1.0,
                cloud_shadow_cascade_count: 1.0,
                cloud_shadow_split_lambda: 0.5,
                cloud_temporal_blend_factor: 0.9,
                cloud_temporal_clamp_strength: 0.7,
                cloud_temporal_depth_sigma: 15.0,
                cloud_temporal_history_weight_scale: 1.0,
                cloud_debug_view: 0.0,
                cloud_performance_budget_ms: 4.0,
            },
        }
    }

    pub fn handle_event(&mut self, event: &Event) {
        unsafe {
            if event.source() == EventSource::Window && event.event_type() == EventType::Quit {
                return;
            }
            if event.source() == EventSource::Mouse && event.event_type() == EventType::CursorMoved
            {
                self.cursor = event.motion2d();
            }
            if event.source() == EventSource::Mouse && event.event_type() == EventType::Motion2D {
                self.scroll_delta += event.motion2d().y;
            }
            if event.source() == EventSource::MouseButton {
                if event.event_type() == EventType::Pressed {
                    self.mouse_pressed = true;
                    self.mouse_down = true;
                }
                if event.event_type() == EventType::Released {
                    self.mouse_down = false;
                }
            }
            if event.source() == EventSource::Key {
                if event.event_type() == EventType::Pressed {
                    match event.key() {
                        KeyCode::Control => self.control_down = true,
                        KeyCode::GraveAccent => {
                            if self.control_down {
                                self.debug_toggle_requested = true;
                            }
                        }
                        _ => {}
                    }
                }
                if event.event_type() == EventType::Released {
                    if event.key() == KeyCode::Control {
                        self.control_down = false;
                    }
                }
            }
        }
    }

    pub fn build_frame(
        &mut self,
        viewport: Vec2,
        renderer_label: &str,
        average_frame_time_ms: Option<f64>,
        bindings: DebugGuiBindings,
    ) -> DebugGuiOutput {
        if self.debug_toggle_requested {
            self.debug_toggle_requested = false;
            unsafe {
                if let Some(debug_mode) = bindings.debug_mode.as_mut() {
                    *debug_mode = !*debug_mode;
                }
            }
        }

        let debug_mode = unsafe { bindings.debug_mode.as_ref().copied().unwrap_or(false) };
        if !debug_mode {
            self.reset_for_hidden();
            let mut cloud_dirty = false;
            unsafe {
                if let Some(clouds) = bindings.cloud_settings.as_mut() {
                    if clouds.debug_view == CloudDebugView::Stats {
                        clouds.debug_view = CloudDebugView::None;
                        cloud_dirty = true;
                    }
                }
            }
            return DebugGuiOutput {
                frame: None,
                skybox_dirty: false,
                sky_dirty: false,
                ocean_dirty: false,
                cloud_dirty,
            };
        }

        if self.debug_slider_state.active.is_none() {
            unsafe {
                if let Some(skybox) = bindings.skybox_settings.as_ref() {
                    self.slider_values.skybox_intensity = skybox.intensity;
                }
                if let Some(sky) = bindings.sky_settings.as_ref() {
                    self.slider_values.sun_intensity = sky.sun_intensity;
                    self.slider_values.sun_angular_radius = sky.sun_angular_radius;
                    self.slider_values.moon_intensity = sky.moon_intensity;
                    self.slider_values.moon_angular_radius = sky.moon_angular_radius;
                }
                if let Some(ocean) = bindings.ocean_settings.as_ref() {
                    self.slider_values.ocean_wind_speed = ocean.wind_speed;
                    self.slider_values.ocean_wave_amplitude = ocean.wave_amplitude;
                    self.slider_values.ocean_gerstner_amplitude = ocean.gerstner_amplitude;
                    self.slider_values.ocean_fresnel_bias = ocean.fresnel_bias;
                    self.slider_values.ocean_fresnel_strength = ocean.fresnel_strength;
                    self.slider_values.ocean_foam_strength = ocean.foam_strength;
                    self.slider_values.ocean_foam_threshold = ocean.foam_threshold;
                    self.slider_values.ocean_capillary_strength = ocean.capillary_strength;
                    self.slider_values.ocean_time_scale = ocean.time_scale;
                }
                if let Some(clouds) = bindings.cloud_settings.as_ref() {
                    self.slider_values.cloud_enabled = clouds.enabled as u32 as f32;
                    self.slider_values.cloud_layer_a_base_altitude = clouds.layer_a.base_altitude;
                    self.slider_values.cloud_layer_a_top_altitude = clouds.layer_a.top_altitude;
                    self.slider_values.cloud_layer_a_density_scale = clouds.layer_a.density_scale;
                    self.slider_values.cloud_layer_a_noise_scale = clouds.layer_a.noise_scale;
                    self.slider_values.cloud_layer_a_wind_x = clouds.layer_a.wind.x;
                    self.slider_values.cloud_layer_a_wind_y = clouds.layer_a.wind.y;
                    self.slider_values.cloud_layer_a_wind_speed = clouds.layer_a.wind_speed;
                    self.slider_values.cloud_layer_b_base_altitude = clouds.layer_b.base_altitude;
                    self.slider_values.cloud_layer_b_top_altitude = clouds.layer_b.top_altitude;
                    self.slider_values.cloud_layer_b_density_scale = clouds.layer_b.density_scale;
                    self.slider_values.cloud_layer_b_noise_scale = clouds.layer_b.noise_scale;
                    self.slider_values.cloud_layer_b_wind_x = clouds.layer_b.wind.x;
                    self.slider_values.cloud_layer_b_wind_y = clouds.layer_b.wind.y;
                    self.slider_values.cloud_layer_b_wind_speed = clouds.layer_b.wind_speed;
                    self.slider_values.cloud_light_step_count = clouds.light_step_count as f32;
                    self.slider_values.cloud_coverage_power = clouds.coverage_power;
                    self.slider_values.cloud_detail_strength = clouds.detail_strength;
                    self.slider_values.cloud_curl_strength = clouds.curl_strength;
                    self.slider_values.cloud_jitter_strength = clouds.jitter_strength;
                    self.slider_values.cloud_epsilon = clouds.epsilon;
                    self.slider_values.cloud_low_res_scale =
                        cloud_resolution_scale_value(clouds.low_res_scale);
                    self.slider_values.cloud_phase_g = clouds.phase_g;
                    self.slider_values.cloud_step_count = clouds.step_count as f32;
                    self.slider_values.cloud_sun_radiance_r = clouds.sun_radiance.x;
                    self.slider_values.cloud_sun_radiance_g = clouds.sun_radiance.y;
                    self.slider_values.cloud_sun_radiance_b = clouds.sun_radiance.z;
                    self.slider_values.cloud_sun_direction_x = clouds.sun_direction.x;
                    self.slider_values.cloud_sun_direction_y = clouds.sun_direction.y;
                    self.slider_values.cloud_sun_direction_z = clouds.sun_direction.z;
                    self.slider_values.cloud_shadow_enabled = clouds.shadow.enabled as u32 as f32;
                    self.slider_values.cloud_shadow_resolution = clouds.shadow.resolution as f32;
                    self.slider_values.cloud_shadow_extent = clouds.shadow.extent;
                    self.slider_values.cloud_shadow_strength = clouds.shadow.strength;
                    self.slider_values.cloud_shadow_cascade_count =
                        clouds.shadow.cascades.cascade_count as f32;
                    self.slider_values.cloud_shadow_split_lambda =
                        clouds.shadow.cascades.split_lambda;
                    self.slider_values.cloud_temporal_blend_factor = clouds.temporal.blend_factor;
                    self.slider_values.cloud_temporal_clamp_strength = clouds.temporal.clamp_strength;
                    self.slider_values.cloud_temporal_depth_sigma = clouds.temporal.depth_sigma;
                    self.slider_values.cloud_temporal_history_weight_scale =
                        clouds.temporal.history_weight_scale;
                    self.slider_values.cloud_debug_view = clouds.debug_view as u32 as f32;
                    self.slider_values.cloud_performance_budget_ms = clouds.performance_budget_ms;
                }
            }
        }

        let debug_panel_width = (viewport.x * 0.34).clamp(320.0, 520.0);
        let debug_panel_height = (viewport.y * 0.7).clamp(360.0, 720.0);
        let debug_panel_size = vec2(debug_panel_width, debug_panel_height);
        let ui_scale = (debug_panel_height / 520.0).clamp(0.85, 1.2);
        let debug_title_bar_height = 28.0 * ui_scale;
        let debug_taskbar_height = 26.0 * ui_scale;
        let panel_margin = 16.0 * ui_scale;
        let panel_min_pos = vec2(panel_margin, panel_margin);
        let panel_max_pos = vec2(
            (viewport.x - debug_panel_size.x - panel_margin).max(panel_margin),
            (viewport.y - debug_panel_size.y - panel_margin).max(panel_margin),
        );
        self.debug_panel_position = self.debug_panel_position.clamp(panel_min_pos, panel_max_pos);
        let debug_panel_position = self.debug_panel_position;
        let debug_title_bar_pos = debug_panel_position;
        let debug_title_bar_size = vec2(debug_panel_size.x, debug_title_bar_height);
        let debug_title_hovered =
            point_in_rect(self.cursor, debug_title_bar_pos, debug_title_bar_size);
        let debug_taskbar_pos = vec2(
            debug_panel_position.x,
            debug_panel_position.y + debug_panel_size.y - debug_taskbar_height,
        );
        let debug_taskbar_size = vec2(debug_panel_size.x, debug_taskbar_height);
        let close_button_size = vec2(64.0, 18.0) * ui_scale;
        let reset_button_size = vec2(92.0, 18.0) * ui_scale;
        let button_padding = 10.0 * ui_scale;
        let button_gap = 8.0 * ui_scale;
        let close_button_pos = vec2(
            debug_taskbar_pos.x + debug_taskbar_size.x - button_padding - close_button_size.x,
            debug_taskbar_pos.y + 4.0 * ui_scale,
        );
        let reset_button_pos = vec2(
            close_button_pos.x - button_gap - reset_button_size.x,
            debug_taskbar_pos.y + 4.0 * ui_scale,
        );
        let close_button_hovered =
            point_in_rect(self.cursor, close_button_pos, close_button_size);
        let reset_button_hovered =
            point_in_rect(self.cursor, reset_button_pos, reset_button_size);
        let mut close_requested = false;

        if self.mouse_pressed {
            if close_button_hovered {
                close_requested = true;
            } else if reset_button_hovered {
                self.debug_panel_position = vec2(
                    (viewport.x - debug_panel_size.x - panel_margin).max(panel_margin),
                    panel_margin,
                );
                self.scroll_offset = 0.0;
            }
            if debug_title_hovered {
                self.drag_target = Some(DragTarget::DebugPanel);
                self.drag_offset = self.cursor - debug_panel_position;
            }
        }

        if !self.mouse_down {
            self.drag_target = None;
        }

        if let Some(DragTarget::DebugPanel) = self.drag_target {
            if self.mouse_down {
                let new_pos = self.cursor - self.drag_offset;
                self.debug_panel_position = new_pos;
            }
        }

        if self.mouse_pressed {
            let tab_height = 26.0 * ui_scale;
            let tab_padding = 12.0 * ui_scale;
            let tab_gap = 6.0 * ui_scale;
            let tab_width = (debug_panel_size.x - tab_padding * 2.0) / 3.0;
            let tab_y = debug_title_bar_pos.y + debug_title_bar_height + 6.0 * ui_scale;
            let tab_x = debug_panel_position.x + tab_padding;
            let tabs = [DebugTab::Graphics, DebugTab::Physics, DebugTab::Audio];
            for (index, tab) in tabs.iter().enumerate() {
                let tab_pos = vec2(tab_x + tab_width * index as f32, tab_y);
                let tab_size = vec2(tab_width - tab_gap, tab_height);
                if point_in_rect(self.cursor, tab_pos, tab_size) {
                    self.debug_tab = *tab;
                    self.debug_slider_state.active = None;
                    self.scroll_offset = 0.0;
                }
            }
        }

        if self.mouse_pressed && self.debug_tab == DebugTab::Graphics {
            let tab_height = 26.0 * ui_scale;
            let subtab_height = 22.0 * ui_scale;
            let subtab_padding = 12.0 * ui_scale;
            let subtab_gap = 6.0 * ui_scale;
            let subtab_width = (debug_panel_size.x - subtab_padding * 2.0) / 3.0;
            let subtab_y = debug_title_bar_pos.y
                + debug_title_bar_height
                + 6.0 * ui_scale
                + tab_height
                + 6.0 * ui_scale;
            let subtab_x = debug_panel_position.x + subtab_padding;
            let subtabs = [
                DebugGraphicsTab::Sky,
                DebugGraphicsTab::Ocean,
                DebugGraphicsTab::Clouds,
            ];
            for (index, tab) in subtabs.iter().enumerate() {
                let tab_pos = vec2(subtab_x + subtab_width * index as f32, subtab_y);
                let tab_size = vec2(subtab_width - subtab_gap, subtab_height);
                if point_in_rect(self.cursor, tab_pos, tab_size) {
                    self.debug_graphics_tab = *tab;
                    self.debug_slider_state.active = None;
                    self.scroll_offset = 0.0;
                }
            }
        }

        let hovered_debug_slider = if self.debug_tab == DebugTab::Graphics {
            self.debug_slider_layout.items.iter().find(|item| {
                item.enabled
                    && (point_in_menu_rect(self.cursor, item.track_rect)
                        || point_in_menu_rect(self.cursor, item.knob_rect))
            })
        } else {
            None
        };
        self.debug_slider_state.hovered = hovered_debug_slider.map(|item| item.id);

        if self.mouse_pressed {
            if let Some(item) = hovered_debug_slider {
                self.debug_slider_state.active = Some(item.id);
            }
        }

        if !self.mouse_down {
            self.debug_slider_state.active = None;
        }

        if let Some(active_id) = self.debug_slider_state.active {
            if let Some(item) = self
                .debug_slider_layout
                .items
                .iter()
                .find(|item| item.id == active_id && item.enabled)
            {
                let value =
                    slider_value_from_cursor(self.cursor, item.track_rect, item.min, item.max);
                match active_id {
                    101 => self.slider_values.skybox_intensity = value,
                    102 => self.slider_values.sun_intensity = value,
                    103 => self.slider_values.sun_angular_radius = value,
                    104 => self.slider_values.moon_intensity = value,
                    105 => self.slider_values.moon_angular_radius = value,
                    201 => self.slider_values.ocean_wind_speed = value,
                    202 => self.slider_values.ocean_wave_amplitude = value,
                    203 => self.slider_values.ocean_gerstner_amplitude = value,
                    204 => self.slider_values.ocean_fresnel_bias = value,
                    205 => self.slider_values.ocean_fresnel_strength = value,
                    206 => self.slider_values.ocean_foam_strength = value,
                    207 => self.slider_values.ocean_foam_threshold = value,
                    208 => self.slider_values.ocean_capillary_strength = value,
                    209 => self.slider_values.ocean_time_scale = value,
                    300 => self.slider_values.cloud_enabled = value,
                    301 => self.slider_values.cloud_layer_a_base_altitude = value,
                    302 => self.slider_values.cloud_layer_a_top_altitude = value,
                    303 => self.slider_values.cloud_layer_a_density_scale = value,
                    304 => self.slider_values.cloud_layer_a_noise_scale = value,
                    305 => self.slider_values.cloud_layer_a_wind_x = value,
                    306 => self.slider_values.cloud_layer_a_wind_y = value,
                    307 => self.slider_values.cloud_layer_a_wind_speed = value,
                    308 => self.slider_values.cloud_layer_b_base_altitude = value,
                    309 => self.slider_values.cloud_layer_b_top_altitude = value,
                    310 => self.slider_values.cloud_layer_b_density_scale = value,
                    311 => self.slider_values.cloud_layer_b_noise_scale = value,
                    312 => self.slider_values.cloud_layer_b_wind_x = value,
                    313 => self.slider_values.cloud_layer_b_wind_y = value,
                    314 => self.slider_values.cloud_layer_b_wind_speed = value,
                    315 => self.slider_values.cloud_step_count = value,
                    316 => self.slider_values.cloud_light_step_count = value,
                    317 => self.slider_values.cloud_phase_g = value,
                    318 => self.slider_values.cloud_low_res_scale = value,
                    319 => self.slider_values.cloud_coverage_power = value,
                    320 => self.slider_values.cloud_detail_strength = value,
                    321 => self.slider_values.cloud_curl_strength = value,
                    322 => self.slider_values.cloud_jitter_strength = value,
                    323 => self.slider_values.cloud_epsilon = value,
                    324 => self.slider_values.cloud_sun_radiance_r = value,
                    325 => self.slider_values.cloud_sun_radiance_g = value,
                    326 => self.slider_values.cloud_sun_radiance_b = value,
                    327 => self.slider_values.cloud_sun_direction_x = value,
                    328 => self.slider_values.cloud_sun_direction_y = value,
                    329 => self.slider_values.cloud_sun_direction_z = value,
                    330 => self.slider_values.cloud_shadow_enabled = value,
                    331 => self.slider_values.cloud_shadow_resolution = value,
                    332 => self.slider_values.cloud_shadow_extent = value,
                    333 => self.slider_values.cloud_shadow_strength = value,
                    334 => self.slider_values.cloud_shadow_cascade_count = value,
                    335 => self.slider_values.cloud_shadow_split_lambda = value,
                    336 => self.slider_values.cloud_temporal_blend_factor = value,
                    337 => self.slider_values.cloud_temporal_clamp_strength = value,
                    338 => self.slider_values.cloud_temporal_depth_sigma = value,
                    339 => self.slider_values.cloud_temporal_history_weight_scale = value,
                    340 => self.slider_values.cloud_debug_view = value,
                    341 => self.slider_values.cloud_performance_budget_ms = value,
                    _ => {}
                }
            }
        }

        if close_requested {
            unsafe {
                if let Some(debug_mode) = bindings.debug_mode.as_mut() {
                    *debug_mode = false;
                }
            }
            self.reset_for_hidden();
            return DebugGuiOutput {
                frame: None,
                skybox_dirty: false,
                sky_dirty: false,
                ocean_dirty: false,
                cloud_dirty: false,
            };
        }

        let mut skybox_dirty = false;
        let mut sky_dirty = false;
        let mut ocean_dirty = false;
        let mut cloud_dirty = false;
        unsafe {
            if let Some(skybox) = bindings.skybox_settings.as_mut() {
                let new_value = self.slider_values.skybox_intensity.clamp(0.2, 2.0);
                if (skybox.intensity - new_value).abs() > f32::EPSILON {
                    skybox.intensity = new_value;
                    skybox_dirty = true;
                }
            }
            if let Some(sky) = bindings.sky_settings.as_mut() {
                let new_value = self.slider_values.sun_intensity.clamp(0.1, 5.0);
                if (sky.sun_intensity - new_value).abs() > f32::EPSILON {
                    sky.sun_intensity = new_value;
                    sky_dirty = true;
                }
                let new_value = self.slider_values.sun_angular_radius.clamp(0.001, 0.05);
                if (sky.sun_angular_radius - new_value).abs() > f32::EPSILON {
                    sky.sun_angular_radius = new_value;
                    sky_dirty = true;
                }
                let new_value = self.slider_values.moon_intensity.clamp(0.0, 2.0);
                if (sky.moon_intensity - new_value).abs() > f32::EPSILON {
                    sky.moon_intensity = new_value;
                    sky_dirty = true;
                }
                let new_value = self.slider_values.moon_angular_radius.clamp(0.001, 0.05);
                if (sky.moon_angular_radius - new_value).abs() > f32::EPSILON {
                    sky.moon_angular_radius = new_value;
                    sky_dirty = true;
                }
            }
            if let Some(ocean) = bindings.ocean_settings.as_mut() {
                let new_value = self.slider_values.ocean_wind_speed.clamp(0.1, 20.0);
                if (ocean.wind_speed - new_value).abs() > f32::EPSILON {
                    ocean.wind_speed = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_wave_amplitude.clamp(0.1, 10.0);
                if (ocean.wave_amplitude - new_value).abs() > f32::EPSILON {
                    ocean.wave_amplitude = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_gerstner_amplitude.clamp(0.0, 1.0);
                if (ocean.gerstner_amplitude - new_value).abs() > f32::EPSILON {
                    ocean.gerstner_amplitude = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_fresnel_bias.clamp(0.0, 0.2);
                if (ocean.fresnel_bias - new_value).abs() > f32::EPSILON {
                    ocean.fresnel_bias = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_fresnel_strength.clamp(0.0, 1.5);
                if (ocean.fresnel_strength - new_value).abs() > f32::EPSILON {
                    ocean.fresnel_strength = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_foam_strength.clamp(0.0, 4.0);
                if (ocean.foam_strength - new_value).abs() > f32::EPSILON {
                    ocean.foam_strength = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_foam_threshold.clamp(0.0, 1.0);
                if (ocean.foam_threshold - new_value).abs() > f32::EPSILON {
                    ocean.foam_threshold = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_capillary_strength.clamp(0.0, 2.0);
                if (ocean.capillary_strength - new_value).abs() > f32::EPSILON {
                    ocean.capillary_strength = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_time_scale.clamp(0.1, 4.0);
                if (ocean.time_scale - new_value).abs() > f32::EPSILON {
                    ocean.time_scale = new_value;
                    ocean_dirty = true;
                }
            }
            if let Some(clouds) = bindings.cloud_settings.as_mut() {
                let new_enabled = self.slider_values.cloud_enabled >= 0.5;
                if clouds.enabled != new_enabled {
                    clouds.enabled = new_enabled;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_layer_a_base_altitude.clamp(0.0, 3000.0);
                if (clouds.layer_a.base_altitude - new_value).abs() > f32::EPSILON {
                    clouds.layer_a.base_altitude = new_value;
                    cloud_dirty = true;
                }
                let min_top = clouds.layer_a.base_altitude + 10.0;
                let new_value = self
                    .slider_values
                    .cloud_layer_a_top_altitude
                    .clamp(min_top, 6000.0);
                if (clouds.layer_a.top_altitude - new_value).abs() > f32::EPSILON {
                    clouds.layer_a.top_altitude = new_value;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_layer_a_density_scale.clamp(0.0, 2.0);
                if (clouds.layer_a.density_scale - new_value).abs() > f32::EPSILON {
                    clouds.layer_a.density_scale = new_value;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_layer_a_noise_scale.clamp(0.1, 2.0);
                if (clouds.layer_a.noise_scale - new_value).abs() > f32::EPSILON {
                    clouds.layer_a.noise_scale = new_value;
                    cloud_dirty = true;
                }
                let new_wind_x = self.slider_values.cloud_layer_a_wind_x.clamp(-5.0, 5.0);
                let new_wind_y = self.slider_values.cloud_layer_a_wind_y.clamp(-5.0, 5.0);
                if (clouds.layer_a.wind.x - new_wind_x).abs() > f32::EPSILON
                    || (clouds.layer_a.wind.y - new_wind_y).abs() > f32::EPSILON
                {
                    clouds.layer_a.wind = Vec2::new(new_wind_x, new_wind_y);
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_layer_a_wind_speed.clamp(0.0, 5.0);
                if (clouds.layer_a.wind_speed - new_value).abs() > f32::EPSILON {
                    clouds.layer_a.wind_speed = new_value;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_layer_b_base_altitude.clamp(0.0, 12000.0);
                if (clouds.layer_b.base_altitude - new_value).abs() > f32::EPSILON {
                    clouds.layer_b.base_altitude = new_value;
                    cloud_dirty = true;
                }
                let min_top = clouds.layer_b.base_altitude + 10.0;
                let new_value = self
                    .slider_values
                    .cloud_layer_b_top_altitude
                    .clamp(min_top, 20000.0);
                if (clouds.layer_b.top_altitude - new_value).abs() > f32::EPSILON {
                    clouds.layer_b.top_altitude = new_value;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_layer_b_density_scale.clamp(0.0, 2.0);
                if (clouds.layer_b.density_scale - new_value).abs() > f32::EPSILON {
                    clouds.layer_b.density_scale = new_value;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_layer_b_noise_scale.clamp(0.1, 2.0);
                if (clouds.layer_b.noise_scale - new_value).abs() > f32::EPSILON {
                    clouds.layer_b.noise_scale = new_value;
                    cloud_dirty = true;
                }
                let new_wind_x = self.slider_values.cloud_layer_b_wind_x.clamp(-5.0, 5.0);
                let new_wind_y = self.slider_values.cloud_layer_b_wind_y.clamp(-5.0, 5.0);
                if (clouds.layer_b.wind.x - new_wind_x).abs() > f32::EPSILON
                    || (clouds.layer_b.wind.y - new_wind_y).abs() > f32::EPSILON
                {
                    clouds.layer_b.wind = Vec2::new(new_wind_x, new_wind_y);
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_layer_b_wind_speed.clamp(0.0, 5.0);
                if (clouds.layer_b.wind_speed - new_value).abs() > f32::EPSILON {
                    clouds.layer_b.wind_speed = new_value;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_step_count.clamp(8.0, 256.0).round();
                let new_steps = new_value as u32;
                if clouds.step_count != new_steps {
                    clouds.step_count = new_steps;
                    cloud_dirty = true;
                }
                let new_value =
                    self.slider_values.cloud_light_step_count.clamp(4.0, 128.0).round();
                let new_steps = new_value as u32;
                if clouds.light_step_count != new_steps {
                    clouds.light_step_count = new_steps;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_phase_g.clamp(-0.2, 0.9);
                if (clouds.phase_g - new_value).abs() > f32::EPSILON {
                    clouds.phase_g = new_value;
                    cloud_dirty = true;
                }
                let new_scale = cloud_resolution_scale_from_value(self.slider_values.cloud_low_res_scale);
                if clouds.low_res_scale != new_scale {
                    clouds.low_res_scale = new_scale;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_coverage_power.clamp(0.1, 4.0);
                if (clouds.coverage_power - new_value).abs() > f32::EPSILON {
                    clouds.coverage_power = new_value;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_detail_strength.clamp(0.0, 2.0);
                if (clouds.detail_strength - new_value).abs() > f32::EPSILON {
                    clouds.detail_strength = new_value;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_curl_strength.clamp(0.0, 2.0);
                if (clouds.curl_strength - new_value).abs() > f32::EPSILON {
                    clouds.curl_strength = new_value;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_jitter_strength.clamp(0.0, 2.0);
                if (clouds.jitter_strength - new_value).abs() > f32::EPSILON {
                    clouds.jitter_strength = new_value;
                    cloud_dirty = true;
                }
                let new_value = self.slider_values.cloud_epsilon.clamp(0.0001, 0.1);
                if (clouds.epsilon - new_value).abs() > f32::EPSILON {
                    clouds.epsilon = new_value;
                    cloud_dirty = true;
                }
                let new_radiance = Vec3::new(
                    self.slider_values.cloud_sun_radiance_r.clamp(0.0, 10.0),
                    self.slider_values.cloud_sun_radiance_g.clamp(0.0, 10.0),
                    self.slider_values.cloud_sun_radiance_b.clamp(0.0, 10.0),
                );
                if (clouds.sun_radiance - new_radiance).length_squared() > f32::EPSILON {
                    clouds.sun_radiance = new_radiance;
                    cloud_dirty = true;
                }
                let new_direction = Vec3::new(
                    self.slider_values.cloud_sun_direction_x.clamp(-1.0, 1.0),
                    self.slider_values.cloud_sun_direction_y.clamp(-1.0, 1.0),
                    self.slider_values.cloud_sun_direction_z.clamp(-1.0, 1.0),
                );
                if (clouds.sun_direction - new_direction).length_squared() > f32::EPSILON {
                    clouds.sun_direction = new_direction;
                    cloud_dirty = true;
                }
                let new_shadow_enabled = self.slider_values.cloud_shadow_enabled >= 0.5;
                if clouds.shadow.enabled != new_shadow_enabled {
                    clouds.shadow.enabled = new_shadow_enabled;
                    cloud_dirty = true;
                }
                let new_shadow_resolution =
                    self.slider_values.cloud_shadow_resolution.clamp(64.0, 2048.0).round() as u32;
                if clouds.shadow.resolution != new_shadow_resolution {
                    clouds.shadow.resolution = new_shadow_resolution;
                    cloud_dirty = true;
                }
                let new_shadow_extent = self.slider_values.cloud_shadow_extent.clamp(1000.0, 200000.0);
                if (clouds.shadow.extent - new_shadow_extent).abs() > f32::EPSILON {
                    clouds.shadow.extent = new_shadow_extent;
                    cloud_dirty = true;
                }
                let new_shadow_strength = self.slider_values.cloud_shadow_strength.clamp(0.0, 2.0);
                if (clouds.shadow.strength - new_shadow_strength).abs() > f32::EPSILON {
                    clouds.shadow.strength = new_shadow_strength;
                    cloud_dirty = true;
                }
                let new_cascade_count =
                    self.slider_values.cloud_shadow_cascade_count.clamp(1.0, 4.0).round() as u32;
                if clouds.shadow.cascades.cascade_count != new_cascade_count {
                    clouds.shadow.cascades.cascade_count = new_cascade_count;
                    cloud_dirty = true;
                }
                let new_split_lambda =
                    self.slider_values.cloud_shadow_split_lambda.clamp(0.0, 1.0);
                if (clouds.shadow.cascades.split_lambda - new_split_lambda).abs() > f32::EPSILON {
                    clouds.shadow.cascades.split_lambda = new_split_lambda;
                    cloud_dirty = true;
                }
                let new_temporal_blend = self.slider_values.cloud_temporal_blend_factor.clamp(0.0, 1.0);
                if (clouds.temporal.blend_factor - new_temporal_blend).abs() > f32::EPSILON {
                    clouds.temporal.blend_factor = new_temporal_blend;
                    cloud_dirty = true;
                }
                let new_temporal_clamp = self.slider_values.cloud_temporal_clamp_strength.clamp(0.0, 1.0);
                if (clouds.temporal.clamp_strength - new_temporal_clamp).abs() > f32::EPSILON {
                    clouds.temporal.clamp_strength = new_temporal_clamp;
                    cloud_dirty = true;
                }
                let new_temporal_sigma = self.slider_values.cloud_temporal_depth_sigma.clamp(0.1, 100.0);
                if (clouds.temporal.depth_sigma - new_temporal_sigma).abs() > f32::EPSILON {
                    clouds.temporal.depth_sigma = new_temporal_sigma;
                    cloud_dirty = true;
                }
                let new_temporal_history =
                    self.slider_values.cloud_temporal_history_weight_scale.clamp(0.0, 4.0);
                if (clouds.temporal.history_weight_scale - new_temporal_history).abs()
                    > f32::EPSILON
                {
                    clouds.temporal.history_weight_scale = new_temporal_history;
                    cloud_dirty = true;
                }
                let new_view = cloud_debug_view_from_value(self.slider_values.cloud_debug_view);
                if clouds.debug_view != new_view {
                    clouds.debug_view = new_view;
                    cloud_dirty = true;
                }
                let new_budget = self.slider_values.cloud_performance_budget_ms.clamp(0.1, 20.0);
                if (clouds.performance_budget_ms - new_budget).abs() > f32::EPSILON {
                    clouds.performance_budget_ms = new_budget;
                    cloud_dirty = true;
                }
            }
        }

        let mut gui = GuiContext::new();
        let panel_brightness = (self.slider_values.skybox_intensity / 1.0).clamp(0.5, 1.4);
        gui.submit_draw(GuiDraw::new(
            GuiLayer::Overlay,
            None,
            quad_from_pixels(
                debug_panel_position,
                debug_panel_size,
                Vec4::new(0.08, 0.1, 0.14, 0.88) * panel_brightness,
                viewport,
            ),
        ));
        gui.submit_draw(GuiDraw::new(
            GuiLayer::Overlay,
            None,
            quad_from_pixels(
                debug_title_bar_pos,
                debug_title_bar_size,
                Vec4::new(0.18, 0.22, 0.3, 0.95),
                viewport,
            ),
        ));
        gui.submit_text(GuiTextDraw {
            text: "Debug".to_string(),
            position: [debug_title_bar_pos.x + 12.0, debug_title_bar_pos.y + 6.0],
            color: Vec4::new(0.92, 0.95, 1.0, 1.0).to_array(),
            scale: 0.95,
        });

        let tab_height = 26.0 * ui_scale;
        let tab_padding = 12.0 * ui_scale;
        let tab_gap = 6.0 * ui_scale;
        let tab_width = (debug_panel_size.x - tab_padding * 2.0) / 3.0;
        let tab_y = debug_title_bar_pos.y + debug_title_bar_height + 6.0 * ui_scale;
        let tab_x = debug_panel_position.x + tab_padding;
        let tabs = [
            (DebugTab::Graphics, "Graphics"),
            (DebugTab::Physics, "Physics"),
            (DebugTab::Audio, "Audio"),
        ];
        for (index, (tab, label)) in tabs.iter().enumerate() {
            let tab_pos = vec2(tab_x + tab_width * index as f32, tab_y);
            let tab_size = vec2(tab_width - tab_gap, tab_height);
            let selected = self.debug_tab == *tab;
            let tab_color = if selected {
                Vec4::new(0.22, 0.28, 0.38, 0.96)
            } else {
                Vec4::new(0.12, 0.16, 0.22, 0.9)
            };
            gui.submit_draw(GuiDraw::new(
                GuiLayer::Overlay,
                None,
                quad_from_pixels(tab_pos, tab_size, tab_color, viewport),
            ));
            gui.submit_text(GuiTextDraw {
                text: (*label).to_string(),
                position: [tab_pos.x + 10.0 * ui_scale, tab_pos.y + 6.0 * ui_scale],
                color: Vec4::new(0.9, 0.93, 0.98, 1.0).to_array(),
                scale: 0.85,
            });
        }

        let subtab_height = 22.0 * ui_scale;
        let mut text_start = vec2(
            debug_panel_position.x + 16.0 * ui_scale,
            tab_y + tab_height + 12.0 * ui_scale,
        );
        if self.debug_tab == DebugTab::Graphics {
            let subtab_padding = 12.0 * ui_scale;
            let subtab_gap = 6.0 * ui_scale;
            let subtab_width = (debug_panel_size.x - subtab_padding * 2.0) / 3.0;
            let subtab_y = tab_y + tab_height + 6.0 * ui_scale;
            let subtab_x = debug_panel_position.x + subtab_padding;
            let subtabs = [
                (DebugGraphicsTab::Sky, "Sky"),
                (DebugGraphicsTab::Ocean, "Ocean"),
                (DebugGraphicsTab::Clouds, "Clouds"),
            ];
            for (index, (tab, label)) in subtabs.iter().enumerate() {
                let tab_pos = vec2(subtab_x + subtab_width * index as f32, subtab_y);
                let tab_size = vec2(subtab_width - subtab_gap, subtab_height);
                let selected = self.debug_graphics_tab == *tab;
                let tab_color = if selected {
                    Vec4::new(0.2, 0.26, 0.34, 0.96)
                } else {
                    Vec4::new(0.1, 0.14, 0.2, 0.9)
                };
                gui.submit_draw(GuiDraw::new(
                    GuiLayer::Overlay,
                    None,
                    quad_from_pixels(tab_pos, tab_size, tab_color, viewport),
                ));
                gui.submit_text(GuiTextDraw {
                    text: (*label).to_string(),
                    position: [tab_pos.x + 10.0 * ui_scale, tab_pos.y + 4.0 * ui_scale],
                    color: Vec4::new(0.9, 0.93, 0.98, 1.0).to_array(),
                    scale: 0.8,
                });
            }
            text_start = vec2(
                debug_panel_position.x + 16.0 * ui_scale,
                subtab_y + subtab_height + 10.0 * ui_scale,
            );
        }

        let avg_ms = average_frame_time_ms;
        let fps_text = avg_ms
            .map(|ms| {
                if ms > 0.0 {
                    format!("{:.1}", 1000.0 / ms)
                } else {
                    "--".to_string()
                }
            })
            .unwrap_or_else(|| "--".to_string());
        let avg_ms_text = avg_ms
            .map(|ms| format!("{ms:.2}"))
            .unwrap_or_else(|| "--".to_string());

        let mut info_lines = vec![
            format!("Renderer: {renderer_label}"),
            format!("Debug mode: {debug_mode}"),
            format!("FPS: {fps_text} ({avg_ms_text} ms avg)"),
            format!("Viewport: {:.0} x {:.0}", viewport.x, viewport.y),
        ];

        if self.debug_tab == DebugTab::Graphics {
            match self.debug_graphics_tab {
                DebugGraphicsTab::Sky => {
                    if let Some(sky) = unsafe { bindings.sky_settings.as_ref() } {
                        info_lines.push(format!("Sky enabled: {}", sky.enabled));
                        if let Some(dir) = sky.sun_direction {
                            info_lines.push(format!(
                                "Sun dir: {:.2}, {:.2}, {:.2}",
                                dir.x, dir.y, dir.z
                            ));
                        }
                        if let Some(dir) = sky.moon_direction {
                            info_lines.push(format!(
                                "Moon dir: {:.2}, {:.2}, {:.2}",
                                dir.x, dir.y, dir.z
                            ));
                        }
                        if let Some(time) = sky.time_of_day {
                            info_lines.push(format!("Time of day: {:.2}h", time));
                        }
                    }
                }
                DebugGraphicsTab::Ocean => {
                    if let Some(ocean) = unsafe { bindings.ocean_settings.as_ref() } {
                        info_lines.push(format!("Ocean enabled: {}", ocean.enabled));
                        info_lines.push(format!(
                            "Wind dir: {:.2}, {:.2}",
                            ocean.wind_dir.x, ocean.wind_dir.y
                        ));
                    }
                }
                DebugGraphicsTab::Clouds => {
                    if let Some(clouds) = unsafe { bindings.cloud_settings.as_ref() } {
                        info_lines.push(format!(
                            "Layer A: {:.0}-{:.0} m",
                            clouds.layer_a.base_altitude, clouds.layer_a.top_altitude
                        ));
                        info_lines.push(format!(
                            "Layer B: {:.0}-{:.0} m",
                            clouds.layer_b.base_altitude, clouds.layer_b.top_altitude
                        ));
                        info_lines.push(format!("Step count: {}", clouds.step_count));
                        info_lines.push(format!(
                            "Debug view: {}",
                            cloud_debug_view_label(clouds.debug_view)
                        ));
                    }
                }
            }
        }

        let line_height = 18.0 * ui_scale;
        for (index, line) in info_lines.iter().enumerate() {
            gui.submit_text(GuiTextDraw {
                text: line.clone(),
                position: [text_start.x, text_start.y + index as f32 * line_height],
                color: Vec4::new(0.75, 0.8, 0.9, 1.0).to_array(),
                scale: if index == 0 { 0.9 } else { 0.85 },
            });
        }
        let slider_start_y =
            text_start.y + info_lines.len() as f32 * line_height + 8.0 * ui_scale;

        let mut debug_slider_layout = SliderLayout::default();
        if self.debug_tab == DebugTab::Graphics {
            let slider_area_height = (debug_panel_size.y
                - (slider_start_y - debug_panel_position.y)
                - 12.0 * ui_scale
                - debug_taskbar_height)
                .max(0.0);
            let debug_slider_options = SliderRenderOptions {
                viewport: [viewport.x, viewport.y],
                position: [debug_panel_position.x, slider_start_y],
                size: [
                    debug_panel_size.x,
                    slider_area_height,
                ],
                layer: GuiLayer::Overlay,
                metrics: SliderMetrics {
                    item_height: (26.0 * ui_scale).clamp(22.0, 32.0),
                    item_gap: (8.0 * ui_scale).clamp(4.0, 12.0),
                    ..SliderMetrics::default()
                },
                colors: SliderColors::default(),
                state: self.debug_slider_state,
            };

            let debug_sliders = match self.debug_graphics_tab {
                DebugGraphicsTab::Sky => vec![
                    Slider::new(
                        101,
                        "Skybox Intensity",
                        0.2,
                        2.0,
                        self.slider_values.skybox_intensity,
                    ),
                    Slider::new(
                        102,
                        "Sun Intensity",
                        0.1,
                        5.0,
                        self.slider_values.sun_intensity,
                    ),
                    Slider::new(
                        103,
                        "Sun Angular Radius",
                        0.001,
                        0.05,
                        self.slider_values.sun_angular_radius,
                    ),
                    Slider::new(
                        104,
                        "Moon Intensity",
                        0.0,
                        2.0,
                        self.slider_values.moon_intensity,
                    ),
                    Slider::new(
                        105,
                        "Moon Angular Radius",
                        0.001,
                        0.05,
                        self.slider_values.moon_angular_radius,
                    ),
                ],
                DebugGraphicsTab::Ocean => vec![
                    Slider::new(
                        201,
                        "Wind Speed",
                        0.1,
                        20.0,
                        self.slider_values.ocean_wind_speed,
                    ),
                    Slider::new(
                        202,
                        "Wave Amplitude",
                        0.1,
                        10.0,
                        self.slider_values.ocean_wave_amplitude,
                    ),
                    Slider::new(
                        203,
                        "Gerstner Amplitude",
                        0.0,
                        1.0,
                        self.slider_values.ocean_gerstner_amplitude,
                    ),
                    Slider::new(
                        204,
                        "Fresnel Bias",
                        0.0,
                        0.2,
                        self.slider_values.ocean_fresnel_bias,
                    ),
                    Slider::new(
                        205,
                        "Fresnel Strength",
                        0.0,
                        1.5,
                        self.slider_values.ocean_fresnel_strength,
                    ),
                    Slider::new(
                        206,
                        "Foam Strength",
                        0.0,
                        4.0,
                        self.slider_values.ocean_foam_strength,
                    ),
                    Slider::new(
                        207,
                        "Foam Threshold",
                        0.0,
                        1.0,
                        self.slider_values.ocean_foam_threshold,
                    ),
                    Slider::new(
                        208,
                        "Capillary Strength",
                        0.0,
                        2.0,
                        self.slider_values.ocean_capillary_strength,
                    ),
                    Slider::new(
                        209,
                        "Time Scale",
                        0.1,
                        4.0,
                        self.slider_values.ocean_time_scale,
                    ),
                ],
                DebugGraphicsTab::Clouds => vec![
                    Slider::new(
                        300,
                        "Enabled",
                        0.0,
                        1.0,
                        self.slider_values.cloud_enabled,
                    ),
                    Slider::new(
                        301,
                        "Layer A Base Alt",
                        0.0,
                        3000.0,
                        self.slider_values.cloud_layer_a_base_altitude,
                    ),
                    Slider::new(
                        302,
                        "Layer A Top Alt",
                        100.0,
                        6000.0,
                        self.slider_values.cloud_layer_a_top_altitude,
                    ),
                    Slider::new(
                        303,
                        "Layer A Density",
                        0.0,
                        2.0,
                        self.slider_values.cloud_layer_a_density_scale,
                    ),
                    Slider::new(
                        304,
                        "Layer A Noise Scale",
                        0.1,
                        2.0,
                        self.slider_values.cloud_layer_a_noise_scale,
                    ),
                    Slider::new(
                        305,
                        "Layer A Wind X",
                        -5.0,
                        5.0,
                        self.slider_values.cloud_layer_a_wind_x,
                    ),
                    Slider::new(
                        306,
                        "Layer A Wind Y",
                        -5.0,
                        5.0,
                        self.slider_values.cloud_layer_a_wind_y,
                    ),
                    Slider::new(
                        307,
                        "Layer A Wind Speed",
                        0.0,
                        5.0,
                        self.slider_values.cloud_layer_a_wind_speed,
                    ),
                    Slider::new(
                        308,
                        "Layer B Base Alt",
                        0.0,
                        12000.0,
                        self.slider_values.cloud_layer_b_base_altitude,
                    ),
                    Slider::new(
                        309,
                        "Layer B Top Alt",
                        100.0,
                        20000.0,
                        self.slider_values.cloud_layer_b_top_altitude,
                    ),
                    Slider::new(
                        310,
                        "Layer B Density",
                        0.0,
                        2.0,
                        self.slider_values.cloud_layer_b_density_scale,
                    ),
                    Slider::new(
                        311,
                        "Layer B Noise Scale",
                        0.1,
                        2.0,
                        self.slider_values.cloud_layer_b_noise_scale,
                    ),
                    Slider::new(
                        312,
                        "Layer B Wind X",
                        -5.0,
                        5.0,
                        self.slider_values.cloud_layer_b_wind_x,
                    ),
                    Slider::new(
                        313,
                        "Layer B Wind Y",
                        -5.0,
                        5.0,
                        self.slider_values.cloud_layer_b_wind_y,
                    ),
                    Slider::new(
                        314,
                        "Layer B Wind Speed",
                        0.0,
                        5.0,
                        self.slider_values.cloud_layer_b_wind_speed,
                    ),
                    Slider::new(
                        315,
                        "Step Count",
                        8.0,
                        256.0,
                        self.slider_values.cloud_step_count,
                    ),
                    Slider::new(
                        316,
                        "Light Step Count",
                        4.0,
                        128.0,
                        self.slider_values.cloud_light_step_count,
                    ),
                    Slider::new(
                        317,
                        "Phase G",
                        -0.2,
                        0.9,
                        self.slider_values.cloud_phase_g,
                    ),
                    Slider::new(
                        318,
                        "Low Res Scale",
                        0.0,
                        1.0,
                        self.slider_values.cloud_low_res_scale,
                    ),
                    Slider::new(
                        319,
                        "Coverage Power",
                        0.1,
                        4.0,
                        self.slider_values.cloud_coverage_power,
                    ),
                    Slider::new(
                        320,
                        "Detail Strength",
                        0.0,
                        2.0,
                        self.slider_values.cloud_detail_strength,
                    ),
                    Slider::new(
                        321,
                        "Curl Strength",
                        0.0,
                        2.0,
                        self.slider_values.cloud_curl_strength,
                    ),
                    Slider::new(
                        322,
                        "Jitter Strength",
                        0.0,
                        2.0,
                        self.slider_values.cloud_jitter_strength,
                    ),
                    Slider::new(
                        323,
                        "Epsilon",
                        0.0001,
                        0.1,
                        self.slider_values.cloud_epsilon,
                    ),
                    Slider::new(
                        324,
                        "Sun Radiance R",
                        0.0,
                        10.0,
                        self.slider_values.cloud_sun_radiance_r,
                    ),
                    Slider::new(
                        325,
                        "Sun Radiance G",
                        0.0,
                        10.0,
                        self.slider_values.cloud_sun_radiance_g,
                    ),
                    Slider::new(
                        326,
                        "Sun Radiance B",
                        0.0,
                        10.0,
                        self.slider_values.cloud_sun_radiance_b,
                    ),
                    Slider::new(
                        327,
                        "Sun Dir X",
                        -1.0,
                        1.0,
                        self.slider_values.cloud_sun_direction_x,
                    ),
                    Slider::new(
                        328,
                        "Sun Dir Y",
                        -1.0,
                        1.0,
                        self.slider_values.cloud_sun_direction_y,
                    ),
                    Slider::new(
                        329,
                        "Sun Dir Z",
                        -1.0,
                        1.0,
                        self.slider_values.cloud_sun_direction_z,
                    ),
                    Slider::new(
                        330,
                        "Shadow Enabled",
                        0.0,
                        1.0,
                        self.slider_values.cloud_shadow_enabled,
                    ),
                    Slider::new(
                        331,
                        "Shadow Resolution",
                        64.0,
                        2048.0,
                        self.slider_values.cloud_shadow_resolution,
                    ),
                    Slider::new(
                        332,
                        "Shadow Extent",
                        1000.0,
                        200000.0,
                        self.slider_values.cloud_shadow_extent,
                    ),
                    Slider::new(
                        333,
                        "Shadow Strength",
                        0.0,
                        2.0,
                        self.slider_values.cloud_shadow_strength,
                    ),
                    Slider::new(
                        334,
                        "Shadow Cascades",
                        1.0,
                        4.0,
                        self.slider_values.cloud_shadow_cascade_count,
                    ),
                    Slider::new(
                        335,
                        "Shadow Split Lambda",
                        0.0,
                        1.0,
                        self.slider_values.cloud_shadow_split_lambda,
                    ),
                    Slider::new(
                        336,
                        "Temporal Blend",
                        0.0,
                        1.0,
                        self.slider_values.cloud_temporal_blend_factor,
                    ),
                    Slider::new(
                        337,
                        "Temporal Clamp",
                        0.0,
                        1.0,
                        self.slider_values.cloud_temporal_clamp_strength,
                    ),
                    Slider::new(
                        338,
                        "Temporal Depth Sigma",
                        0.1,
                        100.0,
                        self.slider_values.cloud_temporal_depth_sigma,
                    ),
                    Slider::new(
                        339,
                        "Temporal History Scale",
                        0.0,
                        4.0,
                        self.slider_values.cloud_temporal_history_weight_scale,
                    ),
                    Slider::new(
                        340,
                        "Debug View",
                        0.0,
                        8.0,
                        self.slider_values.cloud_debug_view,
                    ),
                    Slider::new(
                        341,
                        "Budget (ms)",
                        0.1,
                        20.0,
                        self.slider_values.cloud_performance_budget_ms,
                    ),
                ],
            };
            let metrics = debug_slider_options.metrics;
            let row_height = metrics.item_height + metrics.item_gap;
            let total_items = debug_sliders.len();
            let total_height = metrics.padding[1] * 2.0
                + total_items as f32 * metrics.item_height
                + total_items.saturating_sub(1) as f32 * metrics.item_gap;
            let max_scroll = (total_height - slider_area_height).max(0.0);
            let slider_area_pos = vec2(
                debug_slider_options.position[0],
                debug_slider_options.position[1],
            );
            let slider_area_hovered = point_in_rect(
                self.cursor,
                slider_area_pos,
                vec2(debug_slider_options.size[0], slider_area_height),
            );
            if slider_area_hovered && self.scroll_delta.abs() > 0.0 {
                self.scroll_offset = (self.scroll_offset - self.scroll_delta * 18.0 * ui_scale)
                    .clamp(0.0, max_scroll);
            }
            self.scroll_delta = 0.0;

            let start_index = (self.scroll_offset / row_height).floor() as usize;
            let offset_y = -(self.scroll_offset - start_index as f32 * row_height);
            let visible_count =
                ((slider_area_height - offset_y).max(0.0) / row_height).ceil() as usize + 1;
            let end_index = (start_index + visible_count).min(total_items);
            let visible_sliders = if start_index < end_index {
                &debug_sliders[start_index..end_index]
            } else {
                &debug_sliders[0..0]
            };

            let mut scroll_options = debug_slider_options;
            scroll_options.position[1] += offset_y;
            debug_slider_layout = gui.submit_sliders(visible_sliders, &scroll_options);
        } else {
            gui.submit_text(GuiTextDraw {
                text: "No debug data available.".to_string(),
                position: [text_start.x, slider_start_y],
                color: Vec4::new(0.7, 0.75, 0.85, 1.0).to_array(),
                scale: 0.85,
            });
            self.scroll_delta = 0.0;
        }

        gui.submit_draw(GuiDraw::new(
            GuiLayer::Overlay,
            None,
            quad_from_pixels(
                debug_taskbar_pos,
                debug_taskbar_size,
                Vec4::new(0.12, 0.16, 0.22, 0.95),
                viewport,
            ),
        ));
        gui.submit_text(GuiTextDraw {
            text: "Debug Tools".to_string(),
            position: [
                debug_taskbar_pos.x + 12.0 * ui_scale,
                debug_taskbar_pos.y + 6.0 * ui_scale,
            ],
            color: Vec4::new(0.82, 0.86, 0.92, 1.0).to_array(),
            scale: 0.8,
        });

        let reset_color = if reset_button_hovered {
            Vec4::new(0.22, 0.3, 0.38, 0.96)
        } else {
            Vec4::new(0.16, 0.22, 0.3, 0.95)
        };
        gui.submit_draw(GuiDraw::new(
            GuiLayer::Overlay,
            None,
            quad_from_pixels(reset_button_pos, reset_button_size, reset_color, viewport),
        ));
        gui.submit_text(GuiTextDraw {
            text: "Reset Pos".to_string(),
            position: [
                reset_button_pos.x + 8.0 * ui_scale,
                reset_button_pos.y + 3.0 * ui_scale,
            ],
            color: Vec4::new(0.9, 0.93, 0.98, 1.0).to_array(),
            scale: 0.75,
        });

        let close_color = if close_button_hovered {
            Vec4::new(0.32, 0.22, 0.26, 0.96)
        } else {
            Vec4::new(0.26, 0.18, 0.22, 0.95)
        };
        gui.submit_draw(GuiDraw::new(
            GuiLayer::Overlay,
            None,
            quad_from_pixels(close_button_pos, close_button_size, close_color, viewport),
        ));
        gui.submit_text(GuiTextDraw {
            text: "Close".to_string(),
            position: [
                close_button_pos.x + 14.0 * ui_scale,
                close_button_pos.y + 3.0 * ui_scale,
            ],
            color: Vec4::new(0.95, 0.9, 0.92, 1.0).to_array(),
            scale: 0.75,
        });

        self.debug_slider_layout = debug_slider_layout;
        self.mouse_pressed = false;

        DebugGuiOutput {
            frame: Some(gui.build_frame()),
            skybox_dirty,
            sky_dirty,
            ocean_dirty,
            cloud_dirty,
        }
    }

    fn reset_for_hidden(&mut self) {
        self.mouse_pressed = false;
        self.mouse_down = false;
        self.drag_target = None;
        self.debug_slider_state = SliderState::default();
        self.debug_slider_layout = SliderLayout::default();
        self.scroll_offset = 0.0;
        self.scroll_delta = 0.0;
    }
}

fn cloud_debug_view_from_value(value: f32) -> CloudDebugView {
    match value.round().clamp(0.0, 8.0) as u32 {
        1 => CloudDebugView::WeatherMap,
        2 => CloudDebugView::ShadowMap,
        3 => CloudDebugView::Transmittance,
        4 => CloudDebugView::StepHeatmap,
        5 => CloudDebugView::TemporalWeight,
        6 => CloudDebugView::Stats,
        7 => CloudDebugView::LayerA,
        8 => CloudDebugView::LayerB,
        _ => CloudDebugView::None,
    }
}

fn cloud_debug_view_label(view: CloudDebugView) -> &'static str {
    match view {
        CloudDebugView::None => "None",
        CloudDebugView::WeatherMap => "Weather Map",
        CloudDebugView::ShadowMap => "Shadow Map",
        CloudDebugView::Transmittance => "Transmittance",
        CloudDebugView::StepHeatmap => "Step Heatmap",
        CloudDebugView::TemporalWeight => "Temporal Weight",
        CloudDebugView::Stats => "Stats",
        CloudDebugView::LayerA => "Layer A",
        CloudDebugView::LayerB => "Layer B",
    }
}

fn cloud_resolution_scale_from_value(value: f32) -> CloudResolutionScale {
    if value.round() < 0.5 {
        CloudResolutionScale::Half
    } else {
        CloudResolutionScale::Quarter
    }
}

fn cloud_resolution_scale_value(scale: CloudResolutionScale) -> f32 {
    match scale {
        CloudResolutionScale::Half => 0.0,
        CloudResolutionScale::Quarter => 1.0,
    }
}

fn quad_from_pixels(position: Vec2, size: Vec2, color: Vec4, viewport: Vec2) -> GuiQuad {
    let left = (position.x / viewport.x) * 2.0 - 1.0;
    let right = ((position.x + size.x) / viewport.x) * 2.0 - 1.0;
    let top = 1.0 - (position.y / viewport.y) * 2.0;
    let bottom = 1.0 - ((position.y + size.y) / viewport.y) * 2.0;

    GuiQuad {
        positions: [[left, top], [right, top], [right, bottom], [left, bottom]],
        uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        color: color.to_array(),
    }
}

fn point_in_rect(point: Vec2, position: Vec2, size: Vec2) -> bool {
    point.x >= position.x
        && point.x <= position.x + size.x
        && point.y >= position.y
        && point.y <= position.y + size.y
}

fn point_in_menu_rect(point: Vec2, rect: MenuRect) -> bool {
    point.x >= rect.min[0]
        && point.x <= rect.max[0]
        && point.y >= rect.min[1]
        && point.y <= rect.max[1]
}

fn slider_value_from_cursor(cursor: Vec2, rect: MenuRect, min: f32, max: f32) -> f32 {
    if (max - min).abs() < f32::EPSILON {
        return min;
    }
    let t = ((cursor.x - rect.min[0]) / (rect.max[0] - rect.min[0])).clamp(0.0, 1.0);
    min + (max - min) * t
}
