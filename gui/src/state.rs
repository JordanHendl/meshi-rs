use crate::panels::assets::{AssetEntry, AssetMetadata, ImportJob};
use crate::panels::projects::{ProjectProvider, ProjectSummary};
use crate::panels::scripts::{ScriptProvider, ScriptState, ScriptStatus};

pub struct EditorState {
    pub projects: Vec<ProjectSummary>,
    pub scripts: Vec<ScriptStatus>,
    pub asset_entries: Vec<AssetEntry>,
    pub import_jobs: Vec<ImportJob>,
    pub asset_metadata: AssetMetadata,
    pub scene_tree: SceneNode,
}

impl EditorState {
    pub fn new() -> Self {
        Self {
            projects: vec![
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
            ],
            scripts: vec![
                ScriptStatus {
                    name: "player_controller.rs".to_string(),
                    state: ScriptState::Running,
                    last_run_ms: Some(1_625),
                    errors: vec![],
                },
                ScriptStatus {
                    name: "environment_fx.rs".to_string(),
                    state: ScriptState::Failed,
                    last_run_ms: Some(2_310),
                    errors: vec![
                        "error: missing import `meshi_fx`".to_string(),
                        "error: compilation aborted".to_string(),
                    ],
                },
            ],
            asset_entries: vec![
                AssetEntry {
                    name: "Player".to_string(),
                    asset_type: "Models".to_string(),
                    path: "Assets/Models/player.fbx".to_string(),
                    status: "Imported".to_string(),
                    thumbnail_label: "FBX".to_string(),
                },
                AssetEntry {
                    name: "Hero Texture".to_string(),
                    asset_type: "Textures".to_string(),
                    path: "Assets/Textures/hero_albedo.png".to_string(),
                    status: "Ready".to_string(),
                    thumbnail_label: "PNG".to_string(),
                },
                AssetEntry {
                    name: "Starter Material".to_string(),
                    asset_type: "Materials".to_string(),
                    path: "Assets/Materials/starter.mat".to_string(),
                    status: "Linked".to_string(),
                    thumbnail_label: "MAT".to_string(),
                },
                AssetEntry {
                    name: "Wind Loop".to_string(),
                    asset_type: "Audio".to_string(),
                    path: "Assets/Audio/wind_loop.ogg".to_string(),
                    status: "Streaming".to_string(),
                    thumbnail_label: "OGG".to_string(),
                },
                AssetEntry {
                    name: "Player Controller".to_string(),
                    asset_type: "Scripts".to_string(),
                    path: "Assets/Scripts/player_controller.rs".to_string(),
                    status: "Compiled".to_string(),
                    thumbnail_label: "RS".to_string(),
                },
            ],
            import_jobs: vec![
                ImportJob {
                    source_file: "~/Downloads/robot.glb".to_string(),
                    asset_name: "Robot".to_string(),
                    asset_type: "Models".to_string(),
                    status: "Waiting".to_string(),
                    last_imported: "Never".to_string(),
                },
                ImportJob {
                    source_file: "~/Downloads/terrain_albedo.tif".to_string(),
                    asset_name: "Terrain Albedo".to_string(),
                    asset_type: "Textures".to_string(),
                    status: "Queued".to_string(),
                    last_imported: "2 days ago".to_string(),
                },
            ],
            asset_metadata: AssetMetadata {
                asset_name: "Player".to_string(),
                source_file: "Assets/Models/player.fbx".to_string(),
                import_preset: "Character: High".to_string(),
                vertex_count: "24,320".to_string(),
                material_count: "3".to_string(),
                tags: vec!["character".to_string(), "biped".to_string(), "hero".to_string()],
            },
            scene_tree: SceneNode::new(
                "Root",
                vec![
                    SceneNode::new("Camera", vec![]),
                    SceneNode::new("Directional Light", vec![]),
                    SceneNode::new(
                        "Player",
                        vec![
                            SceneNode::new("Weapon", vec![]),
                            SceneNode::new("Camera Pivot", vec![]),
                        ],
                    ),
                    SceneNode::new(
                        "Environment",
                        vec![
                            SceneNode::new("Ground", vec![]),
                            SceneNode::new("Rocks", vec![]),
                            SceneNode::new("Foliage", vec![]),
                        ],
                    ),
                ],
            ),
        }
    }
}

impl ProjectProvider for EditorState {
    fn recent_projects(&self) -> Vec<ProjectSummary> {
        self.projects.clone()
    }
}

impl ScriptProvider for EditorState {
    fn scripts(&self) -> Vec<ScriptStatus> {
        self.scripts.clone()
    }

    fn empty_message(&self) -> &'static str {
        "no scripts registered"
    }
}

#[derive(Clone)]
pub struct SceneNode {
    pub name: String,
    pub children: Vec<SceneNode>,
}

impl SceneNode {
    pub fn new(name: impl Into<String>, children: Vec<SceneNode>) -> Self {
        Self {
            name: name.into(),
            children,
        }
    }
}
