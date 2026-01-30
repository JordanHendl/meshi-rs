use dashi::Handle;
use glam::{Vec2, Vec4, vec2};
use meshi_ffi_structs::event::{Event, EventSource, EventType, KeyCode};
use meshi_ffi_structs::{LightFlags, LightInfo, LightType};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::gui::{
    GuiClipRect, GuiContext, GuiDraw, GuiLayer, GuiQuad, GuiTextDraw, MenuRect, RadialButton,
    RadialButtonColors, RadialButtonLayout, RadialButtonMetrics, RadialButtonRenderOptions,
    RadialButtonState, Slider, SliderColors, SliderLayout, SliderMetrics, SliderRenderOptions,
    SliderState, SliderValueFormat,
};
use crate::Light;
use crate::render::environment::ocean::OceanDebugView;
use crate::structs::{CloudDebugView, CloudResolutionScale};

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
    Shadow,
    DebugViews,
    Lighting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageType {
    Sky,
    Ocean,
    Clouds,
    Shadow,
    DebugViews,
    Lighting,
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

#[derive(Clone)]
pub struct DebugLightEntry {
    pub handle: Handle<Light>,
    pub name: String,
    pub info: LightInfo,
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
    description: Option<String>,
    control: DebugRegistryControl,
    value: DebugRegistryValue,
    conflicts: Vec<DebugRegistryValue>,
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
    debug_register_slider_with_description(
        page,
        slider,
        DebugRegistryValue::Float(value_ptr),
        label,
        None,
    )
}

pub unsafe fn debug_register_int(
    page: PageType,
    mut slider: Slider,
    value_ptr: *mut u32,
    label: &str,
) -> u32 {
    slider.value_format = SliderValueFormat::Integer;
    debug_register_slider_with_description(
        page,
        slider,
        DebugRegistryValue::U32(value_ptr),
        label,
        None,
    )
}

pub unsafe fn debug_register_with_description(
    page: PageType,
    slider: Slider,
    value_ptr: *mut f32,
    label: &str,
    description: Option<&str>,
) -> u32 {
    debug_register_slider_with_description(
        page,
        slider,
        DebugRegistryValue::Float(value_ptr),
        label,
        description,
    )
}

pub unsafe fn debug_register_int_with_description(
    page: PageType,
    mut slider: Slider,
    value_ptr: *mut u32,
    label: &str,
    description: Option<&str>,
) -> u32 {
    slider.value_format = SliderValueFormat::Integer;
    debug_register_slider_with_description(
        page,
        slider,
        DebugRegistryValue::U32(value_ptr),
        label,
        description,
    )
}

pub unsafe fn debug_register_slider(
    page: PageType,
    slider: Slider,
    value: DebugRegistryValue,
    label: &str,
) -> u32 {
    debug_register_slider_with_description(page, slider, value, label, None)
}

pub unsafe fn debug_register_slider_with_description(
    page: PageType,
    slider: Slider,
    value: DebugRegistryValue,
    label: &str,
    description: Option<&str>,
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
        if let Some(description) = description {
            entry.description = Some(description.to_string());
        }
        return entry.id;
    }
    let id = DEBUG_REGISTRY_NEXT_ID.fetch_add(1, Ordering::Relaxed);
    registry.push(DebugRegistryItem {
        id,
        page,
        label: label.to_string(),
        description: description.map(str::to_string),
        control: DebugRegistryControl::Slider {
            min: slider.min,
            max: slider.max,
            enabled: slider.enabled,
            show_value: slider.show_value,
            value_format: slider.value_format,
        },
        value,
        conflicts: Vec::new(),
    });
    id
}

pub unsafe fn debug_register_radial(
    page: PageType,
    label: &str,
    value: DebugRegistryValue,
    options: &[DebugRadialOption],
) -> u32 {
    debug_register_radial_with_description(page, label, value, options, None)
}

pub unsafe fn debug_register_radial_with_description(
    page: PageType,
    label: &str,
    value: DebugRegistryValue,
    options: &[DebugRadialOption],
    description: Option<&str>,
) -> u32 {
    debug_register_radial_with_description_and_conflicts(
        page,
        label,
        value,
        options,
        description,
        None,
    )
}

