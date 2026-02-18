use meshi_graphics::gui::{Menu, MenuBar, MenuItem};

use crate::project::ProjectManager;

use super::actions::{
    ACTION_BUILD_AND_RUN, ACTION_BUILD_PROJECT, ACTION_CREATE_PROJECT, ACTION_GENERATE_BINDINGS,
    ACTION_OPEN_PROJECT, ACTION_OPEN_WORKSPACE, ACTION_REBUILD_ALL, ACTION_SAVE_ALL, UiAction,
};

pub fn create_menu_bar() -> MenuBar {
    MenuBar {
        menus: vec![
            Menu {
                label: "File".to_string(),
                items: Vec::new(),
            },
            Menu {
                label: "Edit".to_string(),
                items: Vec::new(),
            },
            Menu {
                label: "Build".to_string(),
                items: Vec::new(),
            },
            Menu {
                label: "View".to_string(),
                items: Vec::new(),
            },
            Menu {
                label: "Help".to_string(),
                items: Vec::new(),
            },
        ],
    }
}

pub fn refresh_file_menu(menu_bar: &mut MenuBar, project_manager: &ProjectManager) {
    let recent_entries = project_manager.recent_projects_with_status();
    let recent_menu_items = if recent_entries.is_empty() {
        let mut item = MenuItem::new("No recent projects");
        item.enabled = false;
        vec![item]
    } else {
        recent_entries
            .into_iter()
            .map(|(path, exists)| {
                let mut item = MenuItem::new(path);
                item.enabled = exists;
                item
            })
            .collect()
    };

    let file_items = vec![
        {
            let mut item = MenuItem::new("New Project");
            item.action_id = Some(ACTION_CREATE_PROJECT);
            item
        },
        {
            let mut item = MenuItem::new("Open Project");
            item.action_id = Some(ACTION_OPEN_PROJECT);
            item
        },
        {
            let mut item = MenuItem::new("Open Workspace");
            item.action_id = Some(ACTION_OPEN_WORKSPACE);
            item
        },
        {
            let mut item = MenuItem::new("Save All");
            item.action_id = Some(ACTION_SAVE_ALL);
            item
        },
        MenuItem::separator(),
        MenuItem::new("Recent Projects").with_submenu(recent_menu_items),
    ];

    if let Some(file_menu) = menu_bar.menus.iter_mut().find(|menu| menu.label == "File") {
        file_menu.items = file_items;
    }
}

pub fn refresh_build_menu(menu_bar: &mut MenuBar) {
    let build_items = vec![
        {
            let mut item = MenuItem::new("Build Project");
            item.action_id = Some(ACTION_BUILD_PROJECT);
            item
        },
        {
            let mut item = MenuItem::new("Build & Run");
            item.action_id = Some(ACTION_BUILD_AND_RUN);
            item
        },
        {
            let mut item = MenuItem::new("Rebuild All");
            item.action_id = Some(ACTION_REBUILD_ALL);
            item
        },
        MenuItem::separator(),
        {
            let mut item = MenuItem::new("Generate C++ Bindings");
            item.action_id = Some(ACTION_GENERATE_BINDINGS);
            item
        },
    ];

    if let Some(build_menu) = menu_bar.menus.iter_mut().find(|menu| menu.label == "Build") {
        build_menu.items = build_items;
    }
}

pub fn action_for_menu_item(action_id: u32) -> Option<UiAction> {
    match action_id {
        ACTION_BUILD_PROJECT => Some(UiAction::BuildProject),
        ACTION_BUILD_AND_RUN => Some(UiAction::BuildAndRun),
        ACTION_REBUILD_ALL => Some(UiAction::RebuildAll),
        ACTION_GENERATE_BINDINGS => Some(UiAction::GenerateBindings),
        ACTION_CREATE_PROJECT => Some(UiAction::CreateProject),
        ACTION_OPEN_PROJECT => Some(UiAction::OpenProject),
        ACTION_OPEN_WORKSPACE => Some(UiAction::OpenWorkspace),
        ACTION_SAVE_ALL => Some(UiAction::SaveAll),
        _ => None,
    }
}
