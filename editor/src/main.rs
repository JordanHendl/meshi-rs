mod project;
mod render_backend;
mod runtime;
mod ui;

fn main() {
    editor::run();
}

mod editor {
    use crate::{
        project::ProjectManager,
        runtime::RuntimeBridge,
        ui::EditorUi,
    };
    use eframe::{Frame, NativeOptions};
    use egui::Context;

    pub fn run() {
        let project_manager = ProjectManager::load_or_create(ProjectManager::default_config_path());
        let app = EditorApp::new(project_manager);
        let options = NativeOptions::default();

        eframe::run_native(
            "Meshi Editor",
            options,
            Box::new(|_cc| Ok(Box::new(app))),
        )
        .expect("Failed to start editor window");
    }

    struct EditorApp {
        ui: EditorUi,
        project_manager: ProjectManager,
        runtime: RuntimeBridge,
    }

    impl EditorApp {
        fn new(project_manager: ProjectManager) -> Self {
            Self {
                ui: EditorUi::new(),
                project_manager,
                runtime: RuntimeBridge::new(),
            }
        }
    }

    impl eframe::App for EditorApp {
        fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
            self.ui.build_egui(ctx, &self.project_manager);
            self.runtime.tick();
        }
    }
}
