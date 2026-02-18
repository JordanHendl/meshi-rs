use std::{fs, path::Path};

use crate::project::ProjectManager;

pub struct ProjectTreeEntry {
    pub label: String,
    pub path: Option<std::path::PathBuf>,
}

impl ProjectTreeEntry {
    pub fn text(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            path: None,
        }
    }

    pub fn file(label: impl Into<String>, path: std::path::PathBuf) -> Self {
        Self {
            label: label.into(),
            path: Some(path),
        }
    }
}

pub fn project_structure_entries(project_manager: &ProjectManager) -> Vec<ProjectTreeEntry> {
    let Some(metadata) = project_manager.metadata() else {
        return vec![ProjectTreeEntry::text("No active project")];
    };

    let root = Path::new(&metadata.root_path);
    let app_root = root.join("apps").join("hello_engine");

    let mut entries = vec![
        ProjectTreeEntry::text(format!("Project: {}", metadata.name)),
        ProjectTreeEntry::text(format!("Root: {}", metadata.root_path)),
        ProjectTreeEntry::text("database/"),
        ProjectTreeEntry::text("apps/"),
        ProjectTreeEntry::text("  hello_engine/"),
    ];

    for file_name in ["main.cpp", "CMakeLists.txt", "example_helper.hpp"] {
        let file_path = app_root.join(file_name);
        let marker = if file_path.exists() { "✓" } else { "•" };
        entries.push(ProjectTreeEntry::file(
            format!("    {} {}", marker, file_name),
            file_path,
        ));
    }

    if let Ok(entries_dir) = fs::read_dir(root.join("database")) {
        let count = entries_dir.flatten().count();
        entries.push(ProjectTreeEntry::text(format!(
            "database entries: {}",
            count
        )));
    }

    entries
}
