mod project;
mod render_backend;
mod runtime;
mod terrain;
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
    use std::{
        io,
        path::{Path, PathBuf},
        process::Command,
    };

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
        fn new(mut project_manager: ProjectManager) -> Self {
            let mut runtime = RuntimeBridge::new();
            if project_manager.metadata().is_none() {
                match project_manager.create_project(Some("Default Project".to_string())) {
                    Ok(project) => runtime.log_message(
                        RuntimeLogLevel::Info,
                        format!("Created default project at {}", project.root_path),
                    ),
                    Err(err) => runtime.log_message(
                        RuntimeLogLevel::Error,
                        format!("Failed to create default project: {}", err),
                    ),
                }
            }

            Self {
                ui: EditorUi::new(),
                project_manager,
                runtime,
                runtime_controls: RuntimeControlState::default(),
            }
        }
    }

    fn open_in_system_editor(path: &Path) -> io::Result<()> {
        if !path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Path does not exist: {}", path.display()),
            ));
        }

        let status = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .args(["/C", "start", "", &path.to_string_lossy()])
                .status()?
        } else if cfg!(target_os = "macos") {
            Command::new("open").arg(path).status()?
        } else {
            Command::new("xdg-open").arg(path).status()?
        };

        if status.success() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Editor opener exited with status {}", status),
            ))
        }
    }

    impl eframe::App for EditorApp {
        fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
            let project_viewport_pixels = self
                .project_manager
                .metadata()
                .map(|metadata| metadata.viewport_pixels)
                .unwrap_or([1280, 720]);
            let output = self.ui.build_egui(
                ctx,
                &self.project_manager,
                &mut self.runtime_controls,
                self.runtime.status(),
                self.runtime.logs(),
                self.runtime.last_error(),
                project_viewport_pixels,
            );
            if let Some(action) = output.action {
                let active_project_root = self.project_manager.active_project_root();
                match action {
                    UiAction::BuildProject => {
                        self.runtime.build_project(active_project_root.as_deref())
                    }
                    UiAction::BuildAndRun => {
                        self.runtime.build_and_run(active_project_root.as_deref())
                    }
                    UiAction::RebuildAll => {
                        self.runtime.rebuild_all(active_project_root.as_deref())
                    }
                    UiAction::GenerateBindings => self.runtime.log_message(
                        RuntimeLogLevel::Warn,
                        "Generate C++ Bindings is not implemented yet.",
                    ),
                    UiAction::CreateProject => {
                        if let Err(err) = self.project_manager.create_project(None) {
                            self.runtime.log_message(
                                RuntimeLogLevel::Error,
                                format!("Failed to create project: {}", err),
                            );
                        } else {
                            self.runtime.log_message(
                                RuntimeLogLevel::Info,
                                "Created new project in workspace.",
                            );
                        }
                    }
                    UiAction::OpenProject => {
                        let workspace_root = self.project_manager.workspace_root();
                        let open_target = self
                            .project_manager
                            .recent_projects_with_status()
                            .into_iter()
                            .find_map(|(path, exists)| exists.then(|| PathBuf::from(path)))
                            .unwrap_or(workspace_root);
                        if let Err(err) = self.project_manager.open_project(open_target) {
                            self.runtime.log_message(
                                RuntimeLogLevel::Error,
                                format!("Failed to open project: {}", err),
                            );
                        } else {
                            self.runtime
                                .log_message(RuntimeLogLevel::Info, "Opened project.");
                        }
                    }
                    UiAction::OpenProjectFile(path) => {
                        if let Err(err) = open_in_system_editor(&path) {
                            self.runtime.log_message(
                                RuntimeLogLevel::Error,
                                format!("Failed to open {}: {}", path.display(), err),
                            );
                        } else {
                            self.runtime.log_message(
                                RuntimeLogLevel::Info,
                                format!("Opened {} in system editor.", path.display()),
                            );
                        }
                    }
                    UiAction::OpenWorkspace => {
                        let workspace_root = self.project_manager.workspace_root();
                        if let Err(err) = self.project_manager.select_workspace(workspace_root) {
                            self.runtime.log_message(
                                RuntimeLogLevel::Error,
                                format!("Failed to select workspace: {}", err),
                            );
                        } else {
                            self.runtime
                                .log_message(RuntimeLogLevel::Info, "Workspace selection updated.");
                        }
                    }
                    UiAction::SaveAll => {
                        if let Err(err) = self.project_manager.save_all() {
                            self.runtime.log_message(
                                RuntimeLogLevel::Error,
                                format!("Failed to save project metadata: {}", err),
                            );
                        } else {
                            self.runtime
                                .log_message(RuntimeLogLevel::Info, "Saved project data.");
                        }
                    }
                }
            }
            let delta_time = ctx.input(|input| input.unstable_dt);
            let rendered = self.runtime.tick(
                delta_time,
                &mut self.runtime_controls,
                output.metrics.pixels,
            );
            self.ui
                .update_runtime_texture(ctx, self.runtime.latest_frame());
            if self.runtime_controls.playing || rendered {
                ctx.request_repaint();
            }
        }
    }
}
