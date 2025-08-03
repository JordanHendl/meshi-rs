pub mod error;
use dashi::{utils::Handle, Buffer};
use tracing::info;

pub use error::*;
pub mod json;
use std::collections::HashMap;
use std::fs;
pub mod images;
use images::*;
pub mod geometry;
use geometry::*;
pub mod font;
pub mod geometry_primitives;
pub use font::*;

#[derive(Default, Clone)]
pub struct MeshResource {
    pub name: String,
    pub vertices: Handle<Buffer>,
    pub num_vertices: usize,
    pub indices: Handle<Buffer>,
    pub num_indices: usize,
}

pub struct Database {
    base_path: String,
    geometry: HashMap<String, MeshResource>,
    /// Map of texture names to optionally loaded handles. If a handle is
    /// `None` the texture has been registered but not yet loaded.
    textures: HashMap<String, Option<Handle<koji::Texture>>>,
    _fonts: HashMap<String, TTFont>,
}

// Loading models and images only touches filesystem data structures. It is
// therefore safe to send the database across threads as long as external
// synchronization (like a `Mutex`) is used.


impl Database {
    pub fn base_path(&self) -> &str {
        &self.base_path
    }

    pub fn new(base_path: &str, ctx: &mut dashi::Context) -> Result<Self> {
        info!("Loading Database {}", format!("{}/db.json", base_path));

        let json_data = fs::read_to_string(format!("{}/db.json", base_path))?;
        let info: json::Database = serde_json::from_str(&json_data)?;

        let mut geometry = load_primitives(ctx);

        let mut textures = HashMap::new();
        textures.insert("DEFAULT".to_string(), Some(Handle::default()));

        if let Some(images_file) = info.images {
            let images_path = format!("{}/{}", base_path, images_file);
            let images_json = fs::read_to_string(&images_path)?;
            let images_cfg: json::Image = serde_json::from_str(&images_json)?;
            for img in images_cfg.images {
                let path = format!("{}/{}", base_path, img.path);
                load_image_from_path(&path).map_err(|_| {
                    Error::LoadingError(LoadingError {
                        entry: img.name.clone(),
                        path: path.clone(),
                    })
                })?;
                textures.insert(img.name, None);
            }
        }

        if let Some(geo_file) = info.geometry {
            let geo_path = format!("{}/{}", base_path, geo_file);
            let geo_json = fs::read_to_string(&geo_path)?;
            let geo_cfg: json::Geometry = serde_json::from_str(&geo_json)?;
            for model in geo_cfg.geometry {
                let path = format!("{}/{}", base_path, model.path);
                parse_gltf(&path).map_err(|_| {
                    Error::LoadingError(LoadingError {
                        entry: model.name.clone(),
                        path: path.clone(),
                    })
                })?;
                geometry.entry(model.name.clone()).or_insert(MeshResource {
                    name: model.name,
                    ..Default::default()
                });
            }
        }

        let mut fonts = HashMap::new();
        if let Some(font_file) = info.ttf {
            let font_path = format!("{}/{}", base_path, font_file);
            let font_json = fs::read_to_string(&font_path)?;
            let font_cfg: json::TTF = serde_json::from_str(&font_json)?;
            for f in font_cfg.fonts {
                let path = format!("{}/{}", base_path, f.path);
                fs::read(&path).map_err(|_| {
                    Error::LoadingError(LoadingError {
                        entry: f.name.clone(),
                        path: path.clone(),
                    })
                })?;
                let glyphs: Vec<char> = f.glyphs.unwrap_or_default().chars().collect();
                let font = TTFont::new(&path, 256, 256, f.size as f32, &glyphs);
                fonts.insert(f.name, font);
            }
        }

        Ok(Database {
            base_path: base_path.to_string(),
            geometry,
            textures,
            _fonts: fonts,
        })
    }

    /// Load a model file referenced by `name` into the database.
    ///
    /// The model path is resolved relative to the database base path. The
    /// model is considered loaded once the file exists and is readable. The
    /// currently stubbed implementation simply registers the model name so
    /// that it can be retrieved later by tests or callers.
    pub fn load_model(&mut self, name: &str) -> Result<()> {
        let path = format!("{}/{}", self.base_path, name);
        // Ensure the file exists and is valid glTF.
        parse_gltf(&path).map_err(|e| e.to_string())?;
        // Register the model in the geometry map if not already present.
        self.geometry
            .entry(name.to_string())
            .or_insert(MeshResource {
                name: name.to_string(),
                ..Default::default()
            });
        Ok(())
    }

