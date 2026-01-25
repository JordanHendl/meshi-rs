mod project;
mod runtime;
mod ui;

fn main() {
    editor::run();
}

mod editor {
    use crate::{project::ProjectManager, runtime::RuntimeBridge, ui::EditorUi};
    use meshi_graphics::gui::GuiContext;

    pub fn run() {
        // TODO: initialize Meshi engine via the plugin entry point.
        let mut gui = GuiContext::new();
        let mut ui = EditorUi::new();
        let mut runtime = RuntimeBridge::new();
        let project_manager = ProjectManager::load_or_create(ProjectManager::default_config_path());

        // Placeholder frame loop.
        for _frame in 0..1 {
            ui.build(&mut gui, &project_manager);
            let _frame = gui.build_frame();
            // TODO: submit GUI frame to Meshi render engine.
            runtime.tick();
        }
    }
}
