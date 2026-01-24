//! GUI rendering entry points.
//!
//! Expected entry points:
//! - **Initialization**: create a [`GuiContext`] tied to the renderer/device.
//! - **Registration**: register GUI layers, fonts, and resources up front.
//! - **Draw submission**: submit [`GuiDraw`] records each frame for rendering.
//! - **Frame build**: generate a sorted, batched mesh for GPU upload.

pub mod debug;
pub mod dock;
pub mod icon_atlas;

pub use icon_atlas::{
    GuiIconAtlas, GuiIconAtlasError, GuiIconAtlasInfo, GuiIconRect, GuiIconUv, MenuGlyphId,
    ToolbarIconId,
};

use std::collections::HashSet;
use std::ops::Range;

use crate::render::gui::{GuiMesh, GuiVertex};
use glam::Vec2;
use meshi_ffi_structs::event::{Event, EventSource, EventType, KeyCode};

const GUI_NO_TEXTURE_ID: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GuiId(u64);

impl GuiId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u32> for GuiId {
    fn from(value: u32) -> Self {
        Self(value as u64)
    }
}

impl From<usize> for GuiId {
    fn from(value: usize) -> Self {
        Self(value as u64)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GuiInteraction {
    pub hovered: bool,
    pub active: bool,
    pub focused: bool,
    pub clicked: bool,
}

#[derive(Debug, Clone)]
pub struct GuiInput {
    pub cursor: Vec2,
    pub scroll_delta: Vec2,
    pub mouse_down: bool,
    pub mouse_pressed: bool,
    pub mouse_released: bool,
    hot: Option<GuiId>,
    active: Option<GuiId>,
    focused: Option<GuiId>,
    last_key_pressed: Option<KeyCode>,
    keys_down: HashSet<KeyCode>,
}

impl Default for GuiInput {
    fn default() -> Self {
        Self {
            cursor: Vec2::ZERO,
            scroll_delta: Vec2::ZERO,
            mouse_down: false,
            mouse_pressed: false,
            mouse_released: false,
            hot: None,
            active: None,
            focused: None,
            last_key_pressed: None,
            keys_down: HashSet::new(),
        }
    }
}

impl GuiInput {
    pub fn begin_frame(&mut self) {
        if !self.mouse_down {
            self.active = None;
        }
        self.scroll_delta = Vec2::ZERO;
        self.mouse_pressed = false;
        self.mouse_released = false;
        self.hot = None;
        self.last_key_pressed = None;
    }

    pub fn handle_event(&mut self, event: &Event) {
        unsafe {
            match (event.source(), event.event_type()) {
                (EventSource::Mouse, EventType::CursorMoved) => {
                    self.cursor = event.motion2d();
                }
                (EventSource::Mouse, EventType::Motion2D) => {
                    self.scroll_delta += event.motion2d();
                }
                (EventSource::MouseButton, EventType::Pressed) => {
                    self.mouse_pressed = true;
                    self.mouse_down = true;
                    if let Some(hot) = self.hot {
                        self.active = Some(hot);
                        self.focused = Some(hot);
                    }
                }
                (EventSource::MouseButton, EventType::Released) => {
                    self.mouse_down = false;
                    self.mouse_released = true;
                }
                (EventSource::Key, EventType::Pressed) => {
                    let key = event.key();
                    self.keys_down.insert(key);
                    self.last_key_pressed = Some(key);
                }
                (EventSource::Key, EventType::Released) => {
                    let key = event.key();
                    self.keys_down.remove(&key);
                }
                (EventSource::Window, EventType::WindowUnfocused) => {
                    self.focused = None;
                    self.active = None;
                    self.hot = None;
                    self.keys_down.clear();
                }
                _ => {}
            }
        }
    }

    pub fn interact(&mut self, id: GuiId, hovered: bool) -> GuiInteraction {
        if hovered {
            self.hot = Some(id);
        }

        if self.mouse_pressed && hovered {
            self.active = Some(id);
            self.focused = Some(id);
        }

        let mut clicked = false;
        if self.mouse_released && self.active == Some(id) {
            clicked = hovered;
            if !self.mouse_down {
                self.active = None;
            }
        }

        if !self.mouse_down && self.active == Some(id) {
            self.active = None;
        }

        GuiInteraction {
            hovered: self.hot == Some(id),
            active: self.active == Some(id),
            focused: self.focused == Some(id),
            clicked,
        }
    }

    pub fn hot(&self) -> Option<GuiId> {
        self.hot
    }

    pub fn active(&self) -> Option<GuiId> {
        self.active
    }

    pub fn focused(&self) -> Option<GuiId> {
        self.focused
    }

    pub fn is_key_down(&self, key: KeyCode) -> bool {
        self.keys_down.contains(&key)
    }

    pub fn last_key_pressed(&self) -> Option<KeyCode> {
        self.last_key_pressed
    }

    pub fn clear_focus(&mut self) {
        self.focused = None;
    }

    pub fn set_focus(&mut self, id: GuiId) {
        self.focused = Some(id);
    }
}

/// Primary GUI state owned by the renderer/user layer.
#[derive(Debug, Default)]
pub struct GuiContext {
    draws: Vec<GuiQueuedDraw>,
    text_draws: Vec<GuiTextDraw>,
    draw_sequence: u64,
}

impl GuiContext {
    /// Initialize the GUI context.
    pub fn new() -> Self {
        Self {
            draws: Vec::new(),
            text_draws: Vec::new(),
            draw_sequence: 0,
        }
    }

    /// Register GUI resources or layer configurations.
    pub fn register_layer(&mut self, _layer: GuiLayer) {}

    /// Submit a draw call to be collected for this frame.
    pub fn submit_draw(&mut self, draw: GuiDraw) {
        let order = self.draw_sequence;
        self.draw_sequence = self.draw_sequence.wrapping_add(1);
        self.draws.push(GuiQueuedDraw { order, draw });
    }

    /// Submit a text draw call to be collected for this frame.
    pub fn submit_text(&mut self, draw: GuiTextDraw) {
        self.text_draws.push(draw);
    }

    /// Build a frame mesh by sorting by layer and grouping by texture id.
    pub fn build_frame(&mut self) -> GuiFrame {
        if self.draws.is_empty() && self.text_draws.is_empty() {
            self.draw_sequence = 0;
            return GuiFrame::default();
        }

        let text_draws = self.text_draws.drain(..).collect();

        if self.draws.is_empty() {
            self.draw_sequence = 0;
            return GuiFrame {
                batches: Vec::new(),
                text_draws,
            };
        }

        self.draws.sort_by(|a, b| {
            a.draw
                .layer
                .cmp(&b.draw.layer)
                .then_with(|| a.draw.texture_id.cmp(&b.draw.texture_id))
                .then_with(|| a.order.cmp(&b.order))
        });

        let mut batches: Vec<GuiBatchMesh> = Vec::new();
        let mut current_batch: Option<GuiBatch> = None;
        let mut current_mesh = GuiMesh::default();

        for queued in self.draws.drain(..) {
            let draw = queued.draw;
            let needs_new_batch = current_batch.as_ref().map_or(true, |batch| {
                batch.layer != draw.layer
                    || batch.texture_id != draw.texture_id
                    || batch.clip_rect != draw.clip_rect
            });

            if needs_new_batch {
                if let Some(batch) = current_batch.take() {
                    batches.push(GuiBatchMesh {
                        batch,
                        mesh: current_mesh,
                    });
                }

                current_mesh = GuiMesh::default();
                current_batch = Some(GuiBatch {
                    layer: draw.layer,
                    texture_id: draw.texture_id,
                    index_range: 0..0,
                    clip_rect: draw.clip_rect,
                });
            }

            let base_vertex = current_mesh.vertices.len() as u32;
            current_mesh
                .vertices
                .extend(draw.quad.vertices().into_iter().map(|vertex| GuiVertex {
                    position: vertex.position,
                    uv: vertex.uv,
                    color: vertex.color,
                    texture_id: draw.texture_id.unwrap_or(GUI_NO_TEXTURE_ID),
                    _padding: [0; 3],
                }));
            current_mesh.indices.extend_from_slice(&[
                base_vertex,
                base_vertex + 1,
                base_vertex + 2,
                base_vertex + 2,
                base_vertex + 3,
                base_vertex,
            ]);

            if let Some(batch) = current_batch.as_mut() {
                let end = current_mesh.indices.len() as u32;
                if batch.index_range.end == 0 {
                    batch.index_range = (end - 6)..end;
                } else {
                    batch.index_range.end = end;
                }
            }
        }

        if let Some(batch) = current_batch.take() {
            batches.push(GuiBatchMesh {
                batch,
                mesh: current_mesh,
            });
        }

        self.draw_sequence = 0;
        GuiFrame {
            batches,
            text_draws,
        }
    }

    pub fn submit_menu_bar(
        &mut self,
        menu_bar: &MenuBar,
        options: &MenuBarRenderOptions,
    ) -> MenuBarLayout {
        menu_bar.submit_to_draw_list(self, options)
    }

    pub fn submit_menu_popup(
        &mut self,
        menu: &MenuPopup,
        options: &MenuPopupRenderOptions,
    ) -> MenuPopupLayout {
        menu.submit_to_draw_list(self, options)
    }

    pub fn submit_sliders(
        &mut self,
        sliders: &[Slider],
        options: &SliderRenderOptions,
    ) -> SliderLayout {
        let metrics = &options.metrics;
        let colors = &options.colors;
        let position = options.position;
        let viewport = options.viewport;

        let max_label_width = sliders
            .iter()
            .map(|slider| text_width(&slider.label, metrics.char_width))
            .fold(0.0_f32, f32::max)
            .max(metrics.min_label_width);

        let formatted_values: Vec<String> = sliders
            .iter()
            .map(|slider| {
                if slider.show_value {
                    format!("{:.2}", slider.value)
                } else {
                    String::new()
                }
            })
            .collect();

        let max_value_width = formatted_values
            .iter()
            .map(|value| text_width(value, metrics.char_width))
            .fold(0.0_f32, f32::max)
            .max(metrics.min_value_width);

        let mut layout = SliderLayout::default();
        let track_start_x = position[0] + metrics.padding[0] + max_label_width + metrics.label_gap;
        let track_end_x = position[0] + options.size[0]
            - metrics.padding[0]
            - max_value_width
            - metrics.value_gap;
        let track_width = (track_end_x - track_start_x).max(1.0);

        for (index, slider) in sliders.iter().enumerate() {
            let item_y = position[1]
                + metrics.padding[1]
                + index as f32 * (metrics.item_height + metrics.item_gap);
            let item_rect = MenuRect::from_position_size(
                [position[0] + metrics.padding[0], item_y],
                [
                    options.size[0] - metrics.padding[0] * 2.0,
                    metrics.item_height,
                ],
            );

            let clamped_value = if slider.max > slider.min {
                slider.value.clamp(slider.min, slider.max)
            } else {
                slider.min
            };
            let t = if (slider.max - slider.min).abs() < f32::EPSILON {
                0.0
            } else {
                (clamped_value - slider.min) / (slider.max - slider.min)
            };
            let knob_center_x = track_start_x + track_width * t;
            let track_y = item_y + (metrics.item_height - metrics.track_height) * 0.5;
            let knob_y = item_y + (metrics.item_height - metrics.knob_height) * 0.5;

            let track_rect = MenuRect::from_position_size(
                [track_start_x, track_y],
                [track_width, metrics.track_height],
            );
            let knob_rect = MenuRect::from_position_size(
                [knob_center_x - metrics.knob_width * 0.5, knob_y],
                [metrics.knob_width, metrics.knob_height],
            );

            let is_hovered = options.state.hovered == Some(slider.id);
            let is_active = options.state.active == Some(slider.id);
            let enabled = slider.enabled;

            let track_color = if !enabled {
                colors.disabled
            } else if is_active {
                colors.track_active
            } else if is_hovered {
                colors.track_hover
            } else {
                colors.track
            };

            let knob_color = if !enabled {
                colors.disabled
            } else if is_active {
                colors.knob_active
            } else if is_hovered {
                colors.knob_hover
            } else {
                colors.knob
            };

            self.submit_draw(GuiDraw::new(
                options.layer,
                None,
                quad_from_pixels(
                    [track_rect.min[0], track_rect.min[1]],
                    [track_width, metrics.track_height],
                    track_color,
                    viewport,
                ),
            ));

            self.submit_draw(GuiDraw::new(
                options.layer,
                None,
                quad_from_pixels(
                    [knob_rect.min[0], knob_rect.min[1]],
                    [metrics.knob_width, metrics.knob_height],
                    knob_color,
                    viewport,
                ),
            ));

            let label_pos = [item_rect.min[0], item_rect.min[1] + metrics.text_offset[1]];
            self.submit_text(GuiTextDraw {
                text: slider.label.clone(),
                position: label_pos,
                color: if enabled {
                    colors.label
                } else {
                    colors.disabled
                },
                scale: metrics.font_scale,
            });

            if slider.show_value {
                let value_text = formatted_values.get(index).cloned().unwrap_or_default();
                let value_pos = [
                    track_end_x + metrics.value_gap,
                    item_rect.min[1] + metrics.text_offset[1],
                ];
                self.submit_text(GuiTextDraw {
                    text: value_text,
                    position: value_pos,
                    color: if enabled {
                        colors.value
                    } else {
                        colors.disabled
                    },
                    scale: metrics.font_scale,
                });
            }

            layout.items.push(SliderItemLayout {
                id: slider.id,
                track_rect,
                knob_rect,
                value: clamped_value,
                min: slider.min,
                max: slider.max,
                enabled,
            });
        }

        layout
    }

    pub fn submit_panel(
        &mut self,
        panel: &Panel,
        state: &mut PanelState,
        options: &PanelRenderOptions,
    ) -> PanelLayout {
        let viewport = options.viewport;
        let metrics = &options.metrics;
        let colors = &options.colors;
        let interaction = options.interaction;

        let initial_display_size = panel_display_size(
            state.size,
            metrics.title_bar_height,
            metrics.minimized_extra_height,
            state.minimized,
        );
        let initial_title_bar_rect = MenuRect::from_position_size(
            state.position,
            [initial_display_size[0], metrics.title_bar_height],
        );
        let initial_grip_pos = [state.position[0] + metrics.grip_padding, state.position[1]];
        let initial_grip_rect = MenuRect::from_position_size(initial_grip_pos, metrics.grip_size);
        let initial_drag_rect = if metrics.grip_size[0] > 0.0 {
            initial_grip_rect
        } else {
            initial_title_bar_rect
        };

        let button_size = metrics.button_size;
        let button_gap = metrics.button_gap;
        let button_y = state.position[1] + (metrics.title_bar_height - button_size[1]) * 0.5;
        let buttons_right = state.position[0] + initial_display_size[0] - metrics.button_padding;
        let close_button_pos = [buttons_right - button_size[0], button_y];
        let minimize_button_pos = [buttons_right - button_size[0] * 2.0 - button_gap, button_y];
        let initial_close_rect = MenuRect::from_position_size(close_button_pos, button_size);
        let initial_minimize_rect = MenuRect::from_position_size(minimize_button_pos, button_size);

        let resize_edge = if state.closed || state.minimized {
            None
        } else {
            resize_edge_for_cursor(
                interaction.cursor,
                state.position,
                initial_display_size,
                metrics.resize_margin,
            )
        };

        if interaction.mouse_pressed && !state.closed {
            if options.allow_close && point_in_rect(interaction.cursor, initial_close_rect) {
                state.closed = true;
            } else if options.allow_minimize
                && point_in_rect(interaction.cursor, initial_minimize_rect)
            {
                state.minimized = !state.minimized;
            } else if let Some(edge) = resize_edge {
                state.resize_active = Some(edge);
                state.resize_anchor = interaction.cursor;
                state.resize_start_pos = state.position;
                state.resize_start_size = state.size;
            } else if point_in_rect(interaction.cursor, initial_drag_rect) {
                state.drag_active = true;
                state.drag_offset = sub2(interaction.cursor, state.position);
            }
        }

        if !interaction.mouse_down {
            state.drag_active = false;
            state.resize_active = None;
        }

        if state.drag_active && interaction.mouse_down {
            state.position = sub2(interaction.cursor, state.drag_offset);
        }

        if let Some(edge) = state.resize_active {
            if interaction.mouse_down {
                let delta = sub2(interaction.cursor, state.resize_anchor);
                let (position, size) = apply_resize(
                    edge,
                    state.resize_start_pos,
                    state.resize_start_size,
                    delta,
                    metrics.min_size,
                );
                state.position = position;
                state.size = size;
            }
        }

        if state.closed {
            return PanelLayout::closed(state.position, state.size);
        }

        let display_size = panel_display_size(
            state.size,
            metrics.title_bar_height,
            metrics.minimized_extra_height,
            state.minimized,
        );
        let title_bar_rect = MenuRect::from_position_size(
            state.position,
            [display_size[0], metrics.title_bar_height],
        );
        let title_bar_hovered = !state.closed && point_in_rect(interaction.cursor, title_bar_rect);
        let button_y = state.position[1] + (metrics.title_bar_height - button_size[1]) * 0.5;
        let buttons_right = state.position[0] + display_size[0] - metrics.button_padding;
        let close_button_pos = [buttons_right - button_size[0], button_y];
        let minimize_button_pos = [buttons_right - button_size[0] * 2.0 - button_gap, button_y];
        let close_rect = MenuRect::from_position_size(close_button_pos, button_size);
        let minimize_rect = MenuRect::from_position_size(minimize_button_pos, button_size);

        let display_rect = MenuRect::from_position_size(state.position, display_size);
        let content_rect = if state.minimized {
            MenuRect::from_position_size(state.position, [display_size[0], 0.0])
        } else {
            MenuRect::from_position_size(
                [
                    state.position[0],
                    state.position[1] + metrics.title_bar_height,
                ],
                [
                    display_size[0],
                    (display_size[1] - metrics.title_bar_height).max(0.0),
                ],
            )
        };

        if options.show_shadow {
            let shadow_pos = add2(state.position, metrics.shadow_offset);
            self.submit_draw(GuiDraw::new(
                options.layer,
                None,
                quad_from_pixels(shadow_pos, display_size, colors.shadow, viewport),
            ));
        }

        if options.show_outline {
            let outline_pos = sub2(
                state.position,
                [metrics.outline_thickness, metrics.outline_thickness],
            );
            let outline_size = add2(
                display_size,
                [
                    metrics.outline_thickness * 2.0,
                    metrics.outline_thickness * 2.0,
                ],
            );
            self.submit_draw(GuiDraw::new(
                options.layer,
                None,
                quad_from_pixels(outline_pos, outline_size, colors.outline, viewport),
            ));
        }

        let title_bar_color = if interaction.mouse_down && title_bar_hovered {
            colors.title_bar_active
        } else if title_bar_hovered {
            colors.title_bar_hover
        } else {
            colors.title_bar
        };

        self.submit_draw(GuiDraw::new(
            options.layer,
            None,
            quad_from_pixels(state.position, display_size, colors.background, viewport),
        ));

        self.submit_draw(GuiDraw::new(
            options.layer,
            None,
            quad_from_pixels(
                [title_bar_rect.min[0], title_bar_rect.min[1]],
                [display_size[0], metrics.title_bar_height],
                title_bar_color,
                viewport,
            ),
        ));

        if metrics.grip_size[0] > 0.0 {
            let grip_pos = [state.position[0] + metrics.grip_padding, state.position[1]];
            self.submit_draw(GuiDraw::new(
                options.layer,
                None,
                quad_from_pixels(grip_pos, metrics.grip_size, colors.grip, viewport),
            ));

            for index in 0..3 {
                let dot_pos = [
                    grip_pos[0] + 6.0 + index as f32 * 6.0,
                    grip_pos[1] + (metrics.title_bar_height - 6.0) * 0.5,
                ];
                self.submit_draw(GuiDraw::new(
                    options.layer,
                    None,
                    quad_from_pixels(dot_pos, [2.0, 6.0], colors.grip_dots, viewport),
                ));
            }
        }

        if options.allow_minimize {
            self.submit_draw(GuiDraw::new(
                options.layer,
                None,
                quad_from_pixels(minimize_button_pos, button_size, colors.button, viewport),
            ));
            self.submit_text(GuiTextDraw {
                text: "–".to_string(),
                position: [minimize_button_pos[0] + 6.0, minimize_button_pos[1] + 2.0],
                color: colors.button_text,
                scale: metrics.button_text_scale,
            });
        }

        if options.allow_close {
            self.submit_draw(GuiDraw::new(
                options.layer,
                None,
                quad_from_pixels(close_button_pos, button_size, colors.button_close, viewport),
            ));
            self.submit_text(GuiTextDraw {
                text: "×".to_string(),
                position: [close_button_pos[0] + 5.0, close_button_pos[1] + 1.0],
                color: colors.button_text,
                scale: metrics.button_text_scale,
            });
        }

        self.submit_text(GuiTextDraw {
            text: panel.title.clone(),
            position: [
                state.position[0] + metrics.title_text_offset[0],
                state.position[1] + metrics.title_text_offset[1],
            ],
            color: colors.title_text,
            scale: metrics.title_text_scale,
        });

        if !state.minimized && metrics.resize_handle_size > 0.0 {
            let handle_size = metrics.resize_handle_size;
            let edge_length = metrics.resize_edge_length;
            let edge_thickness = metrics.resize_edge_thickness.max(1.0);
            let left = state.position[0];
            let top = state.position[1];
            let right = state.position[0] + display_size[0];
            let bottom = state.position[1] + display_size[1];
            let center_x = state.position[0] + (display_size[0] - edge_length) * 0.5;
            let center_y = state.position[1] + (display_size[1] - edge_length) * 0.5;

            for (pos, size) in [
                ([left, top], [handle_size, handle_size]),
                ([right - handle_size, top], [handle_size, handle_size]),
                ([left, bottom - handle_size], [handle_size, handle_size]),
                (
                    [right - handle_size, bottom - handle_size],
                    [handle_size, handle_size],
                ),
                ([center_x, top], [edge_length, edge_thickness]),
                (
                    [center_x, bottom - edge_thickness],
                    [edge_length, edge_thickness],
                ),
                ([left, center_y], [edge_thickness, edge_length]),
                (
                    [right - edge_thickness, center_y],
                    [edge_thickness, edge_length],
                ),
            ] {
                self.submit_draw(GuiDraw::new(
                    options.layer,
                    None,
                    quad_from_pixels(pos, size, colors.resize_handle, viewport),
                ));
            }
        }

        PanelLayout {
            title_bar_rect,
            content_rect,
            display_rect,
            close_rect: if options.allow_close {
                Some(close_rect)
            } else {
                None
            },
            minimize_rect: if options.allow_minimize {
                Some(minimize_rect)
            } else {
                None
            },
            minimized: state.minimized,
            closed: state.closed,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct GuiQueuedDraw {
    order: u64,
    draw: GuiDraw,
}

/// Minimal draw submission payload for GUI rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuiDraw {
    pub layer: GuiLayer,
    pub texture_id: Option<u32>,
    pub quad: GuiQuad,
    pub clip_rect: Option<GuiClipRect>,
}

impl GuiDraw {
    pub fn new(layer: GuiLayer, texture_id: Option<u32>, quad: GuiQuad) -> Self {
        Self {
            layer,
            texture_id,
            quad,
            clip_rect: None,
        }
    }

    pub fn with_clip_rect(
        layer: GuiLayer,
        texture_id: Option<u32>,
        quad: GuiQuad,
        clip_rect: GuiClipRect,
    ) -> Self {
        Self {
            layer,
            texture_id,
            quad,
            clip_rect: Some(clip_rect),
        }
    }
}

/// GUI compositing layers, ordered back-to-front by convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GuiLayer {
    Background,
    World,
    Overlay,
}

/// A single GUI quad (two triangles) worth of data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuiQuad {
    pub positions: [[f32; 2]; 4],
    pub uvs: [[f32; 2]; 4],
    pub color: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct GuiTextDraw {
    pub text: String,
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub scale: f32,
}

impl GuiQuad {
    pub fn vertices(&self) -> [GuiVertexData; 4] {
        [
            GuiVertexData {
                position: self.positions[0],
                uv: self.uvs[0],
                color: self.color,
            },
            GuiVertexData {
                position: self.positions[1],
                uv: self.uvs[1],
                color: self.color,
            },
            GuiVertexData {
                position: self.positions[2],
                uv: self.uvs[2],
                color: self.color,
            },
            GuiVertexData {
                position: self.positions[3],
                uv: self.uvs[3],
                color: self.color,
            },
        ]
    }
}

#[derive(Debug, Clone, Copy)]
struct GuiVertexData {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

/// Clip rectangle for GUI draw submissions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuiClipRect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl GuiClipRect {
    pub fn from_min_max(min: [f32; 2], max: [f32; 2]) -> Self {
        Self { min, max }
    }

    pub fn from_position_size(position: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            min: position,
            max: [position[0] + size[0], position[1] + size[1]],
        }
    }
}

/// A frame-ready GUI mesh plus batch metadata.
#[derive(Debug, Default)]
pub struct GuiFrame {
    pub batches: Vec<GuiBatchMesh>,
    pub text_draws: Vec<GuiTextDraw>,
}

/// Consecutive indices sharing the same layer and texture binding.
#[derive(Debug, Clone, PartialEq)]
pub struct GuiBatch {
    pub layer: GuiLayer,
    pub texture_id: Option<u32>,
    pub index_range: Range<u32>,
    pub clip_rect: Option<GuiClipRect>,
}

#[derive(Debug, Clone)]
pub struct GuiBatchMesh {
    pub batch: GuiBatch,
    pub mesh: GuiMesh,
}

#[derive(Debug, Clone)]
pub struct MenuBar {
    pub menus: Vec<Menu>,
}

impl MenuBar {
    pub fn submit_to_draw_list(
        &self,
        ctx: &mut GuiContext,
        options: &MenuBarRenderOptions,
    ) -> MenuBarLayout {
        let metrics = &options.metrics;
        let colors = &options.colors;
        let position = options.position;
        let viewport = options.viewport;

        let mut layout = MenuBarLayout::default();

        ctx.submit_draw(GuiDraw::new(
            options.layer,
            None,
            quad_from_pixels(
                position,
                [viewport[0] - position[0], metrics.bar_height],
                colors.bar_background,
                viewport,
            ),
        ));

        let mut cursor_x = position[0] + metrics.bar_padding[0];
        let menu_y = position[1];

        for (menu_index, menu) in self.menus.iter().enumerate() {
            let label_width = text_width(&menu.label, metrics.char_width);
            let tab_width = label_width + metrics.menu_padding[0] * 2.0;
            let tab_rect =
                MenuRect::from_position_size([cursor_x, menu_y], [tab_width, metrics.bar_height]);

            if options.state.open_menu == Some(menu_index) {
                ctx.submit_draw(GuiDraw::new(
                    options.layer,
                    None,
                    quad_from_pixels(
                        [tab_rect.min[0], tab_rect.min[1]],
                        [tab_width, metrics.bar_height],
                        colors.tab_active,
                        viewport,
                    ),
                ));
            } else if options.state.hovered_menu == Some(menu_index) {
                ctx.submit_draw(GuiDraw::new(
                    options.layer,
                    None,
                    quad_from_pixels(
                        [tab_rect.min[0], tab_rect.min[1]],
                        [tab_width, metrics.bar_height],
                        colors.tab_hover,
                        viewport,
                    ),
                ));
            }

            layout.menu_tabs.push(MenuTabLayout {
                menu_index,
                rect: tab_rect,
            });

            let text_pos = [
                cursor_x + metrics.menu_padding[0],
                menu_y + metrics.text_offset[1],
            ];
            ctx.submit_text(GuiTextDraw {
                text: menu.label.clone(),
                position: text_pos,
                color: colors.text,
                scale: metrics.font_scale,
            });

            cursor_x += tab_width + metrics.menu_gap;

            if options.state.open_menu == Some(menu_index) {
                let dropdown_width = menu_dropdown_width(menu, metrics);
                let dropdown_height = menu_dropdown_height(menu, metrics);
                let dropdown_pos = [tab_rect.min[0], menu_y + metrics.bar_height];
                let dropdown_rect =
                    MenuRect::from_position_size(dropdown_pos, [dropdown_width, dropdown_height]);

                layout.open_menu = Some(OpenMenuLayout {
                    menu_index,
                    rect: dropdown_rect,
                });

                ctx.submit_draw(GuiDraw::new(
                    options.layer,
                    None,
                    quad_from_pixels(
                        dropdown_pos,
                        [dropdown_width, dropdown_height],
                        colors.dropdown_background,
                        viewport,
                    ),
                ));

                let mut item_y = dropdown_pos[1] + metrics.dropdown_padding[1];

                for (item_index, item) in menu.items.iter().enumerate() {
                    if item.is_separator {
                        let line_y = item_y + metrics.separator_padding;
                        ctx.submit_draw(GuiDraw::new(
                            options.layer,
                            None,
                            quad_from_pixels(
                                [dropdown_pos[0] + metrics.dropdown_padding[0], line_y],
                                [
                                    dropdown_width - metrics.dropdown_padding[0] * 2.0,
                                    metrics.separator_thickness,
                                ],
                                colors.separator,
                                viewport,
                            ),
                        ));
                        item_y += metrics.separator_thickness + metrics.separator_padding * 2.0;
                        continue;
                    }

                    let item_rect = MenuRect::from_position_size(
                        [dropdown_pos[0], item_y],
                        [dropdown_width, metrics.item_height],
                    );
                    let text_color = if item.enabled {
                        colors.text
                    } else {
                        colors.disabled_text
                    };

                    if item.enabled && options.state.hovered_item == Some((menu_index, item_index))
                    {
                        ctx.submit_draw(GuiDraw::new(
                            options.layer,
                            None,
                            quad_from_pixels(
                                [item_rect.min[0], item_rect.min[1]],
                                [dropdown_width, metrics.item_height],
                                colors.item_hover,
                                viewport,
                            ),
                        ));
                    }

                    if item.checked {
                        let check_pos = [
                            item_rect.min[0] + metrics.item_padding[0],
                            item_rect.min[1] + metrics.text_offset[1],
                        ];
                        ctx.submit_text(GuiTextDraw {
                            text: "✓".to_string(),
                            position: check_pos,
                            color: colors.checked_text,
                            scale: metrics.font_scale,
                        });
                    }

                    let label_x =
                        item_rect.min[0] + metrics.item_padding[0] + metrics.checkmark_width;
                    let label_pos = [label_x, item_rect.min[1] + metrics.text_offset[1]];
                    ctx.submit_text(GuiTextDraw {
                        text: item.label.clone(),
                        position: label_pos,
                        color: text_color,
                        scale: metrics.font_scale,
                    });

                    if let Some(shortcut) = &item.shortcut {
                        let shortcut_width = text_width(shortcut, metrics.char_width);
                        let shortcut_pos = [
                            item_rect.max[0] - metrics.item_padding[0] - shortcut_width,
                            item_rect.min[1] + metrics.text_offset[1],
                        ];
                        ctx.submit_text(GuiTextDraw {
                            text: shortcut.clone(),
                            position: shortcut_pos,
                            color: text_color,
                            scale: metrics.font_scale,
                        });
                    }

                    if item.enabled {
                        if let Some(submenu_items) = item.submenu.as_ref() {
                            if options.state.open_submenu == Some((menu_index, item_index)) {
                                let submenu_width =
                                    menu_items_dropdown_width(submenu_items, metrics);
                                let submenu_height =
                                    menu_items_dropdown_height(submenu_items, metrics);
                                let submenu_pos = [item_rect.max[0], item_rect.min[1]];
                                let submenu_rect = MenuRect::from_position_size(
                                    submenu_pos,
                                    [submenu_width, submenu_height],
                                );

                                layout.open_submenu = Some(OpenSubmenuLayout {
                                    menu_index,
                                    item_index,
                                    rect: submenu_rect,
                                });

                                ctx.submit_draw(GuiDraw::new(
                                    options.layer,
                                    None,
                                    quad_from_pixels(
                                        submenu_pos,
                                        [submenu_width, submenu_height],
                                        colors.dropdown_background,
                                        viewport,
                                    ),
                                ));

                                let mut submenu_y = submenu_pos[1] + metrics.dropdown_padding[1];
                                for (submenu_index, submenu_item) in
                                    submenu_items.iter().enumerate()
                                {
                                    if submenu_item.is_separator {
                                        let line_y = submenu_y + metrics.separator_padding;
                                        ctx.submit_draw(GuiDraw::new(
                                            options.layer,
                                            None,
                                            quad_from_pixels(
                                                [
                                                    submenu_pos[0] + metrics.dropdown_padding[0],
                                                    line_y,
                                                ],
                                                [
                                                    submenu_width
                                                        - metrics.dropdown_padding[0] * 2.0,
                                                    metrics.separator_thickness,
                                                ],
                                                colors.separator,
                                                viewport,
                                            ),
                                        ));
                                        submenu_y += metrics.separator_thickness
                                            + metrics.separator_padding * 2.0;
                                        continue;
                                    }

                                    let submenu_item_rect = MenuRect::from_position_size(
                                        [submenu_pos[0], submenu_y],
                                        [submenu_width, metrics.item_height],
                                    );
                                    let submenu_text_color = if submenu_item.enabled {
                                        colors.text
                                    } else {
                                        colors.disabled_text
                                    };

                                    if submenu_item.enabled
                                        && options.state.hovered_item
                                            == Some((menu_index, submenu_index))
                                    {
                                        ctx.submit_draw(GuiDraw::new(
                                            options.layer,
                                            None,
                                            quad_from_pixels(
                                                [
                                                    submenu_item_rect.min[0],
                                                    submenu_item_rect.min[1],
                                                ],
                                                [submenu_width, metrics.item_height],
                                                colors.item_hover,
                                                viewport,
                                            ),
                                        ));
                                    }

                                    if submenu_item.checked {
                                        let check_pos = [
                                            submenu_item_rect.min[0] + metrics.item_padding[0],
                                            submenu_item_rect.min[1] + metrics.text_offset[1],
                                        ];
                                        ctx.submit_text(GuiTextDraw {
                                            text: "✓".to_string(),
                                            position: check_pos,
                                            color: colors.checked_text,
                                            scale: metrics.font_scale,
                                        });
                                    }

                                    let submenu_label_x = submenu_item_rect.min[0]
                                        + metrics.item_padding[0]
                                        + metrics.checkmark_width;
                                    let submenu_label_pos = [
                                        submenu_label_x,
                                        submenu_item_rect.min[1] + metrics.text_offset[1],
                                    ];
                                    ctx.submit_text(GuiTextDraw {
                                        text: submenu_item.label.clone(),
                                        position: submenu_label_pos,
                                        color: submenu_text_color,
                                        scale: metrics.font_scale,
                                    });

                                    if let Some(shortcut) = &submenu_item.shortcut {
                                        let shortcut_width =
                                            text_width(shortcut, metrics.char_width);
                                        let shortcut_pos = [
                                            submenu_item_rect.max[0]
                                                - metrics.item_padding[0]
                                                - shortcut_width,
                                            submenu_item_rect.min[1] + metrics.text_offset[1],
                                        ];
                                        ctx.submit_text(GuiTextDraw {
                                            text: shortcut.clone(),
                                            position: shortcut_pos,
                                            color: submenu_text_color,
                                            scale: metrics.font_scale,
                                        });
                                    }

                                    layout.item_rects.push(MenuItemLayout {
                                        menu_index,
                                        item_index: submenu_index,
                                        parent_item_index: Some(item_index),
                                        depth: 1,
                                        rect: submenu_item_rect,
                                        action_id: submenu_item.action_id,
                                        enabled: submenu_item.enabled,
                                        has_submenu: submenu_item.submenu.is_some(),
                                    });

                                    submenu_y += metrics.item_height;
                                }
                            }
                        }
                    }

                    layout.item_rects.push(MenuItemLayout {
                        menu_index,
                        item_index,
                        parent_item_index: None,
                        depth: 0,
                        rect: item_rect,
                        action_id: item.action_id,
                        enabled: item.enabled,
                        has_submenu: item.submenu.is_some(),
                    });

                    item_y += metrics.item_height;
                }
            }
        }

        layout
    }
}

#[derive(Debug, Clone)]
pub struct MenuPopup {
    pub items: Vec<MenuItem>,
}

impl MenuPopup {
    pub fn submit_to_draw_list(
        &self,
        ctx: &mut GuiContext,
        options: &MenuPopupRenderOptions,
    ) -> MenuPopupLayout {
        let metrics = &options.metrics;
        let colors = &options.colors;
        let viewport = options.viewport;

        let dropdown_width = menu_items_dropdown_width(&self.items, metrics);
        let dropdown_height = menu_items_dropdown_height(&self.items, metrics);
        let dropdown_pos = popup_anchor_position(options.anchor, [dropdown_width, dropdown_height]);
        let dropdown_rect =
            MenuRect::from_position_size(dropdown_pos, [dropdown_width, dropdown_height]);

        let mut layout = MenuPopupLayout {
            rect: dropdown_rect,
            ..Default::default()
        };

        ctx.submit_draw(GuiDraw::new(
            options.layer,
            None,
            quad_from_pixels(
                dropdown_pos,
                [dropdown_width, dropdown_height],
                colors.dropdown_background,
                viewport,
            ),
        ));

        let mut item_y = dropdown_pos[1] + metrics.dropdown_padding[1];

        for (item_index, item) in self.items.iter().enumerate() {
            if item.is_separator {
                let line_y = item_y + metrics.separator_padding;
                ctx.submit_draw(GuiDraw::new(
                    options.layer,
                    None,
                    quad_from_pixels(
                        [dropdown_pos[0] + metrics.dropdown_padding[0], line_y],
                        [
                            dropdown_width - metrics.dropdown_padding[0] * 2.0,
                            metrics.separator_thickness,
                        ],
                        colors.separator,
                        viewport,
                    ),
                ));
                item_y += metrics.separator_thickness + metrics.separator_padding * 2.0;
                continue;
            }

            let item_rect = MenuRect::from_position_size(
                [dropdown_pos[0], item_y],
                [dropdown_width, metrics.item_height],
            );
            let text_color = if item.enabled {
                colors.text
            } else {
                colors.disabled_text
            };

            if item.enabled
                && options.state.hovered_item
                    == Some(MenuPopupItemRef {
                        item_index,
                        parent_item_index: None,
                        depth: 0,
                    })
            {
                ctx.submit_draw(GuiDraw::new(
                    options.layer,
                    None,
                    quad_from_pixels(
                        [item_rect.min[0], item_rect.min[1]],
                        [dropdown_width, metrics.item_height],
                        colors.item_hover,
                        viewport,
                    ),
                ));
            }

            if item.checked {
                let check_pos = [
                    item_rect.min[0] + metrics.item_padding[0],
                    item_rect.min[1] + metrics.text_offset[1],
                ];
                ctx.submit_text(GuiTextDraw {
                    text: "✓".to_string(),
                    position: check_pos,
                    color: colors.checked_text,
                    scale: metrics.font_scale,
                });
            }

            let label_x = item_rect.min[0] + metrics.item_padding[0] + metrics.checkmark_width;
            let label_pos = [label_x, item_rect.min[1] + metrics.text_offset[1]];
            ctx.submit_text(GuiTextDraw {
                text: item.label.clone(),
                position: label_pos,
                color: text_color,
                scale: metrics.font_scale,
            });

            if let Some(shortcut) = &item.shortcut {
                let shortcut_width = text_width(shortcut, metrics.char_width);
                let shortcut_pos = [
                    item_rect.max[0] - metrics.item_padding[0] - shortcut_width,
                    item_rect.min[1] + metrics.text_offset[1],
                ];
                ctx.submit_text(GuiTextDraw {
                    text: shortcut.clone(),
                    position: shortcut_pos,
                    color: text_color,
                    scale: metrics.font_scale,
                });
            }

            if item.enabled {
                if let Some(submenu_items) = item.submenu.as_ref() {
                    if options.state.open_submenu == Some(item_index) {
                        let submenu_width = menu_items_dropdown_width(submenu_items, metrics);
                        let submenu_height = menu_items_dropdown_height(submenu_items, metrics);
                        let submenu_pos = [item_rect.max[0], item_rect.min[1]];
                        let submenu_rect = MenuRect::from_position_size(
                            submenu_pos,
                            [submenu_width, submenu_height],
                        );

                        layout.open_submenu = Some(MenuPopupSubmenuLayout {
                            item_index,
                            rect: submenu_rect,
                        });

                        ctx.submit_draw(GuiDraw::new(
                            options.layer,
                            None,
                            quad_from_pixels(
                                submenu_pos,
                                [submenu_width, submenu_height],
                                colors.dropdown_background,
                                viewport,
                            ),
                        ));

                        let mut submenu_y = submenu_pos[1] + metrics.dropdown_padding[1];
                        for (submenu_index, submenu_item) in submenu_items.iter().enumerate() {
                            if submenu_item.is_separator {
                                let line_y = submenu_y + metrics.separator_padding;
                                ctx.submit_draw(GuiDraw::new(
                                    options.layer,
                                    None,
                                    quad_from_pixels(
                                        [submenu_pos[0] + metrics.dropdown_padding[0], line_y],
                                        [
                                            submenu_width - metrics.dropdown_padding[0] * 2.0,
                                            metrics.separator_thickness,
                                        ],
                                        colors.separator,
                                        viewport,
                                    ),
                                ));
                                submenu_y +=
                                    metrics.separator_thickness + metrics.separator_padding * 2.0;
                                continue;
                            }

                            let submenu_item_rect = MenuRect::from_position_size(
                                [submenu_pos[0], submenu_y],
                                [submenu_width, metrics.item_height],
                            );
                            let submenu_text_color = if submenu_item.enabled {
                                colors.text
                            } else {
                                colors.disabled_text
                            };

                            if submenu_item.enabled
                                && options.state.hovered_item
                                    == Some(MenuPopupItemRef {
                                        item_index: submenu_index,
                                        parent_item_index: Some(item_index),
                                        depth: 1,
                                    })
                            {
                                ctx.submit_draw(GuiDraw::new(
                                    options.layer,
                                    None,
                                    quad_from_pixels(
                                        [submenu_item_rect.min[0], submenu_item_rect.min[1]],
                                        [submenu_width, metrics.item_height],
                                        colors.item_hover,
                                        viewport,
                                    ),
                                ));
                            }

                            if submenu_item.checked {
                                let check_pos = [
                                    submenu_item_rect.min[0] + metrics.item_padding[0],
                                    submenu_item_rect.min[1] + metrics.text_offset[1],
                                ];
                                ctx.submit_text(GuiTextDraw {
                                    text: "✓".to_string(),
                                    position: check_pos,
                                    color: colors.checked_text,
                                    scale: metrics.font_scale,
                                });
                            }

                            let submenu_label_x = submenu_item_rect.min[0]
                                + metrics.item_padding[0]
                                + metrics.checkmark_width;
                            let submenu_label_pos = [
                                submenu_label_x,
                                submenu_item_rect.min[1] + metrics.text_offset[1],
                            ];
                            ctx.submit_text(GuiTextDraw {
                                text: submenu_item.label.clone(),
                                position: submenu_label_pos,
                                color: submenu_text_color,
                                scale: metrics.font_scale,
                            });

                            if let Some(shortcut) = &submenu_item.shortcut {
                                let shortcut_width = text_width(shortcut, metrics.char_width);
                                let shortcut_pos = [
                                    submenu_item_rect.max[0]
                                        - metrics.item_padding[0]
                                        - shortcut_width,
                                    submenu_item_rect.min[1] + metrics.text_offset[1],
                                ];
                                ctx.submit_text(GuiTextDraw {
                                    text: shortcut.clone(),
                                    position: shortcut_pos,
                                    color: submenu_text_color,
                                    scale: metrics.font_scale,
                                });
                            }

                            layout.item_rects.push(MenuPopupItemLayout {
                                item_index: submenu_index,
                                parent_item_index: Some(item_index),
                                depth: 1,
                                rect: submenu_item_rect,
                                action_id: submenu_item.action_id,
                                enabled: submenu_item.enabled,
                                has_submenu: submenu_item.submenu.is_some(),
                            });

                            submenu_y += metrics.item_height;
                        }
                    }
                }
            }

            layout.item_rects.push(MenuPopupItemLayout {
                item_index,
                parent_item_index: None,
                depth: 0,
                rect: item_rect,
                action_id: item.action_id,
                enabled: item.enabled,
                has_submenu: item.submenu.is_some(),
            });

            item_y += metrics.item_height;
        }

        layout
    }
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub label: String,
    pub items: Vec<MenuItem>,
}

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub enabled: bool,
    pub shortcut: Option<String>,
    pub checked: bool,
    pub action_id: Option<u32>,
    pub is_separator: bool,
    pub submenu: Option<Vec<MenuItem>>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            shortcut: None,
            checked: false,
            action_id: None,
            is_separator: false,
            submenu: None,
        }
    }

    pub fn with_submenu(mut self, items: Vec<MenuItem>) -> Self {
        self.submenu = Some(items);
        self
    }

    pub fn separator() -> Self {
        Self {
            label: String::new(),
            enabled: false,
            shortcut: None,
            checked: false,
            action_id: None,
            is_separator: true,
            submenu: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Slider {
    pub id: u32,
    pub label: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub enabled: bool,
    pub show_value: bool,
}

impl Slider {
    pub fn new(id: u32, label: impl Into<String>, min: f32, max: f32, value: f32) -> Self {
        Self {
            id,
            label: label.into(),
            value,
            min,
            max,
            enabled: true,
            show_value: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SliderRenderOptions {
    pub viewport: [f32; 2],
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub layer: GuiLayer,
    pub metrics: SliderMetrics,
    pub colors: SliderColors,
    pub state: SliderState,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SliderState {
    pub hovered: Option<u32>,
    pub active: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct SliderMetrics {
    pub item_height: f32,
    pub item_gap: f32,
    pub padding: [f32; 2],
    pub track_height: f32,
    pub knob_width: f32,
    pub knob_height: f32,
    pub label_gap: f32,
    pub value_gap: f32,
    pub min_label_width: f32,
    pub min_value_width: f32,
    pub char_width: f32,
    pub font_scale: f32,
    pub text_offset: [f32; 2],
}

impl Default for SliderMetrics {
    fn default() -> Self {
        Self {
            item_height: 30.0,
            item_gap: 10.0,
            padding: [12.0, 12.0],
            track_height: 6.0,
            knob_width: 14.0,
            knob_height: 18.0,
            label_gap: 12.0,
            value_gap: 10.0,
            min_label_width: 90.0,
            min_value_width: 48.0,
            char_width: 7.2,
            font_scale: 1.0,
            text_offset: [0.0, 7.0],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SliderColors {
    pub track: [f32; 4],
    pub track_hover: [f32; 4],
    pub track_active: [f32; 4],
    pub knob: [f32; 4],
    pub knob_hover: [f32; 4],
    pub knob_active: [f32; 4],
    pub label: [f32; 4],
    pub value: [f32; 4],
    pub disabled: [f32; 4],
}

impl Default for SliderColors {
    fn default() -> Self {
        Self {
            track: [0.18, 0.2, 0.25, 0.7],
            track_hover: [0.22, 0.26, 0.32, 0.85],
            track_active: [0.25, 0.3, 0.4, 0.9],
            knob: [0.7, 0.8, 0.95, 0.95],
            knob_hover: [0.85, 0.9, 1.0, 1.0],
            knob_active: [0.45, 0.8, 1.0, 1.0],
            label: [0.9, 0.92, 0.96, 1.0],
            value: [0.75, 0.8, 0.9, 1.0],
            disabled: [0.4, 0.42, 0.46, 0.6],
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SliderLayout {
    pub items: Vec<SliderItemLayout>,
}

#[derive(Debug, Clone, Copy)]
pub struct SliderItemLayout {
    pub id: u32,
    pub track_rect: MenuRect,
    pub knob_rect: MenuRect,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct Panel {
    pub title: String,
}

impl Panel {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PanelRenderOptions {
    pub viewport: [f32; 2],
    pub layer: GuiLayer,
    pub interaction: PanelInteraction,
    pub metrics: PanelMetrics,
    pub colors: PanelColors,
    pub allow_close: bool,
    pub allow_minimize: bool,
    pub show_shadow: bool,
    pub show_outline: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PanelInteraction {
    pub cursor: [f32; 2],
    pub mouse_pressed: bool,
    pub mouse_down: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PanelMetrics {
    pub title_bar_height: f32,
    pub min_size: [f32; 2],
    pub button_size: [f32; 2],
    pub button_gap: f32,
    pub button_padding: f32,
    pub title_text_offset: [f32; 2],
    pub title_text_scale: f32,
    pub button_text_scale: f32,
    pub grip_size: [f32; 2],
    pub grip_padding: f32,
    pub resize_margin: f32,
    pub resize_handle_size: f32,
    pub resize_edge_length: f32,
    pub resize_edge_thickness: f32,
    pub shadow_offset: [f32; 2],
    pub outline_thickness: f32,
    pub minimized_extra_height: f32,
}

impl Default for PanelMetrics {
    fn default() -> Self {
        Self {
            title_bar_height: 32.0,
            min_size: [240.0, 180.0],
            button_size: [18.0, 18.0],
            button_gap: 6.0,
            button_padding: 12.0,
            title_text_offset: [36.0, 8.0],
            title_text_scale: 1.0,
            button_text_scale: 0.9,
            grip_size: [22.0, 32.0],
            grip_padding: 8.0,
            resize_margin: 8.0,
            resize_handle_size: 8.0,
            resize_edge_length: 18.0,
            resize_edge_thickness: 2.0,
            shadow_offset: [6.0, 6.0],
            outline_thickness: 1.0,
            minimized_extra_height: 6.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PanelColors {
    pub background: [f32; 4],
    pub title_bar: [f32; 4],
    pub title_bar_hover: [f32; 4],
    pub title_bar_active: [f32; 4],
    pub title_text: [f32; 4],
    pub outline: [f32; 4],
    pub shadow: [f32; 4],
    pub button: [f32; 4],
    pub button_close: [f32; 4],
    pub button_text: [f32; 4],
    pub grip: [f32; 4],
    pub grip_dots: [f32; 4],
    pub resize_handle: [f32; 4],
}

impl Default for PanelColors {
    fn default() -> Self {
        Self {
            background: [0.12, 0.14, 0.18, 0.92],
            title_bar: [0.18, 0.2, 0.26, 0.9],
            title_bar_hover: [0.22, 0.26, 0.34, 0.92],
            title_bar_active: [0.28, 0.32, 0.4, 0.95],
            title_text: [0.95, 0.97, 1.0, 1.0],
            outline: [0.02, 0.05, 0.08, 0.9],
            shadow: [0.0, 0.0, 0.0, 0.35],
            button: [0.2, 0.24, 0.32, 0.9],
            button_close: [0.28, 0.2, 0.22, 0.9],
            button_text: [0.9, 0.93, 1.0, 1.0],
            grip: [0.12, 0.14, 0.18, 0.6],
            grip_dots: [0.3, 0.36, 0.5, 0.8],
            resize_handle: [0.32, 0.38, 0.5, 0.8],
        }
    }
}

#[derive(Debug, Clone)]
pub struct PanelState {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub minimized: bool,
    pub closed: bool,
    drag_offset: [f32; 2],
    drag_active: bool,
    resize_active: Option<PanelResizeEdge>,
    resize_anchor: [f32; 2],
    resize_start_pos: [f32; 2],
    resize_start_size: [f32; 2],
}

impl PanelState {
    pub fn new(position: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            position,
            size,
            minimized: false,
            closed: false,
            drag_offset: [0.0, 0.0],
            drag_active: false,
            resize_active: None,
            resize_anchor: [0.0, 0.0],
            resize_start_pos: [0.0, 0.0],
            resize_start_size: [0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelResizeEdge {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy)]
pub struct PanelLayout {
    pub title_bar_rect: MenuRect,
    pub content_rect: MenuRect,
    pub display_rect: MenuRect,
    pub close_rect: Option<MenuRect>,
    pub minimize_rect: Option<MenuRect>,
    pub minimized: bool,
    pub closed: bool,
}

impl PanelLayout {
    fn closed(position: [f32; 2], size: [f32; 2]) -> Self {
        let rect = MenuRect::from_position_size(position, size);
        Self {
            title_bar_rect: rect,
            content_rect: rect,
            display_rect: rect,
            close_rect: None,
            minimize_rect: None,
            minimized: false,
            closed: true,
        }
    }

    pub fn show_content(&self) -> bool {
        !self.closed && !self.minimized
    }
}

#[derive(Debug, Clone)]
pub struct MenuBarRenderOptions {
    pub viewport: [f32; 2],
    pub position: [f32; 2],
    pub layer: GuiLayer,
    pub metrics: MenuLayoutMetrics,
    pub colors: MenuColors,
    pub state: MenuBarState,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MenuBarState {
    pub open_menu: Option<usize>,
    pub hovered_menu: Option<usize>,
    pub hovered_item: Option<(usize, usize)>,
    pub open_submenu: Option<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct MenuPopupRenderOptions {
    pub viewport: [f32; 2],
    pub anchor: MenuPopupAnchor,
    pub layer: GuiLayer,
    pub metrics: MenuLayoutMetrics,
    pub colors: MenuColors,
    pub state: MenuPopupState,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MenuPopupState {
    pub hovered_item: Option<MenuPopupItemRef>,
    pub open_submenu: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuPopupItemRef {
    pub item_index: usize,
    pub parent_item_index: Option<usize>,
    pub depth: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum MenuPopupAnchor {
    Position([f32; 2]),
    Rect {
        rect: MenuRect,
        align: MenuPopupAlign,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum MenuPopupAlign {
    BelowLeft,
    BelowRight,
    AboveLeft,
    AboveRight,
}

#[derive(Debug, Clone, Copy)]
pub struct MenuLayoutMetrics {
    pub bar_height: f32,
    pub bar_padding: [f32; 2],
    pub menu_padding: [f32; 2],
    pub menu_gap: f32,
    pub item_height: f32,
    pub item_padding: [f32; 2],
    pub dropdown_padding: [f32; 2],
    pub separator_thickness: f32,
    pub separator_padding: f32,
    pub shortcut_gap: f32,
    pub checkmark_width: f32,
    pub char_width: f32,
    pub font_scale: f32,
    pub text_offset: [f32; 2],
}

impl Default for MenuLayoutMetrics {
    fn default() -> Self {
        Self {
            bar_height: 28.0,
            bar_padding: [8.0, 4.0],
            menu_padding: [10.0, 6.0],
            menu_gap: 6.0,
            item_height: 26.0,
            item_padding: [12.0, 6.0],
            dropdown_padding: [6.0, 6.0],
            separator_thickness: 1.0,
            separator_padding: 6.0,
            shortcut_gap: 18.0,
            checkmark_width: 14.0,
            char_width: 7.5,
            font_scale: 1.0,
            text_offset: [0.0, 7.0],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MenuColors {
    pub bar_background: [f32; 4],
    pub tab_hover: [f32; 4],
    pub tab_active: [f32; 4],
    pub dropdown_background: [f32; 4],
    pub item_hover: [f32; 4],
    pub separator: [f32; 4],
    pub text: [f32; 4],
    pub disabled_text: [f32; 4],
    pub checked_text: [f32; 4],
}

impl Default for MenuColors {
    fn default() -> Self {
        Self {
            bar_background: [0.08, 0.09, 0.11, 0.98],
            tab_hover: [0.18, 0.2, 0.26, 0.98],
            tab_active: [0.22, 0.24, 0.3, 0.98],
            dropdown_background: [0.12, 0.13, 0.16, 0.96],
            item_hover: [0.2, 0.24, 0.32, 0.92],
            separator: [0.24, 0.26, 0.3, 0.8],
            text: [0.9, 0.92, 0.96, 1.0],
            disabled_text: [0.55, 0.58, 0.62, 1.0],
            checked_text: [0.35, 0.8, 0.45, 1.0],
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct MenuBarLayout {
    pub menu_tabs: Vec<MenuTabLayout>,
    pub item_rects: Vec<MenuItemLayout>,
    pub open_menu: Option<OpenMenuLayout>,
    pub open_submenu: Option<OpenSubmenuLayout>,
}

#[derive(Debug, Default, Clone)]
pub struct MenuPopupLayout {
    pub rect: MenuRect,
    pub item_rects: Vec<MenuPopupItemLayout>,
    pub open_submenu: Option<MenuPopupSubmenuLayout>,
}

#[derive(Debug, Clone, Copy)]
pub struct MenuTabLayout {
    pub menu_index: usize,
    pub rect: MenuRect,
}

#[derive(Debug, Clone, Copy)]
pub struct OpenMenuLayout {
    pub menu_index: usize,
    pub rect: MenuRect,
}

#[derive(Debug, Clone, Copy)]
pub struct OpenSubmenuLayout {
    pub menu_index: usize,
    pub item_index: usize,
    pub rect: MenuRect,
}

#[derive(Debug, Clone, Copy)]
pub struct MenuPopupSubmenuLayout {
    pub item_index: usize,
    pub rect: MenuRect,
}

#[derive(Debug, Clone, Copy)]
pub struct MenuItemLayout {
    pub menu_index: usize,
    pub item_index: usize,
    pub parent_item_index: Option<usize>,
    pub depth: usize,
    pub rect: MenuRect,
    pub action_id: Option<u32>,
    pub enabled: bool,
    pub has_submenu: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct MenuPopupItemLayout {
    pub item_index: usize,
    pub parent_item_index: Option<usize>,
    pub depth: usize,
    pub rect: MenuRect,
    pub action_id: Option<u32>,
    pub enabled: bool,
    pub has_submenu: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct MenuRect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl MenuRect {
    pub fn from_position_size(position: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            min: position,
            max: [position[0] + size[0], position[1] + size[1]],
        }
    }
}

impl Default for MenuRect {
    fn default() -> Self {
        Self {
            min: [0.0, 0.0],
            max: [0.0, 0.0],
        }
    }
}

fn popup_anchor_position(anchor: MenuPopupAnchor, size: [f32; 2]) -> [f32; 2] {
    match anchor {
        MenuPopupAnchor::Position(position) => position,
        MenuPopupAnchor::Rect { rect, align } => match align {
            MenuPopupAlign::BelowLeft => [rect.min[0], rect.max[1]],
            MenuPopupAlign::BelowRight => [rect.max[0] - size[0], rect.max[1]],
            MenuPopupAlign::AboveLeft => [rect.min[0], rect.min[1] - size[1]],
            MenuPopupAlign::AboveRight => [rect.max[0] - size[0], rect.min[1] - size[1]],
        },
    }
}

fn text_width(text: &str, char_width: f32) -> f32 {
    text.chars().count() as f32 * char_width
}

fn menu_dropdown_width(menu: &Menu, metrics: &MenuLayoutMetrics) -> f32 {
    menu_items_dropdown_width(&menu.items, metrics)
}

fn menu_dropdown_height(menu: &Menu, metrics: &MenuLayoutMetrics) -> f32 {
    menu_items_dropdown_height(&menu.items, metrics)
}

fn menu_items_dropdown_width(items: &[MenuItem], metrics: &MenuLayoutMetrics) -> f32 {
    let mut max_width: f32 = 0.0;
    for item in items {
        if item.is_separator {
            continue;
        }
        let label_width = text_width(&item.label, metrics.char_width);
        let shortcut_width = item
            .shortcut
            .as_ref()
            .map(|shortcut| text_width(shortcut, metrics.char_width))
            .unwrap_or(0.0);
        let shortcut_gap = if shortcut_width > 0.0 {
            metrics.shortcut_gap
        } else {
            0.0
        };
        let width = metrics.item_padding[0] * 2.0
            + metrics.checkmark_width
            + label_width
            + shortcut_gap
            + shortcut_width;
        max_width = max_width.max(width);
    }
    max_width.max(metrics.menu_padding[0] * 2.0 + 60.0)
}

fn menu_items_dropdown_height(items: &[MenuItem], metrics: &MenuLayoutMetrics) -> f32 {
    let mut height = metrics.dropdown_padding[1] * 2.0;
    for item in items {
        if item.is_separator {
            height += metrics.separator_thickness + metrics.separator_padding * 2.0;
        } else {
            height += metrics.item_height;
        }
    }
    height
}

fn add2(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub2(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn panel_display_size(
    size: [f32; 2],
    title_bar_height: f32,
    minimized_extra_height: f32,
    minimized: bool,
) -> [f32; 2] {
    if minimized {
        [size[0], title_bar_height + minimized_extra_height]
    } else {
        size
    }
}

fn point_in_rect(point: [f32; 2], rect: MenuRect) -> bool {
    point[0] >= rect.min[0]
        && point[0] <= rect.max[0]
        && point[1] >= rect.min[1]
        && point[1] <= rect.max[1]
}

fn resize_edge_for_cursor(
    cursor: [f32; 2],
    position: [f32; 2],
    size: [f32; 2],
    margin: f32,
) -> Option<PanelResizeEdge> {
    let min = position;
    let max = add2(position, size);
    let within_x = cursor[0] >= min[0] - margin && cursor[0] <= max[0] + margin;
    let within_y = cursor[1] >= min[1] - margin && cursor[1] <= max[1] + margin;
    if !within_x || !within_y {
        return None;
    }

    let left = (cursor[0] - min[0]).abs() <= margin;
    let right = (cursor[0] - max[0]).abs() <= margin;
    let top = (cursor[1] - min[1]).abs() <= margin;
    let bottom = (cursor[1] - max[1]).abs() <= margin;

    match (left, right, top, bottom) {
        (true, _, true, _) => Some(PanelResizeEdge::TopLeft),
        (_, true, true, _) => Some(PanelResizeEdge::TopRight),
        (true, _, _, true) => Some(PanelResizeEdge::BottomLeft),
        (_, true, _, true) => Some(PanelResizeEdge::BottomRight),
        (true, _, _, _) => Some(PanelResizeEdge::Left),
        (_, true, _, _) => Some(PanelResizeEdge::Right),
        (_, _, true, _) => Some(PanelResizeEdge::Top),
        (_, _, _, true) => Some(PanelResizeEdge::Bottom),
        _ => None,
    }
}

fn apply_resize(
    edge: PanelResizeEdge,
    start_pos: [f32; 2],
    start_size: [f32; 2],
    cursor_delta: [f32; 2],
    min_size: [f32; 2],
) -> ([f32; 2], [f32; 2]) {
    let mut position = start_pos;
    let mut size = start_size;

    match edge {
        PanelResizeEdge::Left | PanelResizeEdge::TopLeft | PanelResizeEdge::BottomLeft => {
            position[0] += cursor_delta[0];
            size[0] -= cursor_delta[0];
        }
        PanelResizeEdge::Right | PanelResizeEdge::TopRight | PanelResizeEdge::BottomRight => {
            size[0] += cursor_delta[0];
        }
        _ => {}
    }

    match edge {
        PanelResizeEdge::Top | PanelResizeEdge::TopLeft | PanelResizeEdge::TopRight => {
            position[1] += cursor_delta[1];
            size[1] -= cursor_delta[1];
        }
        PanelResizeEdge::Bottom | PanelResizeEdge::BottomLeft | PanelResizeEdge::BottomRight => {
            size[1] += cursor_delta[1];
        }
        _ => {}
    }

    if size[0] < min_size[0] {
        let delta = min_size[0] - size[0];
        size[0] = min_size[0];
        if matches!(
            edge,
            PanelResizeEdge::Left | PanelResizeEdge::TopLeft | PanelResizeEdge::BottomLeft
        ) {
            position[0] -= delta;
        }
    }

    if size[1] < min_size[1] {
        let delta = min_size[1] - size[1];
        size[1] = min_size[1];
        if matches!(
            edge,
            PanelResizeEdge::Top | PanelResizeEdge::TopLeft | PanelResizeEdge::TopRight
        ) {
            position[1] -= delta;
        }
    }

    (position, size)
}

fn quad_from_pixels(
    position: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    viewport: [f32; 2],
) -> GuiQuad {
    let left = (position[0] / viewport[0]) * 2.0 - 1.0;
    let right = ((position[0] + size[0]) / viewport[0]) * 2.0 - 1.0;
    let top = 1.0 - (position[1] / viewport[1]) * 2.0;
    let bottom = 1.0 - ((position[1] + size[1]) / viewport[1]) * 2.0;

    GuiQuad {
        positions: [[left, top], [right, top], [right, bottom], [left, bottom]],
        uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        color,
    }
}
