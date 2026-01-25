use glam::Vec2;
use meshi_graphics::gui::{
    GuiContext, GuiDraw, GuiLayer, GuiQuad, GuiTextDraw, MenuRect, Panel, PanelColors,
    PanelInteraction, PanelMetrics, PanelRenderOptions, PanelState, Slider, SliderColors,
    SliderLayout, SliderMetrics, SliderRenderOptions, SliderState,
};
use meshi_graphics::rdb::terrain::TerrainMutationOpKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusedInput {
    DbPath,
    Seed,
    Lod,
    GeneratorGraph,
}

pub struct TerrainEditorUiInput {
    pub cursor: Vec2,
    pub mouse_pressed: bool,
    pub mouse_down: bool,
    pub mouse_released: bool,
}

pub struct TerrainEditorUiData<'a> {
    pub viewport: Vec2,
    pub db_path: &'a str,
    pub db_dirty: bool,
    pub db_open: bool,
    pub chunk_keys: &'a [String],
    pub selected_chunk: Option<usize>,
    pub seed_input: &'a str,
    pub lod_input: &'a str,
    pub graph_id_input: &'a str,
    pub brush_tool: TerrainMutationOpKind,
    pub brush_radius: f32,
    pub brush_strength: f32,
}

pub struct TerrainEditorUiOutput {
    pub open_clicked: bool,
    pub save_clicked: bool,
    pub generate_clicked: bool,
    pub brush_apply_clicked: bool,
    pub select_chunk: Option<usize>,
    pub prev_chunk_clicked: bool,
    pub next_chunk_clicked: bool,
    pub focused_input: Option<FocusedInput>,
    pub brush_tool: Option<TerrainMutationOpKind>,
    pub brush_radius: Option<f32>,
    pub brush_strength: Option<f32>,
    pub ui_hovered: bool,
}

pub struct TerrainEditorUi {
    db_panel: PanelState,
    chunk_panel: PanelState,
    generation_panel: PanelState,
    brush_panel: PanelState,
    slider_layout: SliderLayout,
    active_slider: Option<u32>,
}

const SLIDER_RADIUS: u32 = 1;
const SLIDER_STRENGTH: u32 = 2;

impl TerrainEditorUi {
    pub fn new(_window_size: Vec2) -> Self {
        let margin = 16.0;
        let panel_width = 360.0;
        let panel_height = 210.0;
        Self {
            db_panel: PanelState::new([margin, margin], [panel_width, 200.0]),
            chunk_panel: PanelState::new([margin, margin + 216.0], [panel_width, 280.0]),
            generation_panel: PanelState::new(
                [margin + panel_width + 16.0, margin],
                [panel_width, panel_height],
            ),
            brush_panel: PanelState::new(
                [margin + panel_width + 16.0, margin + 232.0],
                [panel_width, 300.0],
            ),
            slider_layout: SliderLayout::default(),
            active_slider: None,
        }
    }