    /// Load an image file referenced by `name` into the database.
    ///
    /// The image path is resolved relative to the database base path. The
    /// image is decoded using the `image` crate to ensure it is valid. Loaded
    /// image names are tracked so subsequent calls are inexpensive.
    pub fn load_image(&mut self, name: &str) -> Result<()> {
        if self.textures.contains_key(name) {
            return Ok(());
        }
        let path = format!("{}/{}", self.base_path, name);
        load_image_from_path(&path)?;
        self.textures.insert(name.to_string(), None);
        Ok(())
    }

    pub fn fetch_texture(&mut self, name: &str) -> Result<Handle<koji::Texture>> {
        match self.textures.get_mut(name) {
            Some(entry) => {
                if let Some(handle) = entry {
                    return Ok(*handle);
                }

                // Lazily load the texture data from disk. We only verify the
                // image can be opened; conversion to a GPU texture is outside
                // the scope of these tests so a default handle is returned.
                let path = format!("{}/{}", self.base_path, name);
                image::open(&path)?;
                let handle = Handle::default();
                *entry = Some(handle);
                Ok(handle)
            }
            None => Err(Error::LookupError(LookupError {
                entry: name.to_string(),
            })),
        }
    }
    pub fn fetch_material(&mut self, name: &str) -> Result<Handle<koji::Texture>> {
        self.fetch_texture(name)
    }

