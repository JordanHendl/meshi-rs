use meshi_graphics::gui::{
    GuiContext, GuiDraw, GuiLayer, GuiQuad, GuiTextDraw, Menu, MenuBar, MenuBarRenderOptions,
    MenuBarState, MenuColors, MenuItem, MenuLayoutMetrics,
};
use std::fmt::Write;

use crate::{
    project::ProjectManager,
    runtime::{RuntimeControlState, RuntimeFrame, RuntimeLogEntry, RuntimeLogLevel, RuntimeStatus},
};

pub struct EditorUi {
    menu_bar: MenuBar,
    menu_state: MenuBarState,
    viewport: [f32; 2],
    runtime_texture: Option<egui::TextureHandle>,
    runtime_texture_size: [usize; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    BuildProject,
    BuildAndRun,
    RebuildAll,
    GenerateBindings,
}

const ACTION_BUILD_PROJECT: u32 = 1;
const ACTION_BUILD_AND_RUN: u32 = 2;
const ACTION_REBUILD_ALL: u32 = 3;
const ACTION_GENERATE_BINDINGS: u32 = 4;

impl EditorUi {
    pub fn new() -> Self {
        Self {
            menu_bar: MenuBar {
                menus: vec![
                    Menu {
                        label: "File".to_string(),
                        items: Vec::new(),
                    },
                    Menu {
                        label: "Edit".to_string(),
                        items: Vec::new(),
                    },
                    Menu {
                        label: "Build".to_string(),
                        items: Vec::new(),
                    },
                    Menu {
                        label: "View".to_string(),
                        items: Vec::new(),
                    },
                    Menu {
                        label: "Help".to_string(),
                        items: Vec::new(),
                    },
                ],
            },
            menu_state: MenuBarState::default(),
            viewport: [1280.0, 720.0],
            runtime_texture: None,
            runtime_texture_size: [0, 0],
        }
    }

    pub fn build_meshi(&mut self, gui: &mut GuiContext, project_manager: &ProjectManager) {
        self.refresh_file_menu(project_manager);
        self.refresh_build_menu();
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

    pub fn build_egui(
        &mut self,
        ctx: &egui::Context,
        project_manager: &ProjectManager,
        runtime_controls: &mut RuntimeControlState,
        runtime_status: RuntimeStatus,
        runtime_logs: &[RuntimeLogEntry],
        runtime_error: Option<&str>,
    ) -> UiFrameOutput {
        self.refresh_file_menu(project_manager);
        self.refresh_build_menu();

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
                ui.heading("ECS / Scene");
                ui.separator();
                for line in ["Scene Hierarchy", "Entities & Components", "Systems"] {
                    ui.label(line);
                }
            });