    pub fn build(
        &mut self,
        gui: &mut GuiContext,
        input: &TerrainEditorUiInput,
        data: &TerrainEditorUiData,
        focused_input: Option<FocusedInput>,
    ) -> TerrainEditorUiOutput {
        let mut output = TerrainEditorUiOutput {
            open_clicked: false,
            save_clicked: false,
            generate_clicked: false,
            brush_apply_clicked: false,
            select_chunk: None,
            prev_chunk_clicked: false,
            next_chunk_clicked: false,
            focused_input: None,
            brush_tool: None,
            brush_radius: None,
            brush_strength: None,
            ui_hovered: false,
        };

        let panel_metrics = PanelMetrics::default();
        let panel_colors = PanelColors::default();
        let interaction = PanelInteraction {
            cursor: [input.cursor.x, input.cursor.y],
            mouse_pressed: input.mouse_pressed,
            mouse_down: input.mouse_down,
        };

        let db_layout = gui.submit_panel(
            &Panel::new("Database"),
            &mut self.db_panel,
            &PanelRenderOptions {
                viewport: data.viewport.to_array(),
                layer: GuiLayer::Overlay,
                interaction,
                metrics: panel_metrics,
                colors: panel_colors,
                allow_close: false,
                allow_minimize: false,
                show_shadow: true,
                show_outline: true,
            },
        );
        output.ui_hovered |= db_layout.display_rect.contains([input.cursor.x, input.cursor.y]);
        if db_layout.show_content() {
            let content_width = db_layout.content_rect.max[0] - db_layout.content_rect.min[0];
            let mut cursor_y = db_layout.content_rect.min[1] + 10.0;
            let label_x = db_layout.content_rect.min[0] + 12.0;
            gui.submit_text(GuiTextDraw {
                text: "Database Path".to_string(),
                position: [label_x, cursor_y],
                color: [0.85, 0.9, 0.97, 1.0],
                scale: 0.85,
            });
            cursor_y += 18.0;

            let input_rect = MenuRect::from_position_size(
                [label_x, cursor_y],
                [content_width - 24.0, 26.0],
            );
            if text_field(
                gui,
                input_rect,
                data.db_path,
                data.viewport,
                focused_input == Some(FocusedInput::DbPath),
                input.cursor,
            ) && input.mouse_pressed
            {
                output.focused_input = Some(FocusedInput::DbPath);
            }
            cursor_y += 38.0;

            let button_width = 90.0;
            let button_height = 26.0;
            let button_gap = 10.0;
            let open_rect = MenuRect::from_position_size(
                [label_x, cursor_y],
                [button_width, button_height],
            );
            if button(gui, open_rect, "Open", input, data.viewport) {
                output.open_clicked = true;
            }
            let save_rect = MenuRect::from_position_size(
                [label_x + button_width + button_gap, cursor_y],
                [button_width, button_height],
            );
            if button(gui, save_rect, "Save", input, data.viewport) {
                output.save_clicked = true;
            }

            let dirty_label = if data.db_open {
                if data.db_dirty {
                    "Dirty: Yes"
                } else {
                    "Dirty: No"
                }
            } else {
                "Closed"
            };
            gui.submit_text(GuiTextDraw {
                text: dirty_label.to_string(),
                position: [save_rect.max[0] + 16.0, cursor_y + 6.0],
                color: [0.78, 0.82, 0.9, 1.0],
                scale: 0.82,
            });
        }

        let chunk_layout = gui.submit_panel(
            &Panel::new("Chunks"),
            &mut self.chunk_panel,
            &PanelRenderOptions {
                viewport: data.viewport.to_array(),
                layer: GuiLayer::Overlay,
                interaction,
                metrics: panel_metrics,
                colors: panel_colors,
                allow_close: false,
                allow_minimize: false,
                show_shadow: true,
                show_outline: true,
            },
        );
        output.ui_hovered |= chunk_layout.display_rect.contains([input.cursor.x, input.cursor.y]);
        if chunk_layout.show_content() {
            let content_width = chunk_layout.content_rect.max[0] - chunk_layout.content_rect.min[0];
            let mut cursor_y = chunk_layout.content_rect.min[1] + 10.0;
            let label_x = chunk_layout.content_rect.min[0] + 12.0;
            gui.submit_text(GuiTextDraw {
                text: format!("Chunks ({})", data.chunk_keys.len()),
                position: [label_x, cursor_y],
                color: [0.85, 0.9, 0.97, 1.0],
                scale: 0.85,
            });
            cursor_y += 22.0;

            let item_height = 22.0;
            let item_gap = 4.0;
            let list_height =
                (chunk_layout.content_rect.max[1] - cursor_y - 40.0).max(item_height);
            let max_items =
                ((list_height + item_gap) / (item_height + item_gap)).floor() as usize;
            for (index, key) in data.chunk_keys.iter().take(max_items).enumerate() {
                let rect = MenuRect::from_position_size(
                    [label_x, cursor_y + index as f32 * (item_height + item_gap)],
                    [content_width - 24.0, item_height],
                );
                let selected = data.selected_chunk == Some(index);
                let hovered = rect.contains([input.cursor.x, input.cursor.y]);
                let color = if selected {
                    [0.28, 0.34, 0.46, 0.92]
                } else if hovered {
                    [0.2, 0.24, 0.34, 0.85]
                } else {
                    [0.16, 0.18, 0.24, 0.8]
                };
                gui.submit_draw(GuiDraw::new(
                    GuiLayer::Overlay,
                    None,
                    quad_from_rect(rect, color, data.viewport),
                ));
                gui.submit_text(GuiTextDraw {
                    text: key.clone(),
                    position: [rect.min[0] + 8.0, rect.min[1] + 4.0],
                    color: [0.86, 0.9, 0.97, 1.0],
                    scale: 0.78,
                });
                if hovered && input.mouse_pressed {
                    output.select_chunk = Some(index);
                }
            }

            let button_width = 90.0;
            let button_height = 26.0;
            let button_gap = 10.0;
            let button_y = chunk_layout.content_rect.max[1] - 32.0;
            let prev_rect = MenuRect::from_position_size(
                [label_x, button_y],
                [button_width, button_height],
            );
            if button(gui, prev_rect, "Prev", input, data.viewport) {
                output.prev_chunk_clicked = true;
            }
            let next_rect = MenuRect::from_position_size(
                [label_x + button_width + button_gap, button_y],
                [button_width, button_height],
            );
            if button(gui, next_rect, "Next", input, data.viewport) {
                output.next_chunk_clicked = true;
            }
        }

        let generation_layout = gui.submit_panel(
            &Panel::new("Generation"),
            &mut self.generation_panel,
            &PanelRenderOptions {
                viewport: data.viewport.to_array(),
                layer: GuiLayer::Overlay,
                interaction,
                metrics: panel_metrics,
                colors: panel_colors,
                allow_close: false,
                allow_minimize: false,
                show_shadow: true,
                show_outline: true,
            },
        );
        output.ui_hovered |=
            generation_layout.display_rect.contains([input.cursor.x, input.cursor.y]);
        if generation_layout.show_content() {
            let content_width =
                generation_layout.content_rect.max[0] - generation_layout.content_rect.min[0];
            let label_x = generation_layout.content_rect.min[0] + 12.0;
            let mut cursor_y = generation_layout.content_rect.min[1] + 10.0;

            cursor_y = labeled_input(
                gui,
                "Seed",
                data.seed_input,
                label_x,
                cursor_y,
                content_width,
                focused_input == Some(FocusedInput::Seed),
                input,
                data.viewport,
                &mut output,
                FocusedInput::Seed,
            );

            cursor_y = labeled_input(
                gui,
                "LOD",
                data.lod_input,
                label_x,
                cursor_y,
                content_width,
                focused_input == Some(FocusedInput::Lod),
                input,
                data.viewport,
                &mut output,
                FocusedInput::Lod,
            );

            cursor_y = labeled_input(
                gui,
                "Generator Graph",
                data.graph_id_input,
                label_x,
                cursor_y,
                content_width,
                focused_input == Some(FocusedInput::GeneratorGraph),
                input,
                data.viewport,
                &mut output,
                FocusedInput::GeneratorGraph,
            );

            let button_rect = MenuRect::from_position_size(
                [label_x, cursor_y + 6.0],
                [120.0, 28.0],
            );
            if button(gui, button_rect, "Generate", input, data.viewport) {
                output.generate_clicked = true;
            }
        }

        let brush_layout = gui.submit_panel(
            &Panel::new("Brush"),
            &mut self.brush_panel,
            &PanelRenderOptions {
                viewport: data.viewport.to_array(),
                layer: GuiLayer::Overlay,
                interaction,
                metrics: panel_metrics,
                colors: panel_colors,
                allow_close: false,
                allow_minimize: false,
                show_shadow: true,
                show_outline: true,
            },
        );
        output.ui_hovered |= brush_layout.display_rect.contains([input.cursor.x, input.cursor.y]);
        if brush_layout.show_content() {
            let content_width = brush_layout.content_rect.max[0] - brush_layout.content_rect.min[0];
            let label_x = brush_layout.content_rect.min[0] + 12.0;
            let mut cursor_y = brush_layout.content_rect.min[1] + 10.0;
            gui.submit_text(GuiTextDraw {
                text: "Tool".to_string(),
                position: [label_x, cursor_y],
                color: [0.85, 0.9, 0.97, 1.0],
                scale: 0.85,
            });
            cursor_y += 20.0;

            let tool_buttons = [
                (TerrainMutationOpKind::SphereAdd, "Sphere Add"),
                (TerrainMutationOpKind::SphereSub, "Sphere Sub"),
                (TerrainMutationOpKind::Smooth, "Smooth"),
                (TerrainMutationOpKind::MaterialPaint, "Paint"),
                (TerrainMutationOpKind::CapsuleAdd, "Capsule Add"),
                (TerrainMutationOpKind::CapsuleSub, "Capsule Sub"),
            ];
            let button_width = (content_width - 24.0) * 0.5 - 6.0;
            let button_height = 24.0;
            for (index, (tool, label)) in tool_buttons.iter().enumerate() {
                let row = index / 2;
                let col = index % 2;
                let rect = MenuRect::from_position_size(
                    [
                        label_x + col as f32 * (button_width + 12.0),
                        cursor_y + row as f32 * (button_height + 8.0),
                    ],
                    [button_width, button_height],
                );
                let selected = data.brush_tool == *tool;
                if button_with_state(gui, rect, label, input, data.viewport, selected) {
                    output.brush_tool = Some(*tool);
                }
            }

            let slider_start_y = cursor_y + 3.0 * (button_height + 8.0) + 4.0;
            let slider_metrics = SliderMetrics {
                item_height: 26.0,
                item_gap: 8.0,
                ..SliderMetrics::default()
            };
            let slider_height =
                slider_metrics.padding[1] * 2.0 + slider_metrics.item_height * 2.0
                    + slider_metrics.item_gap;
            let slider_options = SliderRenderOptions {
                viewport: data.viewport.to_array(),
                position: [brush_layout.content_rect.min[0], slider_start_y],
                size: [content_width, slider_height],
                layer: GuiLayer::Overlay,
                metrics: slider_metrics,
                colors: SliderColors::default(),
                state: SliderState {
                    hovered: hovered_slider(&self.slider_layout, input.cursor),
                    active: self.active_slider,
                },
            };

            let sliders = [
                Slider {
                    id: SLIDER_RADIUS,
                    label: "Radius".to_string(),
                    value: data.brush_radius,
                    min: 1.0,
                    max: 64.0,
                    enabled: true,
                    show_value: true,
                },
                Slider {
                    id: SLIDER_STRENGTH,
                    label: "Strength".to_string(),
                    value: data.brush_strength,
                    min: 0.1,
                    max: 8.0,
                    enabled: true,
                    show_value: true,
                },
            ];

            let layout = gui.submit_sliders(&sliders, &slider_options);
            self.slider_layout = layout.clone();
            if input.mouse_pressed {
                if let Some(item) = layout
                    .items
                    .iter()
                    .find(|item| slider_hit(item, input.cursor))
                {
                    self.active_slider = Some(item.id);
                }
            }
            if input.mouse_released {
                self.active_slider = None;
            }
            if input.mouse_down {
                if let Some(active_id) = self.active_slider {
                    if let Some(item) = layout.items.iter().find(|item| item.id == active_id) {
                        let value = slider_value_from_cursor(input.cursor, item.track_rect, item.min, item.max);
                        match active_id {
                            SLIDER_RADIUS => output.brush_radius = Some(value),
                            SLIDER_STRENGTH => output.brush_strength = Some(value),
                            _ => {}
                        }
                    }
                }
            }

            let button_rect = MenuRect::from_position_size(
                [label_x, slider_start_y + slider_height + 8.0],
                [130.0, 28.0],
            );
            if button(gui, button_rect, "Apply Brush", input, data.viewport) {
                output.brush_apply_clicked = true;
            }
        }

        output
    }
}

