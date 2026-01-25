use serde::{Deserialize, Serialize};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    #[serde(default = "default_project_name")]
    pub name: String,
    #[serde(default)]
    pub root_path: String,
    #[serde(default = "default_engine_version")]
    pub engine_version: String,
    #[serde(default)]
    pub recent_projects: Vec<String>,
}

impl Default for ProjectMetadata {
    fn default() -> Self {
        Self {
            name: default_project_name(),
            root_path: String::new(),
            engine_version: default_engine_version(),
            recent_projects: Vec::new(),
        }
    }
}

pub struct ProjectManager {
    config_path: PathBuf,
    metadata: ProjectMetadata,
}

impl ProjectManager {
    pub fn load_or_create(config_path: PathBuf) -> Self {
        if config_path.exists() {
            if let Ok(metadata) = Self::load_metadata(&config_path) {
                return Self {
                    config_path,
                    metadata,
                };
            }
        }

        let manager = Self {
            config_path,
            metadata: ProjectMetadata::default(),
        };
        let _ = manager.save();
        manager
    }

    pub fn default_config_path() -> PathBuf {
        if let Ok(config_path) = env::var("MESHI_EDITOR_CONFIG") {
            return PathBuf::from(config_path);
        }

        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join(".meshi").join("Project.toml");
        }

        if let Ok(home) = env::var("USERPROFILE") {
            return PathBuf::from(home).join(".meshi").join("Project.toml");
        }

        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("Project.toml")
    }

    pub fn metadata(&self) -> &ProjectMetadata {
        &self.metadata
    }

    pub fn recent_projects_with_status(&self) -> Vec<(String, bool)> {
        self.metadata
            .recent_projects
            .iter()
            .map(|path| {
                let exists = Self::validate_project_path(Path::new(path));
                (path.clone(), exists)
            })
            .collect()
    }

    pub fn save(&self) -> io::Result<()> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = toml::to_string_pretty(&self.metadata)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        fs::write(&self.config_path, data)?;
        Ok(())
    }

    pub fn validate_project_path(path: &Path) -> bool {
        path.exists() && path.is_dir()
    }

    fn load_metadata(path: &Path) -> io::Result<ProjectMetadata> {
        let contents = fs::read_to_string(path)?;
        toml::from_str(&contents).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }
}

fn default_project_name() -> String {
    "Untitled Project".to_string()
}

fn default_engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
