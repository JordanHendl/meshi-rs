use glam::{Vec2, Vec3, Vec4, vec2};
use meshi_ffi_structs::event::{Event, EventSource, EventType, KeyCode};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::gui::{
    GuiClipRect, GuiContext, GuiDraw, GuiLayer, GuiQuad, GuiTextDraw, MenuRect, RadialButton,
    RadialButtonColors, RadialButtonLayout, RadialButtonMetrics, RadialButtonRenderOptions,
    RadialButtonState, Slider, SliderColors, SliderLayout, SliderMetrics, SliderRenderOptions,
    SliderState, SliderValueFormat,
};
use crate::render::environment::ocean::{OceanDebugView, OceanFrameSettings};
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
pub enum PageType {
    Sky,
    Ocean,
    Clouds,
    Physics,
    Audio,
}

#[derive(Clone)]
pub struct DebugRadialOption {
    pub label: &'static str,
    pub value: f32,
}

#[derive(Clone)]
struct DebugRegistryRadialOption {
    id: u32,
    label: String,
    value: f32,
}

#[derive(Clone)]
enum DebugRegistryControl {
    Slider {
        min: f32,
        max: f32,
        enabled: bool,
        show_value: bool,
        value_format: SliderValueFormat,
    },
    Radial {
        options: Vec<DebugRegistryRadialOption>,
    },
}

#[derive(Clone)]
pub enum DebugRegistryValue {
    Float(*mut f32),
    U32(*mut u32),
    Bool(*mut bool),
    CloudResolutionScale(*mut CloudResolutionScale),
    CloudDebugView(*mut CloudDebugView),
    OceanDebugView(*mut OceanDebugView),
}

impl DebugRegistryValue {
    fn matches(&self, other: &DebugRegistryValue) -> bool {
        match (self, other) {
            (DebugRegistryValue::Float(lhs), DebugRegistryValue::Float(rhs)) => lhs == rhs,
            (DebugRegistryValue::U32(lhs), DebugRegistryValue::U32(rhs)) => lhs == rhs,
            (DebugRegistryValue::Bool(lhs), DebugRegistryValue::Bool(rhs)) => lhs == rhs,
            (
                DebugRegistryValue::CloudResolutionScale(lhs),
                DebugRegistryValue::CloudResolutionScale(rhs),
            ) => lhs == rhs,
            (DebugRegistryValue::CloudDebugView(lhs), DebugRegistryValue::CloudDebugView(rhs)) => {
                lhs == rhs
            }
            (DebugRegistryValue::OceanDebugView(lhs), DebugRegistryValue::OceanDebugView(rhs)) => {
                lhs == rhs
            }
            _ => false,
        }
    }

    unsafe fn get(&self) -> f32 {
        match self {
            DebugRegistryValue::Float(ptr) => ptr.as_ref().copied().unwrap_or(0.0),
            DebugRegistryValue::U32(ptr) => ptr.as_ref().copied().unwrap_or(0) as f32,
            DebugRegistryValue::Bool(ptr) => ptr.as_ref().copied().unwrap_or(false) as u32 as f32,
            DebugRegistryValue::CloudResolutionScale(ptr) => ptr
                .as_ref()
                .map(|value| cloud_resolution_scale_value(*value))
                .unwrap_or(0.0),
            DebugRegistryValue::CloudDebugView(ptr) => ptr
                .as_ref()
                .map(|value| *value as u32 as f32)
                .unwrap_or(0.0),
            DebugRegistryValue::OceanDebugView(ptr) => ptr
                .as_ref()
                .map(|value| *value as u32 as f32)
                .unwrap_or(0.0),
        }
    }

    unsafe fn set(&self, value: f32) {
        match self {
            DebugRegistryValue::Float(ptr) => {
                if let Some(target) = ptr.as_mut() {
                    *target = value;
                }
            }
            DebugRegistryValue::U32(ptr) => {
                if let Some(target) = ptr.as_mut() {
                    *target = value.round().max(0.0) as u32;
                }
            }
            DebugRegistryValue::Bool(ptr) => {
                if let Some(target) = ptr.as_mut() {
                    *target = value >= 0.5;
                }
            }
            DebugRegistryValue::CloudResolutionScale(ptr) => {
                if let Some(target) = ptr.as_mut() {
                    *target = cloud_resolution_scale_from_value(value);
                }
            }
            DebugRegistryValue::CloudDebugView(ptr) => {
                if let Some(target) = ptr.as_mut() {
                    *target = cloud_debug_view_from_value(value);
                }
            }
            DebugRegistryValue::OceanDebugView(ptr) => {
                if let Some(target) = ptr.as_mut() {
                    *target = ocean_debug_view_from_value(value);
                }
            }
        }
    }
}

#[derive(Clone)]
struct DebugRegistryItem {
    id: u32,
    page: PageType,
    label: String,
    control: DebugRegistryControl,
    value: DebugRegistryValue,
}

unsafe impl Send for DebugRegistryItem {}
static DEBUG_REGISTRY: OnceLock<Mutex<Vec<DebugRegistryItem>>> = OnceLock::new();
static DEBUG_REGISTRY_NEXT_ID: AtomicU32 = AtomicU32::new(10_000);