fn labeled_input(
    gui: &mut GuiContext,
    label: &str,
    value: &str,
    x: f32,
    y: f32,
    width: f32,
    focused: bool,
    input: &TerrainEditorUiInput,
    viewport: Vec2,
    output: &mut TerrainEditorUiOutput,
    field: FocusedInput,
) -> f32 {
    gui.submit_text(GuiTextDraw {
        text: label.to_string(),
        position: [x, y],
        color: [0.85, 0.9, 0.97, 1.0],
        scale: 0.82,
    });
    let input_rect = MenuRect::from_position_size([x, y + 16.0], [width - 24.0, 24.0]);
    if text_field(gui, input_rect, value, viewport, focused, input.cursor) && input.mouse_pressed {
        output.focused_input = Some(field);
    }
    y + 48.0
}

fn text_field(
    gui: &mut GuiContext,
    rect: MenuRect,
    text: &str,
    viewport: Vec2,
    focused: bool,
    cursor: Vec2,
) -> bool {
    let hovered = rect.contains([cursor.x, cursor.y]);
    let base_color = if focused {
        [0.2, 0.26, 0.36, 0.95]
    } else if hovered {
        [0.18, 0.24, 0.34, 0.9]
    } else {
        [0.16, 0.18, 0.24, 0.85]
    };
    gui.submit_draw(GuiDraw::new(
        GuiLayer::Overlay,
        None,
        quad_from_rect(rect, base_color, viewport),
    ));
    gui.submit_text(GuiTextDraw {
        text: text.to_string(),
        position: [rect.min[0] + 8.0, rect.min[1] + 5.0],
        color: [0.88, 0.92, 0.98, 1.0],
        scale: 0.78,
    });
    hovered
}

