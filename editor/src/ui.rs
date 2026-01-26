use meshi_graphics::gui::{
    GuiContext, GuiDraw, GuiLayer, GuiQuad, GuiTextDraw, Menu, MenuBar, MenuBarRenderOptions,
    MenuBarState, MenuColors, MenuItem, MenuLayoutMetrics,
};
use std::fmt::Write;

use crate::project::ProjectManager;

pub struct EditorUi {
    menu_bar: MenuBar,
    menu_state: MenuBarState,
    viewport: [f32; 2],
}

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
        let content_size = [
            self.viewport[0],
            self.viewport[1] - metrics.bar_height,
        ];

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
            &[
                "Scene Hierarchy",
                "Entities & Components",
                "Systems",
            ],
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

    pub fn build_egui(&mut self, ctx: &egui::Context, project_manager: &ProjectManager) {
        self.refresh_file_menu(project_manager);
        self.refresh_build_menu();

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                for menu in &self.menu_bar.menus {
                    ui.menu_button(&menu.label, |ui| {
                        self.draw_egui_menu_items(ui, &menu.items);
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
                for line in [
                    "Scene Hierarchy",
                    "Entities & Components",
                    "Systems",
                ] {
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

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Engine Preview (egui)");
            ui.separator();
            for line in [
                "Runtime frame will render here",
                "Use external IDE for C++ scripts",
                "egui backend active (no renderer wired yet)",
            ] {
                ui.label(line);
            }
        });
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

    fn draw_egui_menu_items(&self, ui: &mut egui::Ui, items: &[MenuItem]) {
        for item in items {
            if item.is_separator {
                ui.separator();
                continue;
            }

            if let Some(submenu) = item.submenu.as_ref() {
                ui.menu_button(&item.label, |ui| {
                    self.draw_egui_menu_items(ui, submenu);
                });
                continue;
            }

            let response = ui.add_enabled(item.enabled, egui::Button::new(&item.label));
            if response.clicked() {
                ui.close_menu();
            }
        }
    }

    pub fn viewport(&self) -> [f32; 2] {
        self.viewport
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
            MenuItem::new("Build Project"),
            MenuItem::new("Build & Run"),
            MenuItem::new("Rebuild All"),
            MenuItem::separator(),
            MenuItem::new("Generate C++ Bindings"),
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

    fn draw_panel(
        &self,
        gui: &mut GuiContext,
        layout: &PanelLayout,
        title: &str,
        lines: &[&str],
    ) {
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