pub unsafe fn debug_register(
    page: PageType,
    slider: Slider,
    value_ptr: *mut f32,
    label: &str,
) -> u32 {
    debug_register_slider(page, slider, DebugRegistryValue::Float(value_ptr), label)
}

pub unsafe fn debug_register_int(
    page: PageType,
    mut slider: Slider,
    value_ptr: *mut u32,
    label: &str,
) -> u32 {
    slider.value_format = SliderValueFormat::Integer;
    debug_register_slider(page, slider, DebugRegistryValue::U32(value_ptr), label)
}

pub unsafe fn debug_register_slider(
    page: PageType,
    slider: Slider,
    value: DebugRegistryValue,
    label: &str,
) -> u32 {
    let registry = DEBUG_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let mut registry = registry.lock().expect("debug registry poisoned");
    if let Some(entry) = registry
        .iter_mut()
        .find(|entry| entry.page == page && entry.value.matches(&value) && entry.label == label)
    {
        entry.control = DebugRegistryControl::Slider {
            min: slider.min,
            max: slider.max,
            enabled: slider.enabled,
            show_value: slider.show_value,
            value_format: slider.value_format,
        };
        return entry.id;
    }
    let id = DEBUG_REGISTRY_NEXT_ID.fetch_add(1, Ordering::Relaxed);
    registry.push(DebugRegistryItem {
        id,
        page,
        label: label.to_string(),
        control: DebugRegistryControl::Slider {
            min: slider.min,
            max: slider.max,
            enabled: slider.enabled,
            show_value: slider.show_value,
            value_format: slider.value_format,
        },
        value,
    });
    id
}

pub unsafe fn debug_register_radial(
    page: PageType,
    label: &str,
    value: DebugRegistryValue,
    options: &[DebugRadialOption],
) -> u32 {
    let registry = DEBUG_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let mut registry = registry.lock().expect("debug registry poisoned");
    if let Some(entry) = registry
        .iter_mut()
        .find(|entry| entry.page == page && entry.value.matches(&value) && entry.label == label)
    {
        let DebugRegistryControl::Radial { options: existing } = &mut entry.control else {
            entry.control = DebugRegistryControl::Radial {
                options: Vec::new(),
            };
            if let DebugRegistryControl::Radial { options: existing } = &mut entry.control {
                *existing = build_radial_options(options);
            }
            return entry.id;
        };

        if existing.len() == options.len() {
            for (stored, incoming) in existing.iter_mut().zip(options.iter()) {
                stored.label = incoming.label.to_string();
                stored.value = incoming.value;
            }
        } else {
            *existing = build_radial_options(options);
        }
        return entry.id;
    }

    let base_id = DEBUG_REGISTRY_NEXT_ID.fetch_add(1 + options.len() as u32, Ordering::Relaxed);
    let radial_options = options
        .iter()
        .enumerate()
        .map(|(index, option)| DebugRegistryRadialOption {
            id: base_id + 1 + index as u32,
            label: option.label.to_string(),
            value: option.value,
        })
        .collect();
    registry.push(DebugRegistryItem {
        id: base_id,
        page,
        label: label.to_string(),
        control: DebugRegistryControl::Radial {
            options: radial_options,
        },
        value,
    });
    base_id
}

fn build_radial_options(options: &[DebugRadialOption]) -> Vec<DebugRegistryRadialOption> {
    let base_id = DEBUG_REGISTRY_NEXT_ID.fetch_add(1 + options.len() as u32, Ordering::Relaxed);
    options
        .iter()
        .enumerate()
        .map(|(index, option)| DebugRegistryRadialOption {
            id: base_id + 1 + index as u32,
            label: option.label.to_string(),
            value: option.value,
        })
        .collect()
}

unsafe fn debug_registry_sliders(page: PageType) -> Vec<Slider> {
    let registry = DEBUG_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let registry = registry.lock().expect("debug registry poisoned");
    registry
        .iter()
        .filter(|entry| entry.page == page)
        .filter_map(|entry| match &entry.control {
            DebugRegistryControl::Slider {
                min,
                max,
                enabled,
                show_value,
                value_format,
            } => Some(Slider {
                id: entry.id,
                label: entry.label.clone(),
                value: unsafe { entry.value.get() },
                min: *min,
                max: *max,
                enabled: *enabled,
                show_value: *show_value,
                value_format: *value_format,
            }),
            DebugRegistryControl::Radial { .. } => None,
        })
        .collect()
}

#[derive(Clone)]
struct DebugRegistryRadialGroup {
    label: String,
    options: Vec<DebugRegistryRadialOption>,
    value: f32,
}

unsafe fn debug_registry_radials(page: PageType) -> Vec<DebugRegistryRadialGroup> {
    let registry = DEBUG_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let registry = registry.lock().expect("debug registry poisoned");
    registry
        .iter()
        .filter(|entry| entry.page == page)
        .filter_map(|entry| match &entry.control {
            DebugRegistryControl::Radial { options } => Some(DebugRegistryRadialGroup {
                label: entry.label.clone(),
                options: options.clone(),
                value: unsafe { entry.value.get() },
            }),
            DebugRegistryControl::Slider { .. } => None,
        })
        .collect()
}

