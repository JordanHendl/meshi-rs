use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Default)]
pub struct ImageEntry {
    pub name: String,
    pub path: String,
}

#[derive(Deserialize, Serialize, Clone, Default)]
pub struct Image {
    pub images: Vec<ImageEntry>,
}

#[derive(Deserialize, Serialize, Clone, Default)]
pub struct MaterialEntry {
    pub name: String,
    pub passes: Vec<String>,
    pub base_color: Option<String>,
    pub normal: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Materials {
    pub materials: Vec<MaterialEntry>,
}

#[derive(Deserialize, Serialize, Clone, Default)]
pub struct GeometryEntry {
    /// Name used to reference this mesh.
    pub name: String,
    /// Path to a glTF file relative to the database root. A `#mesh` or
    /// `#mesh/primitive` suffix may be appended to target a specific mesh and
    /// primitive within the file.
    pub path: String,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Geometry {
    pub geometry: Vec<GeometryEntry>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct TTFEntry {
    pub name: String,
    pub path: String,
    pub size: f64,
    pub glyphs: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct TTF {
    pub fonts: Vec<TTFEntry>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Database {
    pub images: Option<String>,
    pub materials: Option<String>,
    pub geometry: Option<String>,
    pub ttf: Option<String>,
}
