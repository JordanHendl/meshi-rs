use crate::{
    runtime::{RuntimeControlState, RuntimeLogEntry, RuntimeLogLevel, RuntimeStatus},
    ui::project_tree::ProjectTreeEntry,
};

use super::actions::UiAction;

/// Central place for panel widgets.
/// Add new GUI elements by creating a `draw_*_panel` function in this module
/// and calling it from `EditorUi::build_egui`.
pub fn draw_project_panel(ui: &mut egui::Ui, entries: Vec<ProjectTreeEntry>) -> Option<UiAction> {
    ui.heading("Project / Scene");
    ui.separator();

    let mut action = None;
    for entry in entries {
        if let Some(path) = entry.path {
            let response = ui.selectable_label(false, entry.label);
            if response.double_clicked() {
                action = Some(UiAction::OpenProjectFile(path));
            }
        } else {
            ui.label(entry.label);
        }
    }

    action
}

pub fn draw_inspector_panel(ui: &mut egui::Ui) {
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
}

pub fn draw_viewport_panel(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    runtime_controls: &mut RuntimeControlState,
    runtime_texture: Option<&egui::TextureHandle>,
    viewport_pixels: [u32; 2],
) {
    ui.heading("Engine Preview");
    ui.separator();
    ui.horizontal(|ui| {
        let play_label = if runtime_controls.playing {
            "Pause"
        } else {
            "Play"
        };
        if ui.button(play_label).clicked() {
            runtime_controls.playing = !runtime_controls.playing;
        }
        if ui.button("Step").clicked() {
            runtime_controls.request_step();
        }

        ui.separator();
        ui.checkbox(&mut runtime_controls.hot_reload_enabled, "Hot Reload C++");

        let status = if runtime_controls.playing {
            "Running"
        } else {
            "Paused"
        };
        ui.label(status);
    });
    ui.separator();

    let pixels_per_point = ctx.pixels_per_point();
    let viewport_points = egui::vec2(
        viewport_pixels[0] as f32 / pixels_per_point,
        viewport_pixels[1] as f32 / pixels_per_point,
    );

    if let Some(texture) = runtime_texture {
        ui.centered_and_justified(|ui| {
            ui.add(egui::Image::new(texture).fit_to_exact_size(viewport_points));
        });
    } else {
        ui.centered_and_justified(|ui| {
            ui.label("Waiting for runtime frame...");
        });
    }
}

pub fn draw_runtime_logs_panel(
    ui: &mut egui::Ui,
    runtime_status: RuntimeStatus,
    runtime_logs: &[RuntimeLogEntry],
    runtime_error: Option<&str>,
) {
    ui.horizontal(|ui| {
        ui.heading("Runtime Logs");
        ui.separator();
        ui.label(match runtime_status {
            RuntimeStatus::Idle => "Idle",
            RuntimeStatus::Building => "Building",
            RuntimeStatus::Running => "Running",
            RuntimeStatus::Failed => "Failed",
        });
    });

    if let Some(error) = runtime_error {
        ui.colored_label(egui::Color32::RED, error);
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for entry in runtime_logs {
                let color = match entry.level {
                    RuntimeLogLevel::Info => egui::Color32::LIGHT_GRAY,
                    RuntimeLogLevel::Warn => egui::Color32::YELLOW,
                    RuntimeLogLevel::Error => egui::Color32::LIGHT_RED,
                };
                ui.colored_label(color, &entry.message);
            }
        });
}
