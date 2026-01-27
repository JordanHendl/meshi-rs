use std::env;

use crate::{project::ProjectManager, runtime::RuntimeControlState, ui::EditorUi};
use meshi_graphics::gui::{GuiContext, GuiFrame};

pub trait EditorRenderBackend {
    fn name(&self) -> &'static str;
    fn render_frame(&mut self, ui: &mut EditorUi, project_manager: &ProjectManager);
    fn take_gui_frame(&mut self) -> Option<GuiFrame>;
}

pub fn select_render_backend() -> Box<dyn EditorRenderBackend> {
    match env::var("MESHI_EDITOR_BACKEND")
        .unwrap_or_else(|_| "meshi".to_string())
        .to_lowercase()
        .as_str()
    {
        "egui" => Box::new(EguiBackend::default()),
        "qt" => Box::new(QtBackend::default()),
        _ => Box::new(MeshiGuiBackend::default()),
    }
}

#[derive(Default)]
pub struct MeshiGuiBackend {
    gui: GuiContext,
    last_frame: Option<GuiFrame>,
}

impl EditorRenderBackend for MeshiGuiBackend {
    fn name(&self) -> &'static str {
        "meshi"
    }

    fn render_frame(&mut self, ui: &mut EditorUi, project_manager: &ProjectManager) {
        ui.build_meshi(&mut self.gui, project_manager);
        self.last_frame = Some(self.gui.build_frame());
    }

    fn take_gui_frame(&mut self) -> Option<GuiFrame> {
        self.last_frame.take()
    }
}

#[derive(Default)]
pub struct EguiBackend {
    ctx: egui::Context,
    raw_input: egui::RawInput,
    last_output: Option<egui::FullOutput>,
}

impl EditorRenderBackend for EguiBackend {
    fn name(&self) -> &'static str {
        "egui"
    }

    fn render_frame(&mut self, ui: &mut EditorUi, project_manager: &ProjectManager) {
        let mut raw_input = std::mem::take(&mut self.raw_input);
        let viewport = ui.viewport();
        raw_input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(viewport[0], viewport[1]),
        ));
        self.last_output = Some(self.ctx.run(raw_input, |ctx| {
            let mut controls = RuntimeControlState::default();
            ui.build_egui(ctx, project_manager, &mut controls);
        }));
    }

    fn take_gui_frame(&mut self) -> Option<GuiFrame> {
        None
    }
}

#[derive(Default)]
pub struct QtBackend {
    last_frame: usize,
}

impl EditorRenderBackend for QtBackend {
    fn name(&self) -> &'static str {
        "qt"
    }

    fn render_frame(&mut self, ui: &mut EditorUi, project_manager: &ProjectManager) {
        self.last_frame = self.last_frame.wrapping_add(1);
        ui.build_qt_placeholder(project_manager, self.last_frame);
    }

    fn take_gui_frame(&mut self) -> Option<GuiFrame> {
        None
    }
}