fn button(
    gui: &mut GuiContext,
    rect: MenuRect,
    label: &str,
    input: &TerrainEditorUiInput,
    viewport: Vec2,
) -> bool {
    button_with_state(gui, rect, label, input, viewport, false)
}

fn button_with_state(
    gui: &mut GuiContext,
    rect: MenuRect,
    label: &str,
    input: &TerrainEditorUiInput,
    viewport: Vec2,
    selected: bool,
) -> bool {
    let hovered = rect.contains([input.cursor.x, input.cursor.y]);
    let base_color = if selected {
        [0.28, 0.36, 0.48, 0.95]
    } else if hovered {
        [0.22, 0.28, 0.38, 0.9]
    } else {
        [0.18, 0.22, 0.3, 0.85]
    };
    gui.submit_draw(GuiDraw::new(
        GuiLayer::Overlay,
        None,
        quad_from_rect(rect, base_color, viewport),
    ));
    gui.submit_text(GuiTextDraw {
        text: label.to_string(),
        position: [rect.min[0] + 8.0, rect.min[1] + 6.0],
        color: if selected {
            [0.95, 0.98, 1.0, 1.0]
        } else {
            [0.85, 0.9, 0.97, 1.0]
        },
        scale: 0.78,
    });
    hovered && input.mouse_pressed
}

