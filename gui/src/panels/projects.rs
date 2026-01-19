use egui::Ui;

#[derive(Debug, Clone)]
pub struct ProjectSummary {
    pub name: String,
    pub path: String,
    pub last_opened: String,
}

pub trait ProjectProvider {
    fn recent_projects(&self) -> Vec<ProjectSummary>;
}

pub struct DummyProjectProvider;

impl ProjectProvider for DummyProjectProvider {
    fn recent_projects(&self) -> Vec<ProjectSummary> {
        // ProjectProvider TODO: replace DummyProjectProvider with real project system hook.
        vec![
            ProjectSummary {
                name: "New Prototype".to_string(),
                path: "~/meshi/projects/new-prototype".to_string(),
                last_opened: "Today".to_string(),
            },
            ProjectSummary {
                name: "Sandbox".to_string(),
                path: "~/meshi/projects/sandbox".to_string(),
                last_opened: "Yesterday".to_string(),
            },
        ]
    }
}

pub fn render_projects_panel(ui: &mut Ui, provider: &dyn ProjectProvider) {
    ui.heading("Project");
    ui.separator();

    ui.label("Create and manage Meshi projects.");
    ui.horizontal(|ui| {
        if ui.button("New Project").clicked() {
            // TODO: Hook up project creation flow.
        }
        if ui.button("Open Project").clicked() {
            // TODO: Hook up project open flow.
        }
    });

    ui.add_space(8.0);
    ui.heading("Recent");
    ui.separator();

    let projects = provider.recent_projects();
    if projects.is_empty() {
        ui.label("No recent projects.");
        return;
    }

    for project in projects {
        ui.group(|ui| {
            ui.label(&project.name);
            ui.monospace(&project.path);
            ui.small(&project.last_opened);
        });
    }
}
