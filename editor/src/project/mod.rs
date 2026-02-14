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
    #[serde(default = "default_asset_roots")]
    pub asset_roots: Vec<String>,
    #[serde(default = "default_build_profiles")]
    pub build_profiles: Vec<String>,
    #[serde(default = "default_runtime_target")]
    pub runtime_target: String,
    #[serde(default)]
    pub plugin_list: Vec<String>,
    #[serde(default = "default_viewport_pixels")]
    pub viewport_pixels: [u32; 2],
}

impl Default for ProjectMetadata {
    fn default() -> Self {
        Self {
            name: default_project_name(),
            root_path: String::new(),
            engine_version: default_engine_version(),
            asset_roots: default_asset_roots(),
            build_profiles: default_build_profiles(),
            runtime_target: default_runtime_target(),
            plugin_list: Vec::new(),
            viewport_pixels: default_viewport_pixels(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GlobalProjectConfig {
    #[serde(default)]
    recent_projects: Vec<String>,
    #[serde(default)]
    workspace_root: Option<String>,
    #[serde(default)]
    last_project: Option<String>,
}

impl Default for GlobalProjectConfig {
    fn default() -> Self {
        Self {
            recent_projects: Vec::new(),
            workspace_root: None,
            last_project: None,
        }
    }
}

pub struct ProjectManager {
    config_path: PathBuf,
    global_config: GlobalProjectConfig,
    active_project: Option<ProjectMetadata>,
}

impl ProjectManager {
    pub fn load_or_create(config_path: PathBuf) -> Self {
        let global_config = if config_path.exists() {
            Self::load_global_config(&config_path).unwrap_or_default()
        } else {
            GlobalProjectConfig::default()
        };

        let mut manager = Self {
            config_path,
            global_config,
            active_project: None,
        };

        if let Some(last_project) = manager.global_config.last_project.clone() {
            if let Ok(project) = manager.open_project(PathBuf::from(last_project)) {
                manager.active_project = Some(project);
            }
        }

        let _ = manager.save_global();
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

    pub fn metadata(&self) -> Option<&ProjectMetadata> {
        self.active_project.as_ref()
    }

    pub fn active_project_root(&self) -> Option<PathBuf> {
        self.active_project
            .as_ref()
            .map(|project| PathBuf::from(&project.root_path))
    }

    pub fn workspace_root(&self) -> PathBuf {
        self.global_config
            .workspace_root
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_workspace_root)
    }

    pub fn recent_projects_with_status(&self) -> Vec<(String, bool)> {
        self.global_config
            .recent_projects
            .iter()
            .map(|path| {
                let exists = Self::validate_project_path(Path::new(path));
                (path.clone(), exists)
            })
            .collect()
    }

    pub fn create_project(&mut self, name: Option<String>) -> io::Result<ProjectMetadata> {
        let workspace_root = self.workspace_root();
        fs::create_dir_all(&workspace_root)?;
        let base_name = name.unwrap_or_else(default_project_name);
        let slug = slugify_project_folder(&base_name);
        let project_root = unique_project_root(&workspace_root, &slug);
        fs::create_dir_all(&project_root)?;

        let mut metadata = ProjectMetadata::default();
        metadata.name = base_name;
        metadata.root_path = project_root.to_string_lossy().to_string();

        self.initialize_project_layout(&metadata)?;

        self.set_active_project(metadata.clone());
        self.save_project_metadata(&metadata)?;
        self.save_global()?;
        Ok(metadata)
    }

    pub fn open_project(&mut self, path: PathBuf) -> io::Result<ProjectMetadata> {
        let root = if path.is_dir() {
            path
        } else {
            path.parent().map(PathBuf::from).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Invalid project path")
            })?
        };

        fs::create_dir_all(&root)?;
        let metadata_path = Self::project_metadata_path(&root);
        let mut metadata = if metadata_path.exists() {
            Self::load_project_metadata(&metadata_path)?
        } else {
            ProjectMetadata::default()
        };

        metadata.root_path = root.to_string_lossy().to_string();
        if metadata.name.trim().is_empty() {
            metadata.name = default_project_name();
        }

        self.initialize_project_layout(&metadata)?;

        self.set_active_project(metadata.clone());
        self.save_project_metadata(&metadata)?;
        self.save_global()?;
        Ok(metadata)
    }

    pub fn select_workspace(&mut self, path: PathBuf) -> io::Result<()> {
        fs::create_dir_all(&path)?;
        self.global_config.workspace_root = Some(path.to_string_lossy().to_string());
        self.save_global()
    }

    pub fn save_all(&mut self) -> io::Result<()> {
        if let Some(project) = self.active_project.clone() {
            self.touch_recent_project(&project.root_path);
            self.save_project_metadata(&project)?;
        }
        self.save_global()
    }

    pub fn validate_project_path(path: &Path) -> bool {
        path.exists() && path.is_dir()
    }

    pub fn default_workspace_root() -> PathBuf {
        if let Ok(workspace_root) = env::var("MESHI_EDITOR_WORKSPACE") {
            return PathBuf::from(workspace_root);
        }

        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join("MeshiWorkspace");
        }

        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    fn set_active_project(&mut self, metadata: ProjectMetadata) {
        self.touch_recent_project(&metadata.root_path);
        self.global_config.last_project = Some(metadata.root_path.clone());
        self.active_project = Some(metadata);
    }

    fn touch_recent_project(&mut self, path: &str) {
        let mut recent = Vec::with_capacity(self.global_config.recent_projects.len() + 1);
        recent.push(path.to_string());
        for entry in &self.global_config.recent_projects {
            if entry != path {
                recent.push(entry.clone());
            }
        }
        recent.truncate(10);
        self.global_config.recent_projects = recent;
    }

    fn save_global(&self) -> io::Result<()> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = toml::to_string_pretty(&self.global_config)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        fs::write(&self.config_path, data)?;
        Ok(())
    }

    fn save_project_metadata(&self, metadata: &ProjectMetadata) -> io::Result<()> {
        let root = Path::new(&metadata.root_path);
        let metadata_path = Self::project_metadata_path(root);
        if let Some(parent) = metadata_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = toml::to_string_pretty(metadata)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        fs::write(metadata_path, data)?;
        Ok(())
    }

    fn project_metadata_path(root: &Path) -> PathBuf {
        root.join(".meshi").join("Project.toml")
    }

    fn initialize_project_layout(&self, metadata: &ProjectMetadata) -> io::Result<()> {
        let root = PathBuf::from(&metadata.root_path);
        let database_root = root.join("database");
        let app_root = root.join("apps").join("hello_engine");

        fs::create_dir_all(&database_root)?;
        fs::create_dir_all(&app_root)?;
        ensure_app_starter_files(&app_root)?;
        Ok(())
    }

    fn load_global_config(path: &Path) -> io::Result<GlobalProjectConfig> {
        let contents = fs::read_to_string(path)?;
        toml::from_str(&contents).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    fn load_project_metadata(path: &Path) -> io::Result<ProjectMetadata> {
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

fn default_asset_roots() -> Vec<String> {
    vec!["assets".to_string(), "database".to_string()]
}

fn default_build_profiles() -> Vec<String> {
    vec!["debug".to_string(), "release".to_string()]
}

fn default_runtime_target() -> String {
    "native".to_string()
}

fn default_viewport_pixels() -> [u32; 2] {
    [1280, 720]
}

fn slugify_project_folder(name: &str) -> String {
    let mut slug = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "untitled-project".to_string()
    } else {
        trimmed.to_string()
    }
}

fn unique_project_root(workspace_root: &Path, slug: &str) -> PathBuf {
    let candidate = workspace_root.join(slug);
    if !candidate.exists() {
        return candidate;
    }

    let mut index = 1;
    loop {
        let with_suffix = workspace_root.join(format!("{}-{}", slug, index));
        if !with_suffix.exists() {
            return with_suffix;
        }
        index += 1;
    }
}

fn ensure_app_starter_files(app_root: &Path) -> io::Result<()> {
    let editor_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let meshi_cpp_root = editor_root
        .parent()
        .unwrap_or(&editor_root)
        .join("meshi-cpp");
    let hello_engine_root = meshi_cpp_root
        .join("src")
        .join("examples")
        .join("hello_engine");
    let helper_header = meshi_cpp_root
        .join("src")
        .join("examples")
        .join("example_helper.hpp");

    copy_or_write_default(
        &hello_engine_root.join("main.cpp"),
        &app_root.join("main.cpp"),
        FALLBACK_HELLO_ENGINE_MAIN,
    )?;
    copy_or_write_default(
        &hello_engine_root.join("CMakeLists.txt"),
        &app_root.join("CMakeLists.txt"),
        FALLBACK_HELLO_ENGINE_CMAKELISTS,
    )?;
    copy_or_write_default(
        &helper_header,
        &app_root.join("example_helper.hpp"),
        FALLBACK_EXAMPLE_HELPER,
    )?;

    Ok(())
}

fn copy_or_write_default(
    source: &Path,
    destination: &Path,
    fallback_contents: &str,
) -> io::Result<()> {
    if destination.exists() {
        return Ok(());
    }
    if source.exists() {
        fs::copy(source, destination)?;
    } else {
        fs::write(destination, fallback_contents)?;
    }
    Ok(())
}

const FALLBACK_HELLO_ENGINE_MAIN: &str = r#"#include <meshi/meshi.hpp>

int main() {
  meshi::initialize_meshi_engine(meshi::EngineInfo{
      .application_name = "Hello Engine!",
      .application_root = ".",
  });

  while (meshi::engine()) {
    meshi::engine()->update();
  }
  return 0;
}
"#;

const FALLBACK_HELLO_ENGINE_CMAKELISTS: &str = r#"cmake_minimum_required(VERSION 3.16)
project(meshi_app LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(MESHI_WRAPPER_DIR "" CACHE PATH "Path to the Meshi C++ wrapper repo")
if (NOT MESHI_WRAPPER_DIR)
  message(FATAL_ERROR "MESHI_WRAPPER_DIR is not set. Provide the path to the Meshi C++ wrapper repo.")
endif()

add_subdirectory(${MESHI_WRAPPER_DIR} meshi_wrapper)
add_executable(meshi_app main.cpp)
target_link_libraries(meshi_app PRIVATE meshi_wrapper)
"#;

const FALLBACK_EXAMPLE_HELPER: &str = r#"#pragma once
#ifndef EXAMPLE_APP_DIR
  #define EXAMPLE_APP_DIR
#endif
"#;
