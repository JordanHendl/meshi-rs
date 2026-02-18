mod actions;
mod menu;
mod panels;
mod project_tree;

pub use actions::UiAction;

use meshi_graphics::gui::{
    GuiContext, GuiDraw, GuiLayer, GuiQuad, GuiTextDraw, MenuBar, MenuBarRenderOptions,
    MenuBarState, MenuColors, MenuItem, MenuLayoutMetrics,
};

use crate::{
    project::ProjectManager,
    runtime::{RuntimeControlState, RuntimeFrame},
};

pub struct EditorUi {
    menu_bar: MenuBar,
    menu_state: MenuBarState,
    viewport: [f32; 2],
    runtime_texture: Option<egui::TextureHandle>,
    runtime_texture_size: [usize; 2],
}

impl EditorUi {
    pub fn new() -> Self {
        Self {
            menu_bar: menu::create_menu_bar(),
            menu_state: MenuBarState::default(),
            viewport: [1280.0, 720.0],
            runtime_texture: None,
            runtime_texture_size: [0, 0],
        }
    }

    pub fn build_meshi(&mut self, gui: &mut GuiContext, project_manager: &ProjectManager) {
        menu::refresh_file_menu(&mut self.menu_bar, project_manager);
        menu::refresh_build_menu(&mut self.menu_bar);
        let metrics = MenuLayoutMetrics::default();
        let _menu_layout = self.menu_bar.submit_to_draw_list(
            gui,
            &MenuBarRenderOptions {
                viewport: self.viewport,
                position: [0.0, 0.0],
                layer: GuiLayer::Overlay,
                metrics,
                colors: MenuColors::default(),
                state: self.menu_state,
            },
        );

        let content_origin = [0.0, metrics.bar_height];
        let content_size = [self.viewport[0], self.viewport[1] - metrics.bar_height];

        let gutter = 12.0;
        let left_width = 280.0;
        let right_width = 320.0;
        let center_width = content_size[0] - left_width - right_width - gutter * 2.0;
        let center_height = content_size[1] - gutter * 2.0;

        let left_panel = PanelLayout {
            position: [content_origin[0], content_origin[1]],
            size: [left_width, content_size[1]],
        };
        let right_panel = PanelLayout {
            position: [
                content_origin[0] + left_width + gutter + center_width + gutter,
                content_origin[1],
            ],
            size: [right_width, content_size[1]],
        };
        let center_panel = PanelLayout {
            position: [
                content_origin[0] + left_width + gutter,
                content_origin[1] + gutter,
            ],
            size: [center_width, center_height],
        };

        self.draw_panel(
            gui,
            &left_panel,
            "ECS / Scene",
            &["Scene Hierarchy", "Entities & Components", "Systems"],
        );
        self.draw_panel(
            gui,
            &right_panel,
            "Inspector",
            &[
                "Selection Parameters",
                "Transform",
                "Mesh / Material",
                "Script Bindings",
            ],
        );
        self.draw_panel(
            gui,
            &center_panel,
            "Engine Preview (Meshi GUI)",
            &[
                "Runtime frame will render here",
                "Use external IDE for C++ scripts",
                "GUI shell ready for egui/Qt swap",
            ],
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_egui(
        &mut self,
        ctx: &egui::Context,
        project_manager: &ProjectManager,
        runtime_controls: &mut RuntimeControlState,
        runtime_status: crate::runtime::RuntimeStatus,
        runtime_logs: &[crate::runtime::RuntimeLogEntry],
        runtime_error: Option<&str>,
        project_viewport_pixels: [u32; 2],
    ) -> UiFrameOutput {
        menu::refresh_file_menu(&mut self.menu_bar, project_manager);
        menu::refresh_build_menu(&mut self.menu_bar);

        let mut action = None;
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                for menu in &self.menu_bar.menus {
                    ui.menu_button(&menu.label, |ui| {
                        if let Some(clicked) = self.draw_egui_menu_items(ui, &menu.items) {
                            action = Some(clicked);
                        }
                    });
                }
            });
        });

        egui::SidePanel::left("scene_panel")
            .resizable(false)
            .default_width(280.0)
            .show(ctx, |ui| {
                if let Some(clicked) = panels::draw_project_panel(
                    ui,
                    project_tree::project_structure_entries(project_manager),
                ) {
                    action = Some(clicked);
                }
            });

        egui::SidePanel::right("inspector_panel")
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                panels::draw_inspector_panel(ui);
            });

        let viewport_pixels = [
            project_viewport_pixels[0].max(1),
            project_viewport_pixels[1].max(1),
        ];
        egui::CentralPanel::default().show(ctx, |ui| {
            panels::draw_viewport_panel(
                ui,
                ctx,
                runtime_controls,
                self.runtime_texture.as_ref(),
                viewport_pixels,
            );
        });

        egui::TopBottomPanel::bottom("runtime_logs")
            .resizable(true)
            .default_height(140.0)
            .show(ctx, |ui| {
                panels::draw_runtime_logs_panel(ui, runtime_status, runtime_logs, runtime_error)
            });

        self.viewport = [viewport_pixels[0] as f32, viewport_pixels[1] as f32];
        UiFrameOutput {
            metrics: ViewportMetrics {
                pixels: viewport_pixels,
            },
            action,
        }
    }

    fn draw_egui_menu_items(&self, ui: &mut egui::Ui, items: &[MenuItem]) -> Option<UiAction> {
        let mut action = None;
        for item in items {
            if item.is_separator {
                ui.separator();
                continue;
            }

            if let Some(submenu) = item.submenu.as_ref() {
                ui.menu_button(&item.label, |ui| {
                    if let Some(clicked) = self.draw_egui_menu_items(ui, submenu) {
                        action = Some(clicked);
                    }
                });
                continue;
            }

            let response = ui.add_enabled(item.enabled, egui::Button::new(&item.label));
            if response.clicked() {
                ui.close_menu();
                if let Some(action_id) = item.action_id {
                    action = action.or(menu::action_for_menu_item(action_id));
                }
            }
        }
        action
    }

    pub fn update_runtime_texture(&mut self, ctx: &egui::Context, frame: Option<&RuntimeFrame>) {
        let Some(frame) = frame else {
            return;
        };
        let size = frame.size;
        let image = egui::ColorImage::from_rgba_unmultiplied(size, &frame.pixels);
        match self.runtime_texture.as_mut() {
            Some(texture) if self.runtime_texture_size == size => {
                texture.set(image, egui::TextureOptions::LINEAR);
            }
            _ => {
                self.runtime_texture = Some(ctx.load_texture(
                    "meshi_editor_runtime_viewport",
                    image,
                    egui::TextureOptions::LINEAR,
                ));
                self.runtime_texture_size = size;
            }
        }
    }

    pub fn viewport(&self) -> [f32; 2] {
        self.viewport
    }

    pub fn set_viewport(&mut self, size: [f32; 2]) {
        self.viewport = size;
    }

    pub fn build_qt_placeholder(&mut self, project_manager: &ProjectManager, frame: usize) {
        menu::refresh_file_menu(&mut self.menu_bar, project_manager);
        menu::refresh_build_menu(&mut self.menu_bar);
        let _ = frame;
    }

    fn draw_panel(&self, gui: &mut GuiContext, layout: &PanelLayout, title: &str, lines: &[&str]) {
        gui.submit_draw(GuiDraw::new(
            GuiLayer::Overlay,
            None,
            quad_from_pixels(
                layout.position,
                layout.size,
                [0.08, 0.09, 0.12, 0.95],
                self.viewport,
            ),
        ));

        let header_height = 32.0;
        gui.submit_draw(GuiDraw::new(
            GuiLayer::Overlay,
            None,
            quad_from_pixels(
                layout.position,
                [layout.size[0], header_height],
                [0.12, 0.13, 0.16, 0.95],
                self.viewport,
            ),
        ));

        gui.submit_text(GuiTextDraw {
            text: title.to_string(),
            position: [layout.position[0] + 12.0, layout.position[1] + 20.0],
            color: [0.92, 0.94, 0.98, 1.0],
            scale: 1.0,
        });

        for (index, line) in lines.iter().enumerate() {
            gui.submit_text(GuiTextDraw {
                text: line.to_string(),
                position: [
                    layout.position[0] + 16.0,
                    layout.position[1] + header_height + 28.0 + index as f32 * 22.0,
                ],
                color: [0.78, 0.8, 0.86, 1.0],
                scale: 0.9,
            });
        }
    }
}

struct PanelLayout {
    position: [f32; 2],
    size: [f32; 2],
}

pub struct ViewportMetrics {
    pub pixels: [u32; 2],
}

pub struct UiFrameOutput {
    pub metrics: ViewportMetrics,
    pub action: Option<UiAction>,
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