unsafe fn debug_registry_update_value(id: u32, value: f32) -> bool {
    let registry = DEBUG_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let mut registry = registry.lock().expect("debug registry poisoned");
    if let Some(entry) = registry.iter_mut().find(|entry| entry.id == id) {
        let (min, max) = match &entry.control {
            DebugRegistryControl::Slider { min, max, .. } => (*min, *max),
            DebugRegistryControl::Radial { .. } => return false,
        };
        let clamped = value.clamp(min, max);
        unsafe {
            entry.value.set(clamped);
        }
        return true;
    }
    false
}

unsafe fn debug_registry_update_radial(id: u32) -> bool {
    let registry = DEBUG_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let mut registry = registry.lock().expect("debug registry poisoned");
    for entry in registry.iter_mut() {
        let DebugRegistryControl::Radial { options } = &entry.control else {
            continue;
        };
        if let Some(option) = options.iter().find(|option| option.id == id) {
            unsafe {
                entry.value.set(option.value);
            }
            return true;
        }
    }
    false
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
    ocean_fetch_length: f32,
    ocean_swell_dir_x: f32,
    ocean_swell_dir_y: f32,
    ocean_current_x: f32,
    ocean_current_y: f32,
    ocean_wave_amplitude: f32,
    ocean_gerstner_amplitude: f32,
    ocean_cascade_spectrum_near: f32,
    ocean_cascade_spectrum_mid: f32,
    ocean_cascade_spectrum_far: f32,
    ocean_cascade_swell_near: f32,
    ocean_cascade_swell_mid: f32,
    ocean_cascade_swell_far: f32,
    ocean_depth_meters: f32,
    ocean_depth_damping: f32,
    ocean_fresnel_bias: f32,
    ocean_fresnel_strength: f32,
    ocean_foam_strength: f32,
    ocean_foam_threshold: f32,
    ocean_foam_advection: f32,
    ocean_foam_decay: f32,
    ocean_foam_noise_scale: f32,
    ocean_capillary_strength: f32,
    ocean_time_scale: f32,
}