pub unsafe fn debug_register_radial_with_description_and_conflicts(
    page: PageType,
    label: &str,
    value: DebugRegistryValue,
    options: &[DebugRadialOption],
    description: Option<&str>,
    conflicts: Option<&[DebugRegistryValue]>,
) -> u32 {
    let registry = DEBUG_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let mut registry = registry.lock().expect("debug registry poisoned");
    let conflicts = conflicts
        .map(|values| values.to_vec())
        .unwrap_or_default();
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
            entry.conflicts = conflicts;
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
        if let Some(description) = description {
            entry.description = Some(description.to_string());
        }
        entry.conflicts = conflicts;
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
        description: description.map(str::to_string),
        control: DebugRegistryControl::Radial {
            options: radial_options,
        },
        value,
        conflicts,
    });
    base_id
}

pub unsafe fn debug_registry_radial_enabled(value: &DebugRegistryValue) -> bool {
    debug_registry_radial_value(value)
        .map(|value| value >= 0.5)
        .unwrap_or(false)
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

unsafe fn debug_registry_tooltip_for_id(id: u32) -> Option<String> {
    let registry = DEBUG_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let registry = registry.lock().expect("debug registry poisoned");
    for entry in registry.iter() {
        if entry.id == id {
            if matches!(entry.control, DebugRegistryControl::Slider { .. }) {
                return Some(build_slider_tooltip(
                    &entry.label,
                    entry.description.as_deref(),
                ));
            }
        }
        if let DebugRegistryControl::Radial { options } = &entry.control {
            if let Some(option) = options.iter().find(|option| option.id == id) {
                return Some(build_radial_tooltip(
                    &entry.label,
                    &option.label,
                    entry.description.as_deref(),
                ));
            }
        }
    }
    None
}

fn build_slider_tooltip(label: &str, description: Option<&str>) -> String {
    description
        .map(|text| text.to_string())
        .unwrap_or_else(|| format!("Adjust {label}."))
}

fn build_radial_tooltip(group_label: &str, option_label: &str, description: Option<&str>) -> String {
    description
        .map(|text| format!("{text} ({option_label})."))
        .unwrap_or_else(|| format!("Set {group_label} to {option_label}."))
}

unsafe fn debug_registry_update_value(id: u32, value: f32) -> Option<PageType> {
    let registry = DEBUG_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let mut registry = registry.lock().expect("debug registry poisoned");
    if let Some(entry) = registry.iter_mut().find(|entry| entry.id == id) {
        let (min, max) = match &entry.control {
            DebugRegistryControl::Slider { min, max, .. } => (*min, *max),
            DebugRegistryControl::Radial { .. } => return None,
        };
        let clamped = value.clamp(min, max);
        unsafe {
            let previous = entry.value.get();
            if (previous - clamped).abs() > f32::EPSILON {
                entry.value.set(clamped);
                return Some(entry.page);
            }
        }
        return None;
    }
    None
}

unsafe fn debug_registry_update_radial(id: u32) -> Option<PageType> {
    let registry = DEBUG_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let mut registry = registry.lock().expect("debug registry poisoned");
    for index in 0..registry.len() {
        let option_value = {
            let entry = &registry[index];
            let DebugRegistryControl::Radial { options } = &entry.control else {
                continue;
            };
            let Some(option) = options.iter().find(|option| option.id == id) else {
                continue;
            };
            option.value
        };
        let (entry_id, page, conflicts, updated) = {
            let entry = &mut registry[index];
            let previous = unsafe { entry.value.get() };
            let updated = (previous - option_value).abs() > f32::EPSILON;
            if updated {
                unsafe {
                    entry.value.set(option_value);
                }
            }
            (entry.id, entry.page, entry.conflicts.clone(), updated)
        };
        if updated && option_value.abs() > f32::EPSILON {
            apply_radial_conflicts(&mut registry, entry_id, &conflicts);
            return Some(page);
        }
        return if updated { Some(page) } else { None };
    }
    None
}

fn apply_radial_conflicts(
    registry: &mut [DebugRegistryItem],
    active_id: u32,
    conflicts: &[DebugRegistryValue],
) {
    if conflicts.is_empty() {
        return;
    }
    for conflict in conflicts {
        if let Some(entry) = registry
            .iter_mut()
            .find(|entry| entry.id != active_id && entry.value.matches(conflict))
        {
            let DebugRegistryControl::Radial { options } = &entry.control else {
                continue;
            };
            let default_value = radial_default_value(options);
            unsafe {
                entry.value.set(default_value);
            }
        }
    }
}

unsafe fn debug_registry_radial_value(value: &DebugRegistryValue) -> Option<f32> {
    let registry = DEBUG_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let registry = registry.lock().expect("debug registry poisoned");
    registry
        .iter()
        .find(|entry| {
            matches!(entry.control, DebugRegistryControl::Radial { .. })
                && entry.value.matches(value)
        })
        .map(|entry| unsafe { entry.value.get() })
}

fn radial_default_value(options: &[DebugRegistryRadialOption]) -> f32 {
    options
        .iter()
        .find(|option| option.value.abs() <= f32::EPSILON)
        .map(|option| option.value)
        .unwrap_or_else(|| options.first().map(|option| option.value).unwrap_or(0.0))
}

fn mark_page_dirty(
    page: PageType,
    skybox_dirty: &mut bool,
    sky_dirty: &mut bool,
    ocean_dirty: &mut bool,
    cloud_dirty: &mut bool,
) {
    match page {
        PageType::Sky => {
            *skybox_dirty = true;
            *sky_dirty = true;
        }
        PageType::Ocean => {
            *ocean_dirty = true;
        }
        PageType::Clouds | PageType::Shadow => {
            *cloud_dirty = true;
        }
        PageType::DebugViews
        | PageType::Lighting
        | PageType::Physics
        | PageType::Audio => {}
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragTarget {
    DebugPanel,
}

pub struct DebugGuiBindings<'a> {
    pub debug_mode: *mut bool,
    pub lights: &'a [DebugLightEntry],
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
    grave_down: bool,
    debug_toggle_requested: bool,
    debug_tab: DebugTab,
    debug_graphics_tab: DebugGraphicsTab,
    debug_slider_state: SliderState,
    debug_slider_layout: SliderLayout,
    debug_param_radial_state: RadialButtonState,
    debug_param_radial_layouts: Vec<RadialButtonLayout>,
    debug_panel_position: Vec2,
    drag_target: Option<DragTarget>,
    drag_offset: Vec2,
    scroll_offset: f32,
    scroll_delta: f32,
}

impl DebugGui {
    pub fn new() -> Self {
        Self {
            cursor: Vec2::ZERO,
            mouse_pressed: false,
            mouse_down: false,
            control_down: false,
            grave_down: false,
            debug_toggle_requested: false,
            debug_tab: DebugTab::Graphics,
            debug_graphics_tab: DebugGraphicsTab::Sky,
            debug_slider_state: SliderState::default(),
            debug_slider_layout: SliderLayout::default(),
            debug_param_radial_state: RadialButtonState::default(),
            debug_param_radial_layouts: Vec::new(),
            debug_panel_position: vec2(560.0, 60.0),
            drag_target: None,
            drag_offset: Vec2::ZERO,
            scroll_offset: 0.0,
            scroll_delta: 0.0,
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
                        KeyCode::Control => {
                            self.control_down = true;
                            if self.grave_down {
                                self.debug_toggle_requested = true;
                            }
                        }
                        KeyCode::GraveAccent => {
                            self.grave_down = true;
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
                    if event.key() == KeyCode::GraveAccent {
                        self.grave_down = false;
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
        bindings: DebugGuiBindings<'_>,
    ) -> DebugGuiOutput {
        if self.debug_toggle_requested {
            self.debug_toggle_requested = false;
            unsafe {
                if let Some(debug_mode) = bindings.debug_mode.as_mut() {
                    *debug_mode = true;
                }
            }
        }

        let debug_mode = unsafe { bindings.debug_mode.as_ref().copied().unwrap_or(false) };
        if !debug_mode {
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
                DebugGraphicsTab::Shadow,
                DebugGraphicsTab::DebugViews,
                DebugGraphicsTab::Lighting,
            ];
            let subtab_width =
                (debug_panel_size.x - subtab_padding * 2.0) / subtabs.len() as f32;
            for (index, tab) in subtabs.iter().enumerate() {
                let tab_pos = vec2(subtab_x + subtab_width * index as f32, subtab_y);
                let tab_size = vec2(subtab_width - subtab_gap, subtab_height);
                if point_in_rect(self.cursor, tab_pos, tab_size) {
                    self.debug_graphics_tab = *tab;
                    self.debug_slider_state.active = None;
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
        let tooltip_text = hovered_debug_slider
            .and_then(|item| unsafe { debug_registry_tooltip_for_id(item.id) })
            .or_else(|| {
                hovered_param_radial
                    .and_then(|item| unsafe { debug_registry_tooltip_for_id(item.id) })
            });

        if self.mouse_pressed {
            if let Some(item) = hovered_debug_slider {
                self.debug_slider_state.active = Some(item.id);
            }
            if let Some(item) = hovered_param_radial {
                self.debug_param_radial_state.active = Some(item.id);
                unsafe {
                    if let Some(page) = debug_registry_update_radial(item.id) {
                        mark_page_dirty(page, &mut skybox_dirty, &mut sky_dirty, &mut ocean_dirty, &mut cloud_dirty);
                    }
                }
            }
        }

        if !self.mouse_down {
            self.debug_slider_state.active = None;
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
                unsafe {
                    if let Some(page) = debug_registry_update_value(active_id, value) {
                        mark_page_dirty(
                            page,
                            &mut skybox_dirty,
                            &mut sky_dirty,
                            &mut ocean_dirty,
                            &mut cloud_dirty,
                        );
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

        let mut gui = GuiContext::new();
        let panel_brightness = 1.0;
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
            let subtab_y = tab_y + tab_height + 6.0 * ui_scale;
            let subtab_x = debug_panel_position.x + subtab_padding;
            let subtabs = [
                (DebugGraphicsTab::Sky, "Sky"),
                (DebugGraphicsTab::Ocean, "Ocean"),
                (DebugGraphicsTab::Clouds, "Clouds"),
                (DebugGraphicsTab::Shadow, "Shadow"),
                (DebugGraphicsTab::DebugViews, "Debug Views"),
                (DebugGraphicsTab::Lighting, "Lighting"),
            ];
            let subtab_width =
                (debug_panel_size.x - subtab_padding * 2.0) / subtabs.len() as f32;
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
        if self.debug_tab == DebugTab::Graphics
            && self.debug_graphics_tab == DebugGraphicsTab::Lighting
        {
            info_lines.push(format!("Lights: {}", bindings.lights.len()));
            for entry in bindings.lights {
                info_lines.extend(build_light_info_lines(entry));
            }
        }

        let page_type = if self.debug_tab == DebugTab::Graphics {
            match self.debug_graphics_tab {
                DebugGraphicsTab::Sky => PageType::Sky,
                DebugGraphicsTab::Ocean => PageType::Ocean,
                DebugGraphicsTab::Clouds => PageType::Clouds,
                DebugGraphicsTab::Shadow => PageType::Shadow,
                DebugGraphicsTab::DebugViews => PageType::DebugViews,
                DebugGraphicsTab::Lighting => PageType::Lighting,
            }
        } else {
            match self.debug_tab {
                DebugTab::Physics => PageType::Physics,
                DebugTab::Audio => PageType::Audio,
                DebugTab::Graphics => PageType::Sky,
            }
        };
        let (debug_radials, debug_sliders) = if page_type == PageType::Lighting {
            (Vec::new(), Vec::new())
        } else {
            (
                unsafe { debug_registry_radials(page_type) },
                unsafe { debug_registry_sliders(page_type) },
            )
        };
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
        let mut slider_start_y =
            text_start.y + info_lines.len() as f32 * line_height + 8.0 * ui_scale;

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
            if page_type != PageType::Lighting
                && empty_y + line_height >= content_top
                && empty_y <= content_bottom
            {
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

        if let Some(tooltip_text) = tooltip_text {
            let tooltip_padding = vec2(8.0, 6.0) * ui_scale;
            let tooltip_text_height = 16.0 * ui_scale;
            let tooltip_char_width = 6.6 * ui_scale;
            let tooltip_text_width = tooltip_text.len() as f32 * tooltip_char_width;
            let tooltip_size = vec2(
                tooltip_text_width + tooltip_padding.x * 2.0,
                tooltip_text_height + tooltip_padding.y * 2.0,
            );
            let mut tooltip_pos = self.cursor + vec2(12.0, 16.0) * ui_scale;
            if tooltip_pos.x + tooltip_size.x > viewport.x {
                tooltip_pos.x = (viewport.x - tooltip_size.x - 8.0 * ui_scale).max(0.0);
            }
            if tooltip_pos.y + tooltip_size.y > viewport.y {
                tooltip_pos.y = (viewport.y - tooltip_size.y - 8.0 * ui_scale).max(0.0);
            }
            gui.submit_draw(GuiDraw::new(
                GuiLayer::Overlay,
                None,
                quad_from_pixels(
                    tooltip_pos,
                    tooltip_size,
                    Vec4::new(0.08, 0.1, 0.14, 0.96),
                    viewport,
                ),
            ));
            gui.submit_text(GuiTextDraw {
                text: tooltip_text,
                position: [
                    tooltip_pos.x + tooltip_padding.x,
                    tooltip_pos.y + tooltip_padding.y + 2.0 * ui_scale,
                ],
                color: Vec4::new(0.9, 0.93, 0.98, 1.0).to_array(),
                scale: 0.78 * ui_scale,
            });
        }

        self.debug_slider_layout = debug_slider_layout;
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
        self.debug_param_radial_state = RadialButtonState::default();
        self.debug_param_radial_layouts.clear();
        self.scroll_offset = 0.0;
        self.scroll_delta = 0.0;
    }
}

fn build_light_info_lines(entry: &DebugLightEntry) -> Vec<String> {
    let info = entry.info;
    let mut lines = Vec::new();
    lines.push(format!("Light: {} ({:?})", entry.name, entry.handle));
    lines.push(format!("  Type: {}", light_type_label(info.ty)));
    lines.push(format!("  Flags: {}", light_flags_label(info.flags)));
    lines.push(format!("  Intensity: {:.3}", info.intensity));
    lines.push(format!(
        "  Color: {:.3}, {:.3}, {:.3}",
        info.color_r, info.color_g, info.color_b
    ));

    match info.ty {
        LightType::Directional => {
            lines.push(format!(
                "  Direction: {:.3}, {:.3}, {:.3}",
                info.dir_x, info.dir_y, info.dir_z
            ));
        }
        LightType::Point => {
            lines.push(format!(
                "  Position: {:.2}, {:.2}, {:.2}",
                info.pos_x, info.pos_y, info.pos_z
            ));
            lines.push(format!("  Range: {:.2}", info.range));
        }
        LightType::Spot => {
            lines.push(format!(
                "  Position: {:.2}, {:.2}, {:.2}",
                info.pos_x, info.pos_y, info.pos_z
            ));
            lines.push(format!(
                "  Direction: {:.3}, {:.3}, {:.3}",
                info.dir_x, info.dir_y, info.dir_z
            ));
            lines.push(format!("  Range: {:.2}", info.range));
            lines.push(format!(
                "  Cone: {:.1}° / {:.1}°",
                info.spot_inner_angle_rad.to_degrees(),
                info.spot_outer_angle_rad.to_degrees()
            ));
        }
        LightType::RectArea => {
            lines.push(format!(
                "  Position: {:.2}, {:.2}, {:.2}",
                info.pos_x, info.pos_y, info.pos_z
            ));
            lines.push(format!(
                "  Direction: {:.3}, {:.3}, {:.3}",
                info.dir_x, info.dir_y, info.dir_z
            ));
            lines.push(format!("  Range: {:.2}", info.range));
            lines.push(format!(
                "  Size: {:.2} x {:.2}",
                info.rect_half_width * 2.0,
                info.rect_half_height * 2.0
            ));
        }
    }

    lines
}

fn light_type_label(ty: LightType) -> &'static str {
    match ty {
        LightType::Directional => "Directional",
        LightType::Point => "Point",
        LightType::Spot => "Spot",
        LightType::RectArea => "Rect Area",
    }
}

fn light_flags_label(flags: u32) -> String {
    let parsed = LightFlags::from_bits_truncate(flags);
    let mut labels = Vec::new();
    if parsed.contains(LightFlags::CASTS_SHADOWS) {
        labels.push("Shadows");
    }
    if parsed.contains(LightFlags::VOLUMETRIC) {
        labels.push("Volumetric");
    }
    if labels.is_empty() {
        "None".to_string()
    } else {
        labels.join(", ")
    }
}

fn cloud_debug_view_from_value(value: f32) -> CloudDebugView {
    match value.round().clamp(0.0, 25.0) as u32 {
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
        24 => CloudDebugView::OpaqueShadowAtlas,
        25 => CloudDebugView::OpaqueShadowSampleUV,
        _ => CloudDebugView::None,
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
