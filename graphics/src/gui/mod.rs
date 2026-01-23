//! GUI rendering entry points.
//!
//! Expected entry points:
//! - **Initialization**: create a [`GuiContext`] tied to the renderer/device.
//! - **Registration**: register GUI layers, fonts, and resources up front.
//! - **Draw submission**: submit [`GuiDraw`] records each frame for rendering.
//! - **Frame build**: generate a sorted, batched mesh for GPU upload.

pub mod debug;
pub mod dock;

use std::ops::Range;

use crate::render::gui::{GuiMesh, GuiVertex};

const GUI_NO_TEXTURE_ID: u32 = u32::MAX;

/// Primary GUI state owned by the renderer/user layer.
#[derive(Debug, Default)]
pub struct GuiContext {
    draws: Vec<GuiDraw>,
    text_draws: Vec<GuiTextDraw>,
}

impl GuiContext {
    /// Initialize the GUI context.
    pub fn new() -> Self {
        Self {
            draws: Vec::new(),
            text_draws: Vec::new(),
        }
    }

    /// Register GUI resources or layer configurations.
    pub fn register_layer(&mut self, _layer: GuiLayer) {}

    /// Submit a draw call to be collected for this frame.
    pub fn submit_draw(&mut self, draw: GuiDraw) {
        self.draws.push(draw);
    }

    /// Submit a text draw call to be collected for this frame.
    pub fn submit_text(&mut self, draw: GuiTextDraw) {
        self.text_draws.push(draw);
    }

    /// Build a frame mesh by sorting by layer and grouping by texture id.
    pub fn build_frame(&mut self) -> GuiFrame {
        if self.draws.is_empty() && self.text_draws.is_empty() {
            return GuiFrame::default();
        }

        let text_draws = self.text_draws.drain(..).collect();

        if self.draws.is_empty() {
            return GuiFrame {
                batches: Vec::new(),
                text_draws,
            };
        }

        let mut indexed: Vec<(usize, GuiDraw)> = self.draws.drain(..).enumerate().collect();
        indexed.sort_by(|(a_index, a), (b_index, b)| {
            a.layer
                .cmp(&b.layer)
                .then_with(|| a.texture_id.cmp(&b.texture_id))
                .then_with(|| a_index.cmp(b_index))
        });

        let mut batches: Vec<GuiBatchMesh> = Vec::new();
        let mut current_batch: Option<GuiBatch> = None;
        let mut current_mesh = GuiMesh::default();

        for (_, draw) in indexed {
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

                    layout.item_rects.push(MenuItemLayout {
                        menu_index,
                        item_index,
                        rect: item_rect,
                        action_id: item.action_id,
                        enabled: item.enabled,
                    });

                    item_y += metrics.item_height;
                }
            }
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
        }
    }

    pub fn separator() -> Self {
        Self {
            label: String::new(),
            enabled: false,
            shortcut: None,
            checked: false,
            action_id: None,
            is_separator: true,
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
pub struct MenuItemLayout {
    pub menu_index: usize,
    pub item_index: usize,
    pub rect: MenuRect,
    pub action_id: Option<u32>,
    pub enabled: bool,
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

fn text_width(text: &str, char_width: f32) -> f32 {
    text.chars().count() as f32 * char_width
}

fn menu_dropdown_width(menu: &Menu, metrics: &MenuLayoutMetrics) -> f32 {
    let mut max_width: f32 = 0.0;
    for item in &menu.items {
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

fn menu_dropdown_height(menu: &Menu, metrics: &MenuLayoutMetrics) -> f32 {
    let mut height = metrics.dropdown_padding[1] * 2.0;
    for item in &menu.items {
        if item.is_separator {
            height += metrics.separator_thickness + metrics.separator_padding * 2.0;
        } else {
            height += metrics.item_height;
        }
    }
    height
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
