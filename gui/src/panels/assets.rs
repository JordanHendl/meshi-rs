use egui::{Align, Color32, Layout, RichText, Ui, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetFilter {
    All,
    Models,
    Textures,
    Materials,
    Scripts,
    Audio,
}

impl AssetFilter {
    fn label(self) -> &'static str {
        match self {
            AssetFilter::All => "All",
            AssetFilter::Models => "Models",
            AssetFilter::Textures => "Textures",
            AssetFilter::Materials => "Materials",
            AssetFilter::Scripts => "Scripts",
            AssetFilter::Audio => "Audio",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub name: String,
    pub asset_type: String,
    pub path: String,
    pub status: String,
    pub thumbnail_label: String,
}

#[derive(Debug, Clone)]
pub struct ImportJob {
    pub source_file: String,
    pub asset_name: String,
    pub asset_type: String,
    pub status: String,
    pub last_imported: String,
}

#[derive(Debug, Clone)]
pub struct AssetMetadata {
    pub asset_name: String,
    pub source_file: String,
    pub import_preset: String,
    pub vertex_count: String,
    pub material_count: String,
    pub tags: Vec<String>,
}

pub struct AssetBrowserState {
    pub search: String,
    pub filter: AssetFilter,
}

pub fn render_assets_panel(
    ui: &mut Ui,
    state: &mut AssetBrowserState,
    assets: &[AssetEntry],
    import_jobs: &[ImportJob],
    metadata: &AssetMetadata,
) {
    ui.heading("Asset Browser");
    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Search");
        ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text("Filter by name or path")
                .desired_width(f32::INFINITY),
        );
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        for filter in [
            AssetFilter::All,
            AssetFilter::Models,
            AssetFilter::Textures,
            AssetFilter::Materials,
            AssetFilter::Scripts,
            AssetFilter::Audio,
        ] {
            ui.selectable_value(&mut state.filter, filter, filter.label());
        }
    });

    ui.add_space(10.0);
    ui.heading("Library");
    ui.separator();

    let query = state.search.to_lowercase();
    let filtered_assets = assets.iter().filter(|asset| {
        let matches_filter = match state.filter {
            AssetFilter::All => true,
            _ => asset.asset_type.eq_ignore_ascii_case(state.filter.label()),
        };
        let matches_query = query.is_empty()
            || asset.name.to_lowercase().contains(&query)
            || asset.path.to_lowercase().contains(&query);
        matches_filter && matches_query
    });

    egui::Grid::new("asset_grid")
        .num_columns(3)
        .spacing(Vec2::new(12.0, 12.0))
        .show(ui, |ui| {
            for (index, asset) in filtered_assets.enumerate() {
                draw_asset_tile(ui, asset);
                if (index + 1) % 3 == 0 {
                    ui.end_row();
                }
            }
        });

    ui.add_space(12.0);
    ui.heading("Import Pipeline");
    ui.separator();

    ui.with_layout(Layout::left_to_right().with_cross_align(Align::Center), |ui| {
        let drop_size = Vec2::new(ui.available_width() * 0.6, 80.0);
        let (rect, _response) = ui.allocate_exact_size(drop_size, egui::Sense::hover());
        ui.painter().rect_filled(rect, 6.0, Color32::from_gray(30));
        ui.painter().rect_stroke(rect, 6.0, (1.0, Color32::DARK_GRAY));
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Drag & drop files here",
            egui::FontId::proportional(14.0),
            Color32::GRAY,
        );

        ui.add_space(12.0);
        ui.vertical(|ui| {
            if ui.button("Import Files").clicked() {
                // TODO: Hook up file dialog + import queue.
            }
            if ui.button("Reimport All").clicked() {
                // TODO: Hook up reimport pipeline.
            }
            if ui.button("Import Settings").clicked() {
                // TODO: Hook up importer settings.
            }
        });
    });

    ui.add_space(8.0);
    ui.label(RichText::new("Queued Imports").strong());
    for job in import_jobs {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(&job.asset_name);
                ui.monospace(&job.asset_type);
                ui.label(&job.status);
            });
            ui.horizontal(|ui| {
                ui.label(&job.source_file);
                ui.small(format!("Last: {}", job.last_imported));
            });
        });
    }

    ui.add_space(12.0);
    ui.heading("Selected Asset Metadata");
    ui.separator();
    ui.group(|ui| {
        ui.label(format!("Asset: {}", metadata.asset_name));
        ui.label(format!("Source: {}", metadata.source_file));
        ui.label(format!("Preset: {}", metadata.import_preset));
        ui.label(format!("Vertices: {}", metadata.vertex_count));
        ui.label(format!("Materials: {}", metadata.material_count));
        ui.horizontal_wrapped(|ui| {
            ui.label("Tags:");
            for tag in &metadata.tags {
                ui.add(egui::Label::new(RichText::new(tag).code()));
            }
        });
    });
}

fn draw_asset_tile(ui: &mut Ui, asset: &AssetEntry) {
    ui.vertical(|ui| {
        let thumbnail_size = Vec2::new(96.0, 72.0);
        let (rect, _response) = ui.allocate_exact_size(thumbnail_size, egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 6.0, Color32::from_gray(24));
        ui.painter()
            .rect_stroke(rect, 6.0, (1.0, Color32::from_gray(60)));
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &asset.thumbnail_label,
            egui::FontId::proportional(16.0),
            Color32::LIGHT_GRAY,
        );
        ui.label(RichText::new(&asset.name).strong());
        ui.horizontal(|ui| {
            ui.monospace(&asset.asset_type);
            ui.small(&asset.status);
        });
        ui.small(&asset.path);
    });
}
