use egui::Ui;

#[derive(Debug, Clone)]
pub struct ScriptStatus {
    pub name: String,
    pub state: ScriptState,
    pub last_run_ms: Option<u128>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum ScriptState {
    Idle,
    Running,
    Failed,
}

pub trait ScriptProvider {
    fn scripts(&self) -> Vec<ScriptStatus>;
    fn empty_message(&self) -> &'static str;
}

pub fn render_scripts_panel(ui: &mut Ui, provider: &dyn ScriptProvider) {
    ui.heading("Scripts");
    ui.separator();

    let scripts = provider.scripts();
    if scripts.is_empty() {
        ui.label(provider.empty_message());
        return;
    }

    for script in scripts {
        ui.horizontal(|ui| {
            ui.label(&script.name);
            ui.monospace(format!("{:?}", script.state));
            if let Some(last_run_ms) = script.last_run_ms {
                ui.label(format!("{} ms", last_run_ms));
            }
        });

        for error in script.errors {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
    }
}