#[derive(Clone, Copy, Debug)]
struct DebugToggleValues {
    sky_enabled: bool,
    ocean_enabled: bool,
    cloud_enabled: bool,
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
    debug_toggle_state: RadialButtonState,
    debug_toggle_layout: RadialButtonLayout,
    debug_param_radial_state: RadialButtonState,
    debug_param_radial_layouts: Vec<RadialButtonLayout>,
    debug_panel_position: Vec2,
    drag_target: Option<DragTarget>,
    drag_offset: Vec2,
    scroll_offset: f32,
    scroll_delta: f32,
    slider_values: DebugSliderValues,
    toggle_values: DebugToggleValues,
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
            debug_toggle_state: RadialButtonState::default(),
            debug_toggle_layout: RadialButtonLayout::default(),
            debug_param_radial_state: RadialButtonState::default(),
            debug_param_radial_layouts: Vec::new(),
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
                ocean_fetch_length: 5000.0,
                ocean_swell_dir_x: 0.8,
                ocean_swell_dir_y: 0.1,
                ocean_current_x: 0.0,
                ocean_current_y: 0.0,
                ocean_wave_amplitude: 2.0,
                ocean_gerstner_amplitude: 0.12,
                ocean_cascade_spectrum_near: 1.0,
                ocean_cascade_spectrum_mid: 0.85,
                ocean_cascade_spectrum_far: 0.65,
                ocean_cascade_swell_near: 0.35,
                ocean_cascade_swell_mid: 0.55,
                ocean_cascade_swell_far: 0.75,
                ocean_depth_meters: 200.0,
                ocean_depth_damping: 0.3,
                ocean_fresnel_bias: 0.02,
                ocean_fresnel_strength: 0.85,
                ocean_foam_strength: 1.0,
                ocean_foam_threshold: 0.55,
                ocean_foam_advection: 0.25,
                ocean_foam_decay: 0.08,
                ocean_foam_noise_scale: 0.2,
                ocean_capillary_strength: 1.0,
                ocean_time_scale: 1.0,
            },
            toggle_values: DebugToggleValues {
                sky_enabled: false,
                ocean_enabled: false,
                cloud_enabled: true,
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
                    self.slider_values.ocean_fetch_length = ocean.fetch_length;
                    self.slider_values.ocean_swell_dir_x = ocean.swell_dir.x;
                    self.slider_values.ocean_swell_dir_y = ocean.swell_dir.y;
                    self.slider_values.ocean_current_x = ocean.current.x;
                    self.slider_values.ocean_current_y = ocean.current.y;
                    self.slider_values.ocean_wave_amplitude = ocean.wave_amplitude;
                    self.slider_values.ocean_gerstner_amplitude = ocean.gerstner_amplitude;
                    self.slider_values.ocean_cascade_spectrum_near =
                        ocean.cascade_spectrum_scales[0];
                    self.slider_values.ocean_cascade_spectrum_mid =
                        ocean.cascade_spectrum_scales[1];
                    self.slider_values.ocean_cascade_spectrum_far =
                        ocean.cascade_spectrum_scales[2];
                    self.slider_values.ocean_cascade_swell_near = ocean.cascade_swell_strengths[0];
                    self.slider_values.ocean_cascade_swell_mid = ocean.cascade_swell_strengths[1];
                    self.slider_values.ocean_cascade_swell_far = ocean.cascade_swell_strengths[2];
                    self.slider_values.ocean_depth_meters = ocean.depth_meters;
                    self.slider_values.ocean_depth_damping = ocean.depth_damping;
                    self.slider_values.ocean_fresnel_bias = ocean.fresnel_bias;
                    self.slider_values.ocean_fresnel_strength = ocean.fresnel_strength;
                    self.slider_values.ocean_foam_strength = ocean.foam_strength;
                    self.slider_values.ocean_foam_threshold = ocean.foam_threshold;
                    self.slider_values.ocean_foam_advection = ocean.foam_advection_strength;
                    self.slider_values.ocean_foam_decay = ocean.foam_decay_rate;
                    self.slider_values.ocean_foam_noise_scale = ocean.foam_noise_scale;
                    self.slider_values.ocean_capillary_strength = ocean.capillary_strength;
                    self.slider_values.ocean_time_scale = ocean.time_scale;
                }
            }
        }

        if self.debug_toggle_state.active.is_none() {
            unsafe {
                if let Some(sky) = bindings.sky_settings.as_ref() {
                    self.toggle_values.sky_enabled = sky.enabled;
                }
                if let Some(ocean) = bindings.ocean_settings.as_ref() {
                    self.toggle_values.ocean_enabled = ocean.enabled;
                }
                if let Some(clouds) = bindings.cloud_settings.as_ref() {
                    self.toggle_values.cloud_enabled = clouds.enabled;
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
        self.debug_panel_position = self
            .debug_panel_position
            .clamp(panel_min_pos, panel_max_pos);
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
        let close_button_hovered = point_in_rect(self.cursor, close_button_pos, close_button_size);
        let reset_button_hovered = point_in_rect(self.cursor, reset_button_pos, reset_button_size);
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
                    self.debug_toggle_state = RadialButtonState::default();
                    self.debug_toggle_layout = RadialButtonLayout::default();
                    self.debug_param_radial_state = RadialButtonState::default();
                    self.debug_param_radial_layouts.clear();
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
                    self.debug_toggle_state = RadialButtonState::default();
                    self.debug_toggle_layout = RadialButtonLayout::default();
                    self.debug_param_radial_state = RadialButtonState::default();
                    self.debug_param_radial_layouts.clear();
                    self.scroll_offset = 0.0;
                }
            }
        }

        let hovered_debug_slider = self.debug_slider_layout.items.iter().find(|item| {
            item.enabled
                && (point_in_menu_rect(self.cursor, item.track_rect)
                    || point_in_menu_rect(self.cursor, item.knob_rect))
        });
        self.debug_slider_state.hovered = hovered_debug_slider.map(|item| item.id);

        let hovered_toggle = self.debug_toggle_layout.items.iter().find(|item| {
            item.enabled
                && (point_in_menu_rect(self.cursor, item.item_rect)
                    || point_in_menu_rect(self.cursor, item.button_rect))
        });
        self.debug_toggle_state.hovered = hovered_toggle.map(|item| item.id);

        let hovered_param_radial = self
            .debug_param_radial_layouts
            .iter()
            .flat_map(|layout| layout.items.iter())
            .find(|item| {
                item.enabled
                    && (point_in_menu_rect(self.cursor, item.item_rect)
                        || point_in_menu_rect(self.cursor, item.button_rect))
            });
        self.debug_param_radial_state.hovered = hovered_param_radial.map(|item| item.id);

        if self.mouse_pressed {
            if let Some(item) = hovered_debug_slider {
                self.debug_slider_state.active = Some(item.id);
            }
            if let Some(item) = hovered_toggle {
                self.debug_toggle_state.active = Some(item.id);
                match item.id {
                    6101 => self.toggle_values.sky_enabled = true,
                    6102 => self.toggle_values.sky_enabled = false,
                    6201 => self.toggle_values.ocean_enabled = true,
                    6202 => self.toggle_values.ocean_enabled = false,
                    6301 => self.toggle_values.cloud_enabled = true,
                    6302 => self.toggle_values.cloud_enabled = false,
                    _ => {}
                }
            }
            if let Some(item) = hovered_param_radial {
                self.debug_param_radial_state.active = Some(item.id);
                unsafe {
                    debug_registry_update_radial(item.id);
                }
            }
        }

        if !self.mouse_down {
            self.debug_slider_state.active = None;
            self.debug_toggle_state.active = None;
            self.debug_param_radial_state.active = None;
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
                let mut handled = true;
                match active_id {
                    101 => self.slider_values.skybox_intensity = value,
                    102 => self.slider_values.sun_intensity = value,
                    103 => self.slider_values.sun_angular_radius = value,
                    104 => self.slider_values.moon_intensity = value,
                    105 => self.slider_values.moon_angular_radius = value,
                    201 => self.slider_values.ocean_wind_speed = value,
                    202 => self.slider_values.ocean_fetch_length = value,
                    203 => self.slider_values.ocean_swell_dir_x = value,
                    204 => self.slider_values.ocean_swell_dir_y = value,
                    205 => self.slider_values.ocean_current_x = value,
                    206 => self.slider_values.ocean_current_y = value,
                    207 => self.slider_values.ocean_wave_amplitude = value,
                    208 => self.slider_values.ocean_gerstner_amplitude = value,
                    209 => self.slider_values.ocean_cascade_spectrum_near = value,
                    210 => self.slider_values.ocean_cascade_spectrum_mid = value,
                    211 => self.slider_values.ocean_cascade_spectrum_far = value,
                    212 => self.slider_values.ocean_cascade_swell_near = value,
                    213 => self.slider_values.ocean_cascade_swell_mid = value,
                    214 => self.slider_values.ocean_cascade_swell_far = value,
                    215 => self.slider_values.ocean_depth_meters = value,
                    216 => self.slider_values.ocean_depth_damping = value,
                    217 => self.slider_values.ocean_fresnel_bias = value,
                    218 => self.slider_values.ocean_fresnel_strength = value,
                    219 => self.slider_values.ocean_foam_strength = value,
                    220 => self.slider_values.ocean_foam_threshold = value,
                    221 => self.slider_values.ocean_foam_advection = value,
                    222 => self.slider_values.ocean_foam_decay = value,
                    223 => self.slider_values.ocean_foam_noise_scale = value,
                    224 => self.slider_values.ocean_capillary_strength = value,
                    225 => self.slider_values.ocean_time_scale = value,
                    _ => handled = false,
                }
                if !handled {
                    unsafe {
                        debug_registry_update_value(active_id, value);
                    }
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
                if sky.enabled != self.toggle_values.sky_enabled {
                    sky.enabled = self.toggle_values.sky_enabled;
                    sky_dirty = true;
                }
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
                if ocean.enabled != self.toggle_values.ocean_enabled {
                    ocean.enabled = self.toggle_values.ocean_enabled;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_wind_speed.clamp(0.1, 20.0);
                if (ocean.wind_speed - new_value).abs() > f32::EPSILON {
                    ocean.wind_speed = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_fetch_length.clamp(10.0, 200000.0);
                if (ocean.fetch_length - new_value).abs() > f32::EPSILON {
                    ocean.fetch_length = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_swell_dir_x.clamp(-1.0, 1.0);
                if (ocean.swell_dir.x - new_value).abs() > f32::EPSILON {
                    ocean.swell_dir.x = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_swell_dir_y.clamp(-1.0, 1.0);
                if (ocean.swell_dir.y - new_value).abs() > f32::EPSILON {
                    ocean.swell_dir.y = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_current_x.clamp(-5.0, 5.0);
                if (ocean.current.x - new_value).abs() > f32::EPSILON {
                    ocean.current.x = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_current_y.clamp(-5.0, 5.0);
                if (ocean.current.y - new_value).abs() > f32::EPSILON {
                    ocean.current.y = new_value;
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
                let new_value = self
                    .slider_values
                    .ocean_cascade_spectrum_near
                    .clamp(0.0, 2.0);
                if (ocean.cascade_spectrum_scales[0] - new_value).abs() > f32::EPSILON {
                    ocean.cascade_spectrum_scales[0] = new_value;
                    ocean_dirty = true;
                }
                let new_value = self
                    .slider_values
                    .ocean_cascade_spectrum_mid
                    .clamp(0.0, 2.0);
                if (ocean.cascade_spectrum_scales[1] - new_value).abs() > f32::EPSILON {
                    ocean.cascade_spectrum_scales[1] = new_value;
                    ocean_dirty = true;
                }
                let new_value = self
                    .slider_values
                    .ocean_cascade_spectrum_far
                    .clamp(0.0, 2.0);
                if (ocean.cascade_spectrum_scales[2] - new_value).abs() > f32::EPSILON {
                    ocean.cascade_spectrum_scales[2] = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_cascade_swell_near.clamp(0.0, 1.0);
                if (ocean.cascade_swell_strengths[0] - new_value).abs() > f32::EPSILON {
                    ocean.cascade_swell_strengths[0] = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_cascade_swell_mid.clamp(0.0, 1.0);
                if (ocean.cascade_swell_strengths[1] - new_value).abs() > f32::EPSILON {
                    ocean.cascade_swell_strengths[1] = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_cascade_swell_far.clamp(0.0, 1.0);
                if (ocean.cascade_swell_strengths[2] - new_value).abs() > f32::EPSILON {
                    ocean.cascade_swell_strengths[2] = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_depth_meters.clamp(0.0, 5000.0);
                if (ocean.depth_meters - new_value).abs() > f32::EPSILON {
                    ocean.depth_meters = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_depth_damping.clamp(0.0, 1.0);
                if (ocean.depth_damping - new_value).abs() > f32::EPSILON {
                    ocean.depth_damping = new_value;
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
                let new_value = self.slider_values.ocean_foam_advection.clamp(0.0, 2.0);
                if (ocean.foam_advection_strength - new_value).abs() > f32::EPSILON {
                    ocean.foam_advection_strength = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_foam_decay.clamp(0.0, 1.0);
                if (ocean.foam_decay_rate - new_value).abs() > f32::EPSILON {
                    ocean.foam_decay_rate = new_value;
                    ocean_dirty = true;
                }
                let new_value = self.slider_values.ocean_foam_noise_scale.clamp(0.01, 1.0);
                if (ocean.foam_noise_scale - new_value).abs() > f32::EPSILON {
                    ocean.foam_noise_scale = new_value;
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
                if clouds.enabled != self.toggle_values.cloud_enabled {
                    clouds.enabled = self.toggle_values.cloud_enabled;
                    cloud_dirty = true;
                }
            }
        }

        let mut gui = GuiContext::new();
        let panel_brightness = (self.slider_values.skybox_intensity / 1.0).clamp(0.5, 1.4);
        let panel_color = Vec4::new(
            0.08 * panel_brightness,
            0.1 * panel_brightness,
            0.14 * panel_brightness,
            0.88,
        );
        gui.submit_draw(GuiDraw::new(
            GuiLayer::Overlay,
            None,
            quad_from_pixels(
                debug_panel_position,
                debug_panel_size,
                panel_color,
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
                            info_lines
                                .push(format!("Sun dir: {:.2}, {:.2}, {:.2}", dir.x, dir.y, dir.z));
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
                        info_lines.push(format!(
                            "Debug view: {}",
                            ocean_debug_view_label(ocean.debug_view)
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

        let page_type = if self.debug_tab == DebugTab::Graphics {
            match self.debug_graphics_tab {
                DebugGraphicsTab::Sky => PageType::Sky,
                DebugGraphicsTab::Ocean => PageType::Ocean,
                DebugGraphicsTab::Clouds => PageType::Clouds,
            }
        } else {
            match self.debug_tab {
                DebugTab::Physics => PageType::Physics,
                DebugTab::Audio => PageType::Audio,
                DebugTab::Graphics => PageType::Sky,
            }
        };
        let debug_radials = unsafe { debug_registry_radials(page_type) };
        let debug_sliders = unsafe { debug_registry_sliders(page_type) };
        let content_top = text_start.y;
        let content_bottom = debug_taskbar_pos.y - 8.0 * ui_scale;
        let content_height = (content_bottom - content_top).max(0.0);
        let scrollbar_width = (8.0 * ui_scale).clamp(6.0, 12.0);
        let scrollbar_gap = 6.0 * ui_scale;
        let content_width = (debug_panel_size.x - scrollbar_width - scrollbar_gap).max(0.0);
        let content_clip_rect = GuiClipRect::from_position_size(
            [debug_panel_position.x, content_top],
            [content_width, content_height],
        );
        gui.submit_draw(GuiDraw::with_clip_rect(
            GuiLayer::Overlay,
            None,
            quad_from_pixels(
                vec2(debug_panel_position.x, content_top),
                vec2(content_width, content_height),
                panel_color,
                viewport,
            ),
            content_clip_rect,
        ));
        let line_height = 18.0 * ui_scale;
        let toggle_metrics = RadialButtonMetrics {
            item_height: (22.0 * ui_scale).clamp(18.0, 28.0),
            item_gap: (8.0 * ui_scale).clamp(4.0, 12.0),
            padding: [12.0 * ui_scale, 6.0 * ui_scale],
            button_size: [16.0 * ui_scale, 16.0 * ui_scale],
            indicator_size: [8.0 * ui_scale, 8.0 * ui_scale],
            label_gap: 10.0 * ui_scale,
            char_width: 7.2 * ui_scale,
            font_scale: 0.85 * ui_scale,
            text_offset: [0.0, 6.0 * ui_scale],
        };
        let radial_metrics = RadialButtonMetrics {
            item_height: (20.0 * ui_scale).clamp(18.0, 26.0),
            item_gap: (6.0 * ui_scale).clamp(4.0, 10.0),
            padding: [12.0 * ui_scale, 6.0 * ui_scale],
            button_size: [14.0 * ui_scale, 14.0 * ui_scale],
            indicator_size: [7.0 * ui_scale, 7.0 * ui_scale],
            label_gap: 10.0 * ui_scale,
            char_width: 7.2 * ui_scale,
            font_scale: 0.82 * ui_scale,
            text_offset: [0.0, 6.0 * ui_scale],
        };
        let slider_metrics = SliderMetrics {
            item_height: (26.0 * ui_scale).clamp(22.0, 32.0),
            item_gap: (8.0 * ui_scale).clamp(4.0, 12.0),
            ..SliderMetrics::default()
        };
        let mut content_height_total = info_lines.len() as f32 * line_height + 8.0 * ui_scale;
        let mut toggle_height = 0.0;
        if self.debug_tab == DebugTab::Graphics {
            toggle_height = toggle_metrics.padding[1] * 2.0
                + 2.0 * toggle_metrics.item_height
                + toggle_metrics.item_gap;
            content_height_total += toggle_height + 8.0 * ui_scale;
        }
        if !debug_radials.is_empty() {
            let title_height = 18.0 * ui_scale;
            let group_gap = 10.0 * ui_scale;
            for group in &debug_radials {
                let options_height = radial_metrics.padding[1] * 2.0
                    + group.options.len() as f32 * radial_metrics.item_height
                    + group.options.len().saturating_sub(1) as f32 * radial_metrics.item_gap;
                content_height_total += title_height + options_height + group_gap;
            }
        }
        let mut slider_total_height = 0.0;
        if !debug_sliders.is_empty() {
            slider_total_height = slider_metrics.padding[1] * 2.0
                + debug_sliders.len() as f32 * slider_metrics.item_height
                + debug_sliders.len().saturating_sub(1) as f32 * slider_metrics.item_gap;
            content_height_total += slider_total_height;
        } else if debug_radials.is_empty() {
            content_height_total += line_height;
        }
        let max_scroll = (content_height_total - content_height).max(0.0);
        if max_scroll <= 0.0 {
            self.scroll_offset = 0.0;
        } else {
            self.scroll_offset = self.scroll_offset.clamp(0.0, max_scroll);
        }
        let content_area_hovered = point_in_rect(
            self.cursor,
            vec2(debug_panel_position.x, content_top),
            vec2(debug_panel_size.x, content_height),
        );
        if content_area_hovered && self.scroll_delta.abs() > 0.0 && max_scroll > 0.0 {
            self.scroll_offset = (self.scroll_offset - self.scroll_delta * 18.0 * ui_scale)
                .clamp(0.0, max_scroll);
        }
        self.scroll_delta = 0.0;
        let content_scroll = -self.scroll_offset;

        for (index, line) in info_lines.iter().enumerate() {
            let line_y = text_start.y + index as f32 * line_height + content_scroll;
            if line_y + line_height < content_top || line_y > content_bottom {
                continue;
            }
            gui.submit_text(GuiTextDraw {
                text: line.clone(),
                position: [text_start.x, line_y],
                color: Vec4::new(0.75, 0.8, 0.9, 1.0).to_array(),
                scale: if index == 0 { 0.9 } else { 0.85 },
            });
        }
        let toggle_start_y = text_start.y + info_lines.len() as f32 * line_height + 8.0 * ui_scale;
        let mut slider_start_y = toggle_start_y;

        let mut debug_toggle_layout = RadialButtonLayout::default();
        if self.debug_tab == DebugTab::Graphics {
            let (toggle_label, toggle_enabled, on_id, off_id) = match self.debug_graphics_tab {
                DebugGraphicsTab::Sky => ("Sky", self.toggle_values.sky_enabled, 6101, 6102),
                DebugGraphicsTab::Ocean => ("Ocean", self.toggle_values.ocean_enabled, 6201, 6202),
                DebugGraphicsTab::Clouds => {
                    ("Clouds", self.toggle_values.cloud_enabled, 6301, 6302)
                }
            };
            let toggle_buttons = vec![
                RadialButton::new(on_id, format!("{toggle_label} On"), toggle_enabled),
                RadialButton::new(off_id, format!("{toggle_label} Off"), !toggle_enabled),
            ];
            let toggle_options = RadialButtonRenderOptions {
                viewport: [viewport.x, viewport.y],
                position: [debug_panel_position.x, toggle_start_y + content_scroll],
                size: [content_width, toggle_height],
                layer: GuiLayer::Overlay,
                metrics: toggle_metrics,
                colors: RadialButtonColors::default(),
                state: self.debug_toggle_state,
                clip_rect: Some(content_clip_rect),
            };
            debug_toggle_layout = gui.submit_radial_buttons(&toggle_buttons, &toggle_options);
            slider_start_y = toggle_start_y + toggle_height + 8.0 * ui_scale;
        }

        let mut debug_param_radial_layouts = Vec::new();
        if !debug_radials.is_empty() {
            let title_height = 18.0 * ui_scale;
            let group_gap = 10.0 * ui_scale;
            for group in &debug_radials {
                let title_y = slider_start_y + content_scroll;
                if title_y + title_height >= content_top && title_y <= content_bottom {
                    gui.submit_text(GuiTextDraw {
                        text: group.label.clone(),
                        position: [text_start.x, title_y],
                        color: Vec4::new(0.8, 0.84, 0.92, 1.0).to_array(),
                        scale: 0.82,
                    });
                }
                let current_value = group.value.round();
                let buttons = group
                    .options
                    .iter()
                    .map(|option| {
                        let selected = option.value.round() == current_value;
                        RadialButton::new(option.id, option.label.clone(), selected)
                    })
                    .collect::<Vec<_>>();
                let options_height = radial_metrics.padding[1] * 2.0
                    + buttons.len() as f32 * radial_metrics.item_height
                    + buttons.len().saturating_sub(1) as f32 * radial_metrics.item_gap;
                let radial_options = RadialButtonRenderOptions {
                    viewport: [viewport.x, viewport.y],
                    position: [debug_panel_position.x, slider_start_y + title_height + content_scroll],
                    size: [content_width, options_height],
                    layer: GuiLayer::Overlay,
                    metrics: radial_metrics,
                    colors: RadialButtonColors::default(),
                    state: self.debug_param_radial_state,
                    clip_rect: Some(content_clip_rect),
                };
                let layout = gui.submit_radial_buttons(&buttons, &radial_options);
                debug_param_radial_layouts.push(layout);
                slider_start_y += title_height + options_height + group_gap;
            }
        }

        let mut debug_slider_layout = SliderLayout::default();
        if debug_sliders.is_empty() && debug_radials.is_empty() {
            let empty_y = slider_start_y + content_scroll;
            if empty_y + line_height >= content_top && empty_y <= content_bottom {
                gui.submit_text(GuiTextDraw {
                    text: "No debug data available.".to_string(),
                    position: [text_start.x, empty_y],
                    color: Vec4::new(0.7, 0.75, 0.85, 1.0).to_array(),
                    scale: 0.85,
                });
            }
        } else if !debug_sliders.is_empty() {
            let debug_slider_options = SliderRenderOptions {
                viewport: [viewport.x, viewport.y],
                position: [debug_panel_position.x, slider_start_y + content_scroll],
                size: [content_width, slider_total_height],
                layer: GuiLayer::Overlay,
                metrics: slider_metrics,
                colors: SliderColors::default(),
                state: self.debug_slider_state,
                clip_rect: Some(content_clip_rect),
            };
            debug_slider_layout = gui.submit_sliders(&debug_sliders, &debug_slider_options);
        }

        if max_scroll > 0.0 && content_height > 0.0 {
            let track_pos = vec2(
                debug_panel_position.x + content_width + scrollbar_gap,
                content_top,
            );
            let track_size = vec2(scrollbar_width, content_height);
            gui.submit_draw(GuiDraw::new(
                GuiLayer::Overlay,
                None,
                quad_from_pixels(
                    track_pos,
                    track_size,
                    Vec4::new(0.08, 0.1, 0.14, 0.8),
                    viewport,
                ),
            ));
            let mut thumb_height = (content_height / content_height_total) * content_height;
            let min_thumb = 24.0 * ui_scale;
            if thumb_height < min_thumb {
                thumb_height = min_thumb.min(content_height);
            }
            let thumb_range = (content_height - thumb_height).max(0.0);
            let thumb_offset = if max_scroll > 0.0 {
                (self.scroll_offset / max_scroll) * thumb_range
            } else {
                0.0
            };
            let thumb_pos = vec2(track_pos.x, content_top + thumb_offset);
            gui.submit_draw(GuiDraw::new(
                GuiLayer::Overlay,
                None,
                quad_from_pixels(
                    thumb_pos,
                    vec2(scrollbar_width, thumb_height),
                    Vec4::new(0.34, 0.42, 0.55, 0.9),
                    viewport,
                ),
            ));
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
        self.debug_toggle_layout = debug_toggle_layout;
        self.debug_param_radial_layouts = debug_param_radial_layouts;
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
        self.debug_toggle_state = RadialButtonState::default();
        self.debug_toggle_layout = RadialButtonLayout::default();
        self.debug_param_radial_state = RadialButtonState::default();
        self.debug_param_radial_layouts.clear();
        self.scroll_offset = 0.0;
        self.scroll_delta = 0.0;
    }
}

fn cloud_debug_view_from_value(value: f32) -> CloudDebugView {
    match value.round().clamp(0.0, 23.0) as u32 {
        1 => CloudDebugView::WeatherMap,
        2 => CloudDebugView::ShadowMap,
        3 => CloudDebugView::Transmittance,
        4 => CloudDebugView::StepHeatmap,
        5 => CloudDebugView::TemporalWeight,
        6 => CloudDebugView::Stats,
        7 => CloudDebugView::LayerA,
        8 => CloudDebugView::LayerB,
        9 => CloudDebugView::SingleScatter,
        10 => CloudDebugView::MultiScatter,
        11 => CloudDebugView::ShadowCascade0,
        12 => CloudDebugView::ShadowCascade1,
        13 => CloudDebugView::ShadowCascade2,
        14 => CloudDebugView::ShadowCascade3,
        20 => CloudDebugView::OpaqueShadowCascade0,
        21 => CloudDebugView::OpaqueShadowCascade1,
        22 => CloudDebugView::OpaqueShadowCascade2,
        23 => CloudDebugView::OpaqueShadowCascade3,
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
        CloudDebugView::SingleScatter => "Single Scatter",
        CloudDebugView::MultiScatter => "Multi Scatter",
        CloudDebugView::ShadowCascade0 => "Cloud Shadow Cascade 0",
        CloudDebugView::ShadowCascade1 => "Cloud Shadow Cascade 1",
        CloudDebugView::ShadowCascade2 => "Cloud Shadow Cascade 2",
        CloudDebugView::ShadowCascade3 => "Cloud Shadow Cascade 3",
        CloudDebugView::OpaqueShadowCascade0 => "Opaque Shadow Cascade 0",
        CloudDebugView::OpaqueShadowCascade1 => "Opaque Shadow Cascade 1",
        CloudDebugView::OpaqueShadowCascade2 => "Opaque Shadow Cascade 2",
        CloudDebugView::OpaqueShadowCascade3 => "Opaque Shadow Cascade 3",
    }
}

fn ocean_debug_view_from_value(value: f32) -> OceanDebugView {
    match value.round().clamp(0.0, 4.0) as u32 {
        1 => OceanDebugView::Normals,
        2 => OceanDebugView::WaveHeight,
        3 => OceanDebugView::FoamMask,
        4 => OceanDebugView::Velocity,
        _ => OceanDebugView::None,
    }
}

fn ocean_debug_view_label(view: OceanDebugView) -> &'static str {
    match view {
        OceanDebugView::None => "None",
        OceanDebugView::Normals => "Normals",
        OceanDebugView::WaveHeight => "Wave Height",
        OceanDebugView::FoamMask => "Foam Mask",
        OceanDebugView::Velocity => "Velocity",
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
