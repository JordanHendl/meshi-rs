use meshi_graphics::gui::{
    GuiContext, GuiDraw, GuiLayer, GuiQuad, GuiTextDraw, Menu, MenuBar, MenuBarRenderOptions,
    MenuBarState, MenuColors, MenuLayoutMetrics,
};

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

    pub fn build(&mut self, gui: &mut GuiContext) {
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

        let panel_position = [0.0, metrics.bar_height];
        let panel_size = [600.0, 320.0];

        gui.submit_draw(GuiDraw::new(
            GuiLayer::Overlay,
            None,
            quad_from_pixels(panel_position, panel_size, [0.08, 0.09, 0.1, 0.92], self.viewport),
        ));

        gui.submit_text(GuiTextDraw {
            text: "Editor UI scaffolding".to_string(),
            position: [16.0, panel_position[1] + 24.0],
            color: [0.9, 0.92, 0.96, 1.0],
            scale: 1.0,
        });
    }
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
