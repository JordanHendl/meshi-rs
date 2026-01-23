use glam::{Vec2, Vec4, vec2};
use meshi_ffi_structs::event::{Event, EventSource, EventType, KeyCode};

use crate::gui::{
    GuiContext, GuiDraw, GuiLayer, GuiQuad, GuiTextDraw, MenuRect, Slider, SliderColors,
    SliderLayout, SliderMetrics, SliderRenderOptions, SliderState,
};
use crate::render::environment::sky::{SkyFrameSettings, SkyboxFrameSettings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DebugTab {
    Graphics,
    Physics,
    Audio,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragTarget {
    DebugPanel,
}

#[derive(Clone, Copy, Debug)]
struct DebugSliderValues {
    skybox_intensity: f32,
    sun_intensity: f32,
}

pub struct DebugGuiBindings {
    pub debug_mode: *mut bool,
    pub skybox_settings: *mut SkyboxFrameSettings,
    pub sky_settings: *mut SkyFrameSettings,
}

pub struct DebugGuiOutput {
    pub frame: Option<crate::gui::GuiFrame>,
    pub skybox_dirty: bool,
    pub sky_dirty: bool,
}

pub struct DebugGui {
    cursor: Vec2,
    mouse_pressed: bool,
    mouse_down: bool,
    control_down: bool,
    debug_toggle_requested: bool,
    debug_tab: DebugTab,
    debug_slider_state: SliderState,
    debug_slider_layout: SliderLayout,
    debug_panel_position: Vec2,
    drag_target: Option<DragTarget>,
    drag_offset: Vec2,
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
            debug_slider_state: SliderState::default(),
            debug_slider_layout: SliderLayout::default(),
            debug_panel_position: vec2(560.0, 60.0),
            drag_target: None,
            drag_offset: Vec2::ZERO,
            slider_values: DebugSliderValues {
                skybox_intensity: 1.0,
                sun_intensity: 1.0,
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
            self.mouse_pressed = false;
            return DebugGuiOutput {
                frame: None,
                skybox_dirty: false,
                sky_dirty: false,
            };
        }

        if self.debug_slider_state.active.is_none() {
            unsafe {
                if let Some(skybox) = bindings.skybox_settings.as_ref() {
                    self.slider_values.skybox_intensity = skybox.intensity;
                }
                if let Some(sky) = bindings.sky_settings.as_ref() {
                    self.slider_values.sun_intensity = sky.sun_intensity;
                }
            }
        }

        let debug_panel_size = vec2(340.0, 260.0);
        let debug_title_bar_height = 28.0;
        let debug_panel_position = self.debug_panel_position;
        let debug_title_bar_pos = debug_panel_position;
        let debug_title_bar_size = vec2(debug_panel_size.x, debug_title_bar_height);
        let debug_title_hovered =
            point_in_rect(self.cursor, debug_title_bar_pos, debug_title_bar_size);

        if self.mouse_pressed {
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
            let tab_height = 26.0;
            let tab_width = (debug_panel_size.x - 24.0) / 3.0;
            let tab_y = debug_title_bar_pos.y + debug_title_bar_height + 6.0;
            let tab_x = debug_panel_position.x + 12.0;
            let tabs = [DebugTab::Graphics, DebugTab::Physics, DebugTab::Audio];
            for (index, tab) in tabs.iter().enumerate() {
                let tab_pos = vec2(tab_x + tab_width * index as f32, tab_y);
                let tab_size = vec2(tab_width - 6.0, tab_height);
                if point_in_rect(self.cursor, tab_pos, tab_size) {
                    self.debug_tab = *tab;
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
                    _ => {}
                }
            }
        }

        let mut skybox_dirty = false;
        let mut sky_dirty = false;
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

        let tab_height = 26.0;
        let tab_width = (debug_panel_size.x - 24.0) / 3.0;
        let tab_y = debug_title_bar_pos.y + debug_title_bar_height + 6.0;
        let tab_x = debug_panel_position.x + 12.0;
        let tabs = [
            (DebugTab::Graphics, "Graphics"),
            (DebugTab::Physics, "Physics"),
            (DebugTab::Audio, "Audio"),
        ];
        for (index, (tab, label)) in tabs.iter().enumerate() {
            let tab_pos = vec2(tab_x + tab_width * index as f32, tab_y);
            let tab_size = vec2(tab_width - 6.0, tab_height);
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
                position: [tab_pos.x + 10.0, tab_pos.y + 6.0],
                color: Vec4::new(0.9, 0.93, 0.98, 1.0).to_array(),
                scale: 0.85,
            });
        }

        let text_start = vec2(debug_panel_position.x + 16.0, tab_y + tab_height + 12.0);
        gui.submit_text(GuiTextDraw {
            text: format!("Renderer: {renderer_label}"),
            position: [text_start.x, text_start.y],
            color: Vec4::new(0.85, 0.9, 0.98, 1.0).to_array(),
            scale: 0.9,
        });
        gui.submit_text(GuiTextDraw {
            text: format!("Debug mode: {}", debug_mode),
            position: [text_start.x, text_start.y + 18.0],
            color: Vec4::new(0.75, 0.8, 0.9, 1.0).to_array(),
            scale: 0.85,
        });

        let mut debug_slider_layout = SliderLayout::default();
        if self.debug_tab == DebugTab::Graphics {
            let debug_slider_options = SliderRenderOptions {
                viewport: [viewport.x, viewport.y],
                position: [debug_panel_position.x, text_start.y + 40.0],
                size: [
                    debug_panel_size.x,
                    debug_panel_size.y - (text_start.y - debug_panel_position.y) - 52.0,
                ],
                layer: GuiLayer::Overlay,
                metrics: SliderMetrics {
                    item_height: 26.0,
                    item_gap: 8.0,
                    ..SliderMetrics::default()
                },
                colors: SliderColors::default(),
                state: self.debug_slider_state,
            };

            let debug_sliders = [
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
            ];
            debug_slider_layout = gui.submit_sliders(&debug_sliders, &debug_slider_options);
        } else {
            gui.submit_text(GuiTextDraw {
                text: "No debug data available.".to_string(),
                position: [text_start.x, text_start.y + 40.0],
                color: Vec4::new(0.7, 0.75, 0.85, 1.0).to_array(),
                scale: 0.85,
            });
        }

        self.debug_slider_layout = debug_slider_layout;
        self.mouse_pressed = false;

        DebugGuiOutput {
            frame: Some(gui.build_frame()),
            skybox_dirty,
            sky_dirty,
        }
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
