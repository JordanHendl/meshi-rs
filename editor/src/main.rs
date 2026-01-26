mod project;
mod render_backend;
mod runtime;
mod ui;

fn main() {
    editor::run();
}

mod editor {
    use crate::{
        project::ProjectManager, render_backend::select_render_backend, runtime::RuntimeBridge,
        ui::EditorUi,
    };

    pub fn run() {
        // TODO: initialize Meshi engine via the plugin entry point.
        let mut ui = EditorUi::new();
        let mut runtime = RuntimeBridge::new();
        let mut render_backend = select_render_backend();
        let project_manager = ProjectManager::load_or_create(ProjectManager::default_config_path());

        // Placeholder frame loop.
        loop {
            render_backend.render_frame(&mut ui, &project_manager);
            // TODO: submit GUI frame to Meshi render engine, egui, or Qt bridge.
            runtime.tick();
        }
    }
}