fn slider_value_from_cursor(cursor: Vec2, rect: MenuRect, min: f32, max: f32) -> f32 {
    if (max - min).abs() < f32::EPSILON {
        return min;
    }
    let t = ((cursor.x - rect.min[0]) / (rect.max[0] - rect.min[0])).clamp(0.0, 1.0);
    min + (max - min) * t
}

fn slider_hit(item: &meshi_graphics::gui::SliderItemLayout, cursor: Vec2) -> bool {
    item.track_rect.contains([cursor.x, cursor.y]) || item.knob_rect.contains([cursor.x, cursor.y])
}

fn hovered_slider(layout: &SliderLayout, cursor: Vec2) -> Option<u32> {
    layout
        .items
        .iter()
        .find(|item| slider_hit(item, cursor))
        .map(|item| item.id)
}

fn quad_from_rect(rect: MenuRect, color: [f32; 4], viewport: Vec2) -> GuiQuad {
    let left = (rect.min[0] / viewport.x) * 2.0 - 1.0;
    let right = (rect.max[0] / viewport.x) * 2.0 - 1.0;
    let top = 1.0 - (rect.min[1] / viewport.y) * 2.0;
    let bottom = 1.0 - (rect.max[1] / viewport.y) * 2.0;

    GuiQuad {
        positions: [[left, top], [right, top], [right, bottom], [left, bottom]],
        uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        color,
    }
}
