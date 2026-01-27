use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::project::ProjectMetadata;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssetKind {
    Mesh,
    Texture,
    Material,
    Scene,
    Shader,
    Unknown,
}

impl AssetKind {
    pub fn label(self) -> &'static str {
        match self {
            AssetKind::Mesh => "Mesh",
            AssetKind::Texture => "Texture",
            AssetKind::Material => "Material",
            AssetKind::Scene => "Scene",
            AssetKind::Shader => "Shader",
            AssetKind::Unknown => "Unknown",
        }
    }

    pub fn imported_extension(self) -> &'static str {
        match self {
            AssetKind::Mesh => "mesh",
            AssetKind::Texture => "texture",
            AssetKind::Material => "material",
            AssetKind::Scene => "scene",
            AssetKind::Shader => "shader",
            AssetKind::Unknown => "asset",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetStatus {
    UpToDate,
    NeedsImport,
    NeedsReimport,
    MissingSource,
}

impl AssetStatus {
    pub fn label(self) -> &'static str {
        match self {
            AssetStatus::UpToDate => "Up to date",
            AssetStatus::NeedsImport => "Needs import",
            AssetStatus::NeedsReimport => "Needs reimport",
            AssetStatus::MissingSource => "Missing source",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssetRecord {
    pub path: PathBuf,
    pub display_name: String,
    pub kind: AssetKind,
    pub tags: BTreeSet<String>,
    pub imported_assets: Vec<PathBuf>,
    pub status: AssetStatus,
    pub last_modified: Option<SystemTime>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct AssetImporter {
    pub extensions: &'static [&'static str],
    pub kind: AssetKind,
    pub default_tags: &'static [&'static str],
}

#[derive(Debug, Default, Clone)]
pub struct AssetImporterRegistry {
    importers: Vec<AssetImporter>,
}

impl AssetImporterRegistry {
    pub fn default_registry() -> Self {
        Self {
            importers: vec![
                AssetImporter {
                    extensions: &["gltf", "glb", "fbx", "obj", "dae"],
                    kind: AssetKind::Mesh,
                    default_tags: &["mesh", "geometry"],
                },
                AssetImporter {
                    extensions: &["png", "jpg", "jpeg", "tga", "bmp", "hdr", "exr"],
                    kind: AssetKind::Texture,
                    default_tags: &["texture", "image"],
                },
                AssetImporter {
                    extensions: &["mat", "mtl", "material"],
                    kind: AssetKind::Material,
                    default_tags: &["material"],
                },
                AssetImporter {
                    extensions: &["scene"],
                    kind: AssetKind::Scene,
                    default_tags: &["scene"],
                },
                AssetImporter {
                    extensions: &["shader", "wgsl", "vert", "frag"],
                    kind: AssetKind::Shader,
                    default_tags: &["shader"],
                },
            ],
        }
    }

    pub fn importer_for_extension(&self, extension: &str) -> Option<&AssetImporter> {
        let ext = extension.to_ascii_lowercase();
        self.importers
            .iter()
            .find(|importer| importer.extensions.contains(&ext.as_str()))
    }

    pub fn supported_extensions(&self) -> Vec<&'static str> {
        let mut ext = BTreeSet::new();
        for importer in &self.importers {
            ext.extend(importer.extensions.iter().copied());
        }
        ext.into_iter().collect()
    }
}

pub struct AssetDatabase {
    assets: BTreeMap<String, AssetRecord>,
    importer_registry: AssetImporterRegistry,
    last_scan: Option<Instant>,
    refresh_interval: Duration,
}

impl AssetDatabase {
    pub fn new() -> Self {
        Self {
            assets: BTreeMap::new(),
            importer_registry: AssetImporterRegistry::default_registry(),
            last_scan: None,
            refresh_interval: Duration::from_secs(2),
        }
    }

    pub fn assets(&self) -> impl Iterator<Item = (&String, &AssetRecord)> {
        self.assets.iter()
    }

    pub fn supported_extensions(&self) -> Vec<&'static str> {
        self.importer_registry.supported_extensions()
    }

    pub fn tags(&self) -> BTreeSet<String> {
        let mut tags = BTreeSet::new();
        for record in self.assets.values() {
            tags.extend(record.tags.iter().cloned());
        }
        tags
    }

    pub fn refresh_if_due(&mut self, project: Option<&ProjectMetadata>, workspace_root: &Path) {
        let now = Instant::now();
        if let Some(last_scan) = self.last_scan {
            if now.duration_since(last_scan) < self.refresh_interval {
                return;
            }
        }
        self.refresh(project, workspace_root);
        self.last_scan = Some(now);
    }

    pub fn refresh(&mut self, project: Option<&ProjectMetadata>, workspace_root: &Path) {
        let roots = asset_roots(project, workspace_root);
        let mut seen = BTreeSet::new();

        for root in roots {
            self.scan_root(&root, &mut seen);
        }

        for (path, record) in self.assets.iter_mut() {
            if !seen.contains(path) {
                record.status = AssetStatus::MissingSource;
            }
        }
    }

    pub fn reimport_all(&mut self) {
        for record in self.assets.values_mut() {
            if record.status != AssetStatus::MissingSource {
                record.status = AssetStatus::UpToDate;
                if record.imported_assets.is_empty() {
                    record.imported_assets = vec![default_import_path(&record.path, record.kind)];
                }
            }
        }
    }

    pub fn reimport_asset(&mut self, id: &str) {
        if let Some(record) = self.assets.get_mut(id) {
            if record.status != AssetStatus::MissingSource {
                record.status = AssetStatus::UpToDate;
                if record.imported_assets.is_empty() {
                    record.imported_assets = vec![default_import_path(&record.path, record.kind)];
                }
            }
        }
    }

    pub fn update_tags(&mut self, id: &str, tags: BTreeSet<String>) {
        if let Some(record) = self.assets.get_mut(id) {
            record.tags = tags;
        }
    }

    fn scan_root(&mut self, root: &Path, seen: &mut BTreeSet<String>) {
        if !root.exists() {
            return;
        }
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.scan_root(&path, seen);
                continue;
            }

            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase());

            let Some(extension) = extension else {
                continue;
            };
            let Some(importer) = self.importer_registry.importer_for_extension(&extension) else {
                continue;
            };

            let id = path.to_string_lossy().to_string();
            seen.insert(id.clone());

            let metadata = fs::metadata(&path).ok();
            let modified = metadata.as_ref().and_then(|meta| meta.modified().ok());
            let size = metadata.as_ref().map(|meta| meta.len());

            match self.assets.get_mut(&id) {
                Some(record) => {
                    if record.last_modified != modified {
                        record.status = if record.imported_assets.is_empty() {
                            AssetStatus::NeedsImport
                        } else {
                            AssetStatus::NeedsReimport
                        };
                        record.last_modified = modified;
                        record.size_bytes = size;
                    }
                }
                None => {
                    let mut tags = BTreeSet::new();
                    tags.extend(importer.default_tags.iter().map(|tag| tag.to_string()));
                    let display_name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    self.assets.insert(
                        id,
                        AssetRecord {
                            path,
                            display_name,
                            kind: importer.kind,
                            tags,
                            imported_assets: Vec::new(),
                            status: AssetStatus::NeedsImport,
                            last_modified: modified,
                            size_bytes: size,
                        },
                    );
                }
            }
        }
    }
}

fn asset_roots(project: Option<&ProjectMetadata>, workspace_root: &Path) -> Vec<PathBuf> {
    if let Some(project) = project {
        let root = PathBuf::from(&project.root_path);
        return project
            .asset_roots
            .iter()
            .map(|path| root.join(path))
            .collect();
    }
    vec![workspace_root.join("assets")]
}

fn default_import_path(source: &Path, kind: AssetKind) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("asset");
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    parent
        .join(".meshi")
        .join("imported")
        .join(format!("{}.{}", stem, kind.imported_extension()))
}
