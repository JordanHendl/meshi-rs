use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    BuildProject,
    BuildAndRun,
    RebuildAll,
    GenerateBindings,
    CreateProject,
    OpenProject,
    OpenWorkspace,
    SaveAll,
    OpenProjectFile(PathBuf),
}

pub const ACTION_BUILD_PROJECT: u32 = 1;
pub const ACTION_BUILD_AND_RUN: u32 = 2;
pub const ACTION_REBUILD_ALL: u32 = 3;
pub const ACTION_GENERATE_BINDINGS: u32 = 4;
pub const ACTION_CREATE_PROJECT: u32 = 5;
pub const ACTION_OPEN_PROJECT: u32 = 6;
pub const ACTION_OPEN_WORKSPACE: u32 = 7;
pub const ACTION_SAVE_ALL: u32 = 8;