        egui::SidePanel::right("inspector_panel")
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.heading("Inspector");
                ui.separator();
                for line in [
                    "Selection Parameters",
                    "Transform",
                    "Mesh / Material",
                    "Script Bindings",
                ] {
                    ui.label(line);
                }
            });

        let mut viewport_pixels = [1_u32, 1_u32];
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Engine Preview");
            ui.separator();
            ui.horizontal(|ui| {
                let play_label = if runtime_controls.playing {
                    "Pause"
                } else {
                    "Play"
                };
                if ui.button(play_label).clicked() {
                    runtime_controls.playing = !runtime_controls.playing;
                }
                if ui.button("Step").clicked() {
                    runtime_controls.request_step();
                }
                let status = if runtime_controls.playing {
                    "Running"
                } else {
                    "Paused"
                };
                ui.label(status);
            });
            ui.separator();

            let available = ui.available_size();
            let viewport_points = egui::vec2(available.x.max(1.0), available.y.max(1.0));
            let pixels_per_point = ctx.pixels_per_point();
            viewport_pixels = [
                (viewport_points.x * pixels_per_point).round().max(1.0) as u32,
                (viewport_points.y * pixels_per_point).round().max(1.0) as u32,
            ];

            if let Some(texture) = &self.runtime_texture {
                ui.add(egui::Image::new(texture).fit_to_exact_size(viewport_points));
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Waiting for runtime frame...");
                });
            }
        });
        egui::TopBottomPanel::bottom("runtime_logs")
            .resizable(true)
            .default_height(140.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Runtime Logs");
                    ui.separator();
                    ui.label(match runtime_status {
                        RuntimeStatus::Idle => "Idle",
                        RuntimeStatus::Building => "Building",
                        RuntimeStatus::Running => "Running",
                        RuntimeStatus::Failed => "Failed",
                    });
                });
                if let Some(error) = runtime_error {
                    ui.colored_label(egui::Color32::RED, error);
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for entry in runtime_logs {
                            let color = match entry.level {
                                RuntimeLogLevel::Info => egui::Color32::LIGHT_GRAY,
                                RuntimeLogLevel::Warn => egui::Color32::YELLOW,
                                RuntimeLogLevel::Error => egui::Color32::LIGHT_RED,
                            };
                            ui.colored_label(color, &entry.message);
                        }
                    });
            });
        self.viewport = [viewport_pixels[0] as f32, viewport_pixels[1] as f32];
        UiFrameOutput {
            metrics: ViewportMetrics {
                pixels: viewport_pixels,
            },
            action,
        }
    }

    pub fn build_qt_placeholder(&mut self, project_manager: &ProjectManager, frame: usize) {
        self.refresh_file_menu(project_manager);
        self.refresh_build_menu();

        let mut buffer = String::new();
        let _ = write!(
            &mut buffer,
            "Qt backend placeholder tick {} (menus: {})",
            frame,
            self.menu_bar.menus.len()
        );
        let _ = buffer;
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
                    action = action.or(match action_id {
                        ACTION_BUILD_PROJECT => Some(UiAction::BuildProject),
                        ACTION_BUILD_AND_RUN => Some(UiAction::BuildAndRun),
                        ACTION_REBUILD_ALL => Some(UiAction::RebuildAll),
                        ACTION_GENERATE_BINDINGS => Some(UiAction::GenerateBindings),
                        _ => None,
                    });
                }
            }
        }
        action
    }

    pub fn viewport(&self) -> [f32; 2] {
        self.viewport
    }

    pub fn set_viewport(&mut self, size: [f32; 2]) {
        self.viewport = size;
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

    fn refresh_file_menu(&mut self, project_manager: &ProjectManager) {
        let recent_entries = project_manager.recent_projects_with_status();
        let recent_menu_items = if recent_entries.is_empty() {
            let mut item = MenuItem::new("No recent projects");
            item.enabled = false;
            vec![item]
        } else {
            recent_entries
                .into_iter()
                .map(|(path, exists)| {
                    let mut item = MenuItem::new(path);
                    item.enabled = exists;
                    item
                })
                .collect()
        };

        let file_items = vec![
            MenuItem::new("New Project"),
            MenuItem::new("Open Project"),
            MenuItem::new("Open Workspace"),
            MenuItem::new("Save All"),
            MenuItem::separator(),
            MenuItem::new("Recent Projects").with_submenu(recent_menu_items),
        ];

        if let Some(file_menu) = self
            .menu_bar
            .menus
            .iter_mut()
            .find(|menu| menu.label == "File")
        {
            file_menu.items = file_items;
        }
    }

    fn refresh_build_menu(&mut self) {
        let build_items = vec![
            {
                let mut item = MenuItem::new("Build Project");
                item.action_id = Some(ACTION_BUILD_PROJECT);
                item
            },
            {
                let mut item = MenuItem::new("Build & Run");
                item.action_id = Some(ACTION_BUILD_AND_RUN);
                item
            },
            {
                let mut item = MenuItem::new("Rebuild All");
                item.action_id = Some(ACTION_REBUILD_ALL);
                item
            },
            MenuItem::separator(),
            {
                let mut item = MenuItem::new("Generate C++ Bindings");
                item.action_id = Some(ACTION_GENERATE_BINDINGS);
                item
            },
        ];

        if let Some(build_menu) = self
            .menu_bar
            .menus
            .iter_mut()
            .find(|menu| menu.label == "Build")
        {
            build_menu.items = build_items;
        }
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