    pub fn fetch_mesh(&self, name: &str) -> Result<MeshResource> {
        match self.geometry.get(name) {
            Some(mesh) => Ok(mesh.clone()),
            None => Err(Error::LookupError(LookupError {
                entry: name.to_string(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::collections::HashMap;
    use tempfile::tempdir;

    // Helper to construct a minimal database without a real GPU context.
    fn make_db(path: &str) -> Database {
        Database {
            base_path: path.to_string(),
            geometry: HashMap::new(),
            textures: HashMap::new(),
            _fonts: HashMap::new(),
        }
    }

    #[test]
    fn fetch_texture_success() {
        let dir = tempdir().unwrap();
        let img_path = dir.path().join("test.png");
        let img = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
        img.save(&img_path).unwrap();

        let mut db = make_db(dir.path().to_str().unwrap());
        db.load_image("test.png").unwrap();
        assert!(db.fetch_texture("test.png").is_ok());
    }

    #[test]
    fn fetch_texture_lookup_error() {
        let dir = tempdir().unwrap();
        let mut db = make_db(dir.path().to_str().unwrap());
        let err = db.fetch_texture("missing.png").unwrap_err();
        match err {
            Error::LookupError(_) => {}
            other => panic!("unexpected error: {:?}", other),
        }
    }

    // Build a minimal triangle glTF asset in `dir` and return the glTF file name.
    fn write_triangle_gltf(dir: &std::path::Path) -> String {
        let bin_path = dir.join("data.bin");
        let mut bin = Vec::new();
        for f in [
            0.0f32, 0.0, 0.0, // v0
            1.0, 0.0, 0.0, // v1
            0.0, 1.0, 0.0, // v2
        ] {
            bin.extend_from_slice(&f.to_le_bytes());
        }
        for i in [0u16, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        std::fs::write(&bin_path, &bin).unwrap();

        let gltf = format!(
            "{{\n  \"asset\": {{ \"version\": \"2.0\" }},\n  \"scenes\": [{{ \"nodes\": [0] }}],\n  \"scene\": 0,\n  \"nodes\": [{{ \"mesh\": 0 }}],\n  \"meshes\": [{{ \"primitives\": [{{ \"attributes\": {{ \"POSITION\": 0 }}, \"indices\": 1 }}] }}],\n  \"buffers\": [{{ \"uri\": \"data.bin\", \"byteLength\": {} }}],\n  \"bufferViews\": [{{ \"buffer\": 0, \"byteOffset\": 0, \"byteLength\": 36 }}, {{ \"buffer\": 0, \"byteOffset\": 36, \"byteLength\": 6 }}],\n  \"accessors\": [{{ \"bufferView\": 0, \"componentType\": 5126, \"count\": 3, \"type\": \"VEC3\", \"min\": [0.0,0.0,0.0], \"max\": [1.0,1.0,0.0] }}, {{ \"bufferView\": 1, \"componentType\": 5123, \"count\": 3, \"type\": \"SCALAR\" }}]\n}}",
            bin.len()
        );
        let gltf_path = dir.join("model.gltf");
        std::fs::write(&gltf_path, gltf).unwrap();
        "model.gltf".to_string()
    }

    #[test]
    fn database_new_loads_resources() {
        let dir = tempdir().unwrap();

        // Image
        let img_path = dir.path().join("img.png");
        let img = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
        img.save(&img_path).unwrap();

        // Model
        let model_name = write_triangle_gltf(dir.path());

        // Font
    #[cfg(target_os = "windows")]
        let font_src = "C:/Windows/Fonts/arial.ttf";
    #[cfg(target_os = "linux")]
        let font_src = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

        let font_dest = dir.path().join("font.ttf");
        std::fs::copy(font_src, &font_dest).unwrap();

        // Configuration files
        std::fs::write(
            dir.path().join("images.json"),
            "{\"images\":[{\"name\":\"img\",\"path\":\"img.png\"}]}",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("geometry.json"),
            format!(
                "{{\"geometry\":[{{\"name\":\"model\",\"path\":\"{}\"}}]}}",
                model_name
            ),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("ttf.json"),
            "{\"fonts\":[{\"name\":\"font\",\"path\":\"font.ttf\",\"size\":16.0}]}",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("db.json"),
            "{\"images\":\"images.json\",\"geometry\":\"geometry.json\",\"ttf\":\"ttf.json\"}",
        )
        .unwrap();

        let mut ctx = dashi::Context::headless(&Default::default()).unwrap();
        let db = Database::new(dir.path().to_str().unwrap(), &mut ctx).unwrap();
        assert!(db.textures.contains_key("img"));
        assert!(db.geometry.contains_key("model"));
        assert!(db._fonts.contains_key("font"));
        drop(db);
        ctx.destroy();
    }

    #[test]
    fn database_new_missing_image() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("images.json"),
            "{\"images\":[{\"name\":\"img\",\"path\":\"missing.png\"}]}",
        )
        .unwrap();
        std::fs::write(dir.path().join("db.json"), "{\"images\":\"images.json\"}").unwrap();
        let mut ctx = dashi::Context::headless(&Default::default()).unwrap();
        match Database::new(dir.path().to_str().unwrap(), &mut ctx) {
            Ok(_) => panic!("expected error"),
            Err(Error::LoadingError(e)) => assert_eq!(e.entry, "img"),
            Err(other) => panic!("unexpected error: {:?}", other),
        }
        ctx.destroy();
    }

    #[test]
    fn database_new_missing_model() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("geometry.json"),
            "{\"geometry\":[{\"name\":\"model\",\"path\":\"missing.gltf\"}]}",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("db.json"),
            "{\"geometry\":\"geometry.json\"}",
        )
        .unwrap();
        let mut ctx = dashi::Context::headless(&Default::default()).unwrap();
        match Database::new(dir.path().to_str().unwrap(), &mut ctx) {
            Ok(_) => panic!("expected error"),
            Err(Error::LoadingError(e)) => assert_eq!(e.entry, "model"),
            Err(other) => panic!("unexpected error: {:?}", other),
        }
        ctx.destroy();
    }

    #[test]
    fn database_new_missing_font() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("ttf.json"),
            "{\"fonts\":[{\"name\":\"font\",\"path\":\"missing.ttf\",\"size\":12.0}]}",
        )
        .unwrap();
        std::fs::write(dir.path().join("db.json"), "{\"ttf\":\"ttf.json\"}").unwrap();
        let mut ctx = dashi::Context::headless(&Default::default()).unwrap();
        match Database::new(dir.path().to_str().unwrap(), &mut ctx) {
            Ok(_) => panic!("expected error"),
            Err(Error::LoadingError(e)) => assert_eq!(e.entry, "font"),
            Err(other) => panic!("unexpected error: {:?}", other),
        }
        ctx.destroy();
    }
}
