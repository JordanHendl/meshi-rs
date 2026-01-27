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
        runtime::{RuntimeBridge, RuntimeControlState, RuntimeLogLevel},
        ui::{EditorUi, UiAction},
    };
    use eframe::{Frame, NativeOptions};
    use egui::Context;

    pub fn run() {
        let project_manager = ProjectManager::load_or_create(ProjectManager::default_config_path());
        let app = EditorApp::new(project_manager);
        let options = NativeOptions::default();

        eframe::run_native("Meshi Editor", options, Box::new(|_cc| Ok(Box::new(app))))
            .expect("Failed to start editor window");
    }

    struct EditorApp {
        ui: EditorUi,
        project_manager: ProjectManager,
        runtime: RuntimeBridge,
        runtime_controls: RuntimeControlState,
    }

    impl EditorApp {
        fn new(project_manager: ProjectManager) -> Self {
            Self {
                ui: EditorUi::new(),
                project_manager,
                runtime: RuntimeBridge::new(),
                runtime_controls: RuntimeControlState::default(),
            }
        }
    }

    impl eframe::App for EditorApp {
        fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
            let output = self.ui.build_egui(
                ctx,
                &self.project_manager,
                &mut self.runtime_controls,
                self.runtime.status(),
                self.runtime.logs(),
                self.runtime.last_error(),
            );
            if let Some(action) = output.action {
                match action {
                    UiAction::BuildProject => self.runtime.build_project(),
                    UiAction::BuildAndRun => self.runtime.build_and_run(),
                    UiAction::RebuildAll => self.runtime.rebuild_all(),
                    UiAction::GenerateBindings => self.runtime.log_message(
                        RuntimeLogLevel::Warn,
                        "Generate C++ Bindings is not implemented yet.",
                    ),
                }
            }
            let delta_time = ctx.input(|input| input.unstable_dt);
            let rendered =
                self.runtime
                    .tick(delta_time, &mut self.runtime_controls, output.metrics.pixels);
            self.ui
                .update_runtime_texture(ctx, self.runtime.latest_frame());
            if self.runtime_controls.playing || rendered {
                ctx.request_repaint();
            }
        }
    }
}
