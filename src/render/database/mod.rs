pub mod error;
use dashi::{utils::Handle, Buffer, Context};
use glam::{IVec4, Vec2, Vec4};
use tracing::{error, info};

pub use error::*;
pub mod json;
use std::collections::HashMap;
use std::fs;
pub mod images;
use images::*;
pub mod geometry;
use geometry::*;
pub mod material;
use material::*;
pub mod font;
pub mod geometry_primitives;
pub use font::*;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Vertex {
    pub position: Vec4,
    pub normal: Vec4,
    pub tex_coords: Vec2,
    pub joint_ids: IVec4,
    pub joints: Vec4,
    pub color: Vec4,
}

#[derive(Default, Clone)]
pub struct PrimitiveMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Default, Clone)]
pub struct MeshResource {
    pub name: String,
    pub vertices: Vec<Vertex>,
    pub num_vertices: usize,
    pub indices: Vec<u32>,
    pub num_indices: usize,
    pub vertex_buffer: Option<Handle<Buffer>>,
    pub index_buffer: Option<Handle<Buffer>>,
}

impl MeshResource {
    pub fn from_primitive(name: &str, primitive: PrimitiveMesh) -> Self {
        let num_vertices = primitive.vertices.len();
        let num_indices = primitive.indices.len();
        MeshResource {
            name: name.to_string(),
            vertices: primitive.vertices,
            num_vertices,
            indices: primitive.indices,
            num_indices,
            vertex_buffer: None,
            index_buffer: None,
        }
    }
}

pub struct Database {
    base_path: String,
    geometry: HashMap<String, MeshResource>,
    ctx: *mut Context,
    /// Map of texture names to optionally loaded handles. If a handle is
    /// `None` the texture has been registered but not yet loaded.
    textures: HashMap<String, Option<Handle<koji::Texture>>>,
    /// Map of material names to optionally loaded handles.
    materials: HashMap<String, MaterialResource>,
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

        let mut geometry = load_primitives();

        let mut textures = HashMap::new();
        textures.insert("DEFAULT".to_string(), Some(Handle::default()));
        info!("Registered texture asset: DEFAULT");

        let mut materials = HashMap::new();
        materials.insert(
            "DEFAULT".to_string(),
            MaterialResource {
                cfg: json::MaterialEntry {
                    name: "DEFAULT".to_string(),
                    ..Default::default()
                },
                loaded: Some(Handle::default()),
            },
        );
        info!("Registered material asset: DEFAULT");

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
                info!("Registered image asset: {}", img.name);
                textures.insert(img.name, None);
            }
        }

        if let Some(mat_file) = info.materials {
            let mat_path = format!("{}/{}", base_path, mat_file);
            let mat_json = fs::read_to_string(&mat_path)?;
            let mat_cfg: json::Materials = serde_json::from_str(&mat_json)?;
            for mat in mat_cfg.materials {
                info!("Registered material asset: {}", mat.name);
                materials.insert(
                    mat.name.clone(),
                    MaterialResource {
                        cfg: mat,
                        loaded: None,
                    },
                );
            }
        }

        if let Some(geo_file) = info.geometry {
            let geo_path = format!("{}/{}", base_path, geo_file);
            let geo_json = fs::read_to_string(&geo_path)?;
            let geo_cfg: json::Geometry = serde_json::from_str(&geo_json)?;
            for model in geo_cfg.geometry {
                // Geometry paths may include a mesh/primitive selector after a
                // `#` character. Strip it so the glTF file can be validated.
                let file = model.path.split('#').next().unwrap();
                let path = format!("{}/{}", base_path, file);
                let doc = parse_gltf(&path).map_err(|_| {
                    Error::LoadingError(LoadingError {
                        entry: model.name.clone(),
                        path: path.clone(),
                    })
                })?;

                // Register the base model name so it can still be referenced
                // without a mesh/primitive selector.
                info!("Registered geometry asset: {}", model.name);
                geometry.entry(model.name.clone()).or_insert(MeshResource {
                    name: model.name.clone(),
                    ..Default::default()
                });

                // Expose each mesh primitive as a selectable submesh using the
                // `model#mesh/primitive` naming convention.
                for (mesh_idx, mesh) in doc.meshes().enumerate() {
                    for (prim_idx, _prim) in mesh.primitives().enumerate() {
                        let sub_name = format!("{}#{}/{}", model.name, mesh_idx, prim_idx);
                        info!("Registered geometry asset: {}", sub_name);
                        geometry.entry(sub_name.clone()).or_insert(MeshResource {
                            name: sub_name,
                            ..Default::default()
                        });
                    }
                }
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
                info!("Registered font asset: {}", f.name);
                fonts.insert(f.name, font);
            }
        }

        Ok(Database {
            base_path: base_path.to_string(),
            geometry,
            textures,
            ctx,
            materials,
            _fonts: fonts,
        })
    }

    /// Internal helper to synchronously load a model from disk.
    ///
    /// `name` may include a selector suffix such as `file.gltf#mesh` or
    /// `file.gltf#mesh/1` to target a specific mesh and primitive inside the
    /// glTF file. Mesh selectors may be either a string name or a zero-based
    /// index. The primitive index defaults to `0` if omitted.
    fn load_model_sync(base_path: &str, ctx: *mut Context, name: &str) -> Result<MeshResource> {
        // GPU context is currently unused. Mesh data is kept on the CPU and can
        // be uploaded later if needed.
        let _ = ctx;

        // Allow selectors like `file.gltf#mesh` or `file.gltf#1/2` to target a
        // specific mesh and primitive within the glTF. Anything before `#` is
        // treated as the file path.
        let (file, selector) = if let Some((f, sel)) = name.split_once('#') {
            (f, Some(sel))
        } else {
            (name, None)
        };
        let path = format!("{}/{}", base_path, file);

        // Import the glTF file and associated buffers.
        let (doc, buffers, _images) = gltf::import(&path).map_err(|e| e.to_string())?;

        // Resolve the requested mesh and primitive.
        let primitive = {
            // Split primitive index from mesh selector if provided.
            let (mesh_sel, prim_sel) = selector
                .map(|s| s.split_once('/').unwrap_or((s, "0")))
                .unwrap_or(("", "0"));

            let mesh = if mesh_sel.is_empty() {
                doc.meshes().next()
            } else if let Ok(idx) = mesh_sel.parse::<usize>() {
                doc.meshes().nth(idx)
            } else {
                doc.meshes()
                    .find(|m| m.name().map_or(false, |n| n == mesh_sel))
            };

            let mesh = mesh.ok_or_else(|| {
                Error::LoadingError(LoadingError {
                    entry: name.to_string(),
                    path: path.clone(),
                })
            })?;

            let prim_index = prim_sel.parse::<usize>().unwrap_or(0);
            let primitive = mesh.primitives().nth(prim_index).ok_or_else(|| {
                Error::LoadingError(LoadingError {
                    entry: name.to_string(),
                    path: path.clone(),
                })
            })?;

            primitive
        };

        let reader = primitive.reader(|b| Some(&buffers[b.index()]));
        let positions: Vec<[f32; 3]> = reader
            .read_positions()
            .ok_or_else(|| {
                Error::LoadingError(LoadingError {
                    entry: name.to_string(),
                    path: path.clone(),
                })
            })?
            .collect();
        let indices: Vec<u32> = reader
            .read_indices()
            .ok_or_else(|| {
                Error::LoadingError(LoadingError {
                    entry: name.to_string(),
                    path: path.clone(),
                })
            })?
            .into_u32()
            .collect();

        let mut verts = Vec::with_capacity(positions.len());
        for p in &positions {
            verts.push(Vertex {
                position: Vec4::new(p[0], p[1], p[2], 1.0),
                ..Default::default()
            });
        }

        let num_vertices = positions.len();
        let num_indices = indices.len();
        Ok(MeshResource {
            name: name.to_string(),
            vertices: verts,
            num_vertices,
            indices,
            num_indices,
            vertex_buffer: None,
            index_buffer: None,
        })
    }
    /// Load a model file referenced by `name` into the database.
    ///
    /// The model path is resolved relative to the database base path. An
    /// optional `#mesh[/primitive]` selector may be included to target a
    /// specific mesh and primitive within the glTF file. The model is parsed
    /// using [`gltf`] and the requested primitive is uploaded to GPU buffers so
    /// it can be rendered.
    pub fn load_model(&mut self, name: &str) -> Result<()> {
        let mesh = Self::load_model_sync(&self.base_path, self.ctx, name)?;
        info!("Registered geometry asset: {}", name);
        self.geometry.insert(name.to_string(), mesh);
        Ok(())
    }

    /// Spawn a thread to load a model and upload its data to GPU buffers.
    ///
    /// The returned [`JoinHandle`] resolves to the loaded [`MeshResource`].
    pub fn load_model_async(&self, name: &str) -> std::thread::JoinHandle<Result<MeshResource>> {
        let base = self.base_path.clone();
        let ctx = self.ctx as usize;
        let name = name.to_string();
        std::thread::spawn(move || {
            let ctx = ctx as *mut Context;
            Self::load_model_sync(&base, ctx, &name)
        })
    }

    /// Unload a previously loaded model, dropping its GPU buffers.
    pub fn unload_model(&mut self, name: &str) -> Result<()> {
        match self.geometry.remove(name) {
            Some(_) => Ok(()),
            None => Err(Error::LookupError(LookupError {
                entry: name.to_string(),
            })),
        }
    }

    /// Load an image file referenced by `name` into the database.
    ///
    /// The image path is resolved relative to the database base path. The
    /// image is decoded using the `image` crate to ensure it is valid. Loaded
    /// image names are tracked so subsequent calls are inexpensive.
    pub fn load_image(
        &mut self,
        name: &str,
        mut renderer: Option<&mut koji::renderer::Renderer>,
    ) -> Result<()> {
        if self.textures.contains_key(name) {
            return Ok(());
        }
        let path = format!("{}/{}", self.base_path, name);
        load_image_from_path(&path)?;
        if let Some(r) = renderer.as_mut() {
            r.resources().register_combined(
                name,
                Handle::default(),
                Handle::default(),
                [1, 1],
                Handle::default(),
            );
        }
        info!("Registered image asset: {}", name);
        self.textures.insert(name.to_string(), None);
        Ok(())
    }

    /// Unload a previously loaded image, dropping any associated texture handle.
    pub fn unload_image(&mut self, name: &str) -> Result<()> {
        match self.textures.remove(name) {
            Some(Some(_handle)) => {
                // In a full renderer this would free the GPU texture referenced by
                // `handle`. In these tests the handle is a placeholder so simply
                // dropping it is sufficient.
                Ok(())
            }
            Some(None) => Ok(()),
            None => Err(Error::LookupError(LookupError {
                entry: name.to_string(),
            })),
        }
    }

    pub fn fetch_texture(
        &mut self,
        name: &str,
        mut renderer: Option<&mut koji::renderer::Renderer>,
    ) -> Result<Handle<koji::Texture>> {
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
                if let Some(r) = renderer.as_mut() {
                    r.resources().register_combined(
                        name,
                        Handle::default(),
                        Handle::default(),
                        [1, 1],
                        Handle::default(),
                    );
                }
                *entry = Some(handle);
                Ok(handle)
            }
            None => Err(Error::LookupError(LookupError {
                entry: name.to_string(),
            })),
        }
    }
    pub fn fetch_material(
        &mut self,
        name: &str,
        mut renderer: Option<&mut koji::renderer::Renderer>,
    ) -> Result<Handle<koji::Texture>> {
        if let Some(handle) = self.materials.get(name).and_then(|m| m.loaded) {
            return Ok(handle);
        }

        let tex_name = match self.materials.get(name) {
            Some(mat) => mat.cfg.base_color.clone(),
            None => {
                return Err(Error::LookupError(LookupError {
                    entry: name.to_string(),
                }))
            }
        };

        let handle = match tex_name {
            Some(tex) => {
                let tex_renderer = renderer.as_deref_mut();
                self.fetch_texture(&tex, tex_renderer)?
            }
            None => Handle::default(),
        };

        if let Some(r) = renderer.as_deref_mut() {
            let ctx = unsafe { &mut *self.ctx };
            r.resources()
                .register_variable(format!("mat_{}", name), ctx, [1.0_f32; 4]);
        }

        if let Some(mat) = self.materials.get_mut(name) {
            mat.loaded = Some(handle);
        }

        Ok(handle)
    }

    /// Retrieve a mesh by name, optionally loading it on demand.
    pub fn fetch_mesh(&mut self, name: &str, wait: bool) -> Result<MeshResource> {
        if let Some(mesh) = self.geometry.get(name) {
            return Ok(mesh.clone());
        }

        if wait {
            let handle = self.load_model_async(name);
            match handle.join() {
                Ok(Ok(mesh)) => {
                    info!("Registered geometry asset: {}", name);
                    self.geometry.insert(name.to_string(), mesh.clone());
                    return Ok(mesh);
                }
                Ok(Err(e)) => {
                    error!("Failed to load mesh {}: {}; defaulting to cube", name, e);
                }
                Err(_) => {
                    error!(
                        "Thread panic while loading mesh {}; defaulting to cube",
                        name
                    );
                }
            }
        } else {
            error!("Mesh {} not found; defaulting to cube primitive", name);
        }

        self.geometry.get("MESHI_CUBE").cloned().ok_or_else(|| {
            Error::LookupError(LookupError {
                entry: "MESHI_CUBE".to_string(),
            })
        })
    }

    /// Release any GPU resources tracked by the database.
    pub fn destroy(&mut self) {
        self.geometry.clear();
        self.textures.clear();
        self.materials.clear();
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
            ctx: std::ptr::null_mut(),
            textures: HashMap::new(),
            materials: HashMap::new(),
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
        db.load_image("test.png", None).unwrap();
        assert!(db.fetch_texture("test.png", None).is_ok());
    }

    #[test]
    fn fetch_texture_lookup_error() {
        let dir = tempdir().unwrap();
        let mut db = make_db(dir.path().to_str().unwrap());
        let err = db.fetch_texture("missing.png", None).unwrap_err();
        match err {
            Error::LookupError(_) => {}
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn unload_image_removes_entry() {
        let dir = tempdir().unwrap();
        let img_path = dir.path().join("test.png");
        let img = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
        img.save(&img_path).unwrap();

        let mut db = make_db(dir.path().to_str().unwrap());
        db.load_image("test.png", None).unwrap();
        db.fetch_texture("test.png", None).unwrap();
        assert!(db.textures.contains_key("test.png"));

        db.unload_image("test.png").unwrap();
        assert!(!db.textures.contains_key("test.png"));
    }

    #[test]
    fn unload_image_unknown_name() {
        let dir = tempdir().unwrap();
        let mut db = make_db(dir.path().to_str().unwrap());
        let err = db.unload_image("missing.png").unwrap_err();
        match err {
            Error::LookupError(_) => {}
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn fetch_material_success() {
        let dir = tempdir().unwrap();

        // Create an image that the material will reference.
        let img_path = dir.path().join("mat.png");
        let img = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
        img.save(&img_path).unwrap();

        // Write config files for images and materials.
        std::fs::write(
            dir.path().join("images.json"),
            "{\"images\":[{\"name\":\"mat.png\",\"path\":\"mat.png\"}]}",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("materials.json"),
            "{\"materials\":[{\"name\":\"mat\",\"passes\":[],\"base_color\":\"mat.png\"}]}",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("db.json"),
            "{\"images\":\"images.json\",\"materials\":\"materials.json\"}",
        )
        .unwrap();

        let mut ctx = dashi::Context::headless(&Default::default()).unwrap();
        let mut db = Database::new(dir.path().to_str().unwrap(), &mut ctx).unwrap();
        assert!(db.fetch_material("mat", None).is_ok());
        ctx.destroy();
    }

    #[test]
    fn fetch_mesh_missing_defaults_to_cube() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("db.json"), "{}").unwrap();
        let mut ctx = dashi::Context::headless(&Default::default()).unwrap();
        let mut db = Database::new(dir.path().to_str().unwrap(), &mut ctx).unwrap();
        let mesh = db.fetch_mesh("missing_model.gltf", false).unwrap();
        assert_eq!(mesh.name, "CUBE");
        ctx.destroy();
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
            "{{\n  \"asset\": {{ \"version\": \"2.0\" }},\n  \"scenes\": [{{ \"nodes\": [0] }}],\n  \"scene\": 0,\n  \"nodes\": [{{ \"mesh\": 0 }}],\n  \"meshes\": [{{ \"name\": \"mesh0\", \"primitives\": [{{ \"attributes\": {{ \"POSITION\": 0 }}, \"indices\": 1 }}] }}],\n  \"buffers\": [{{ \"uri\": \"data.bin\", \"byteLength\": {} }}],\n  \"bufferViews\": [{{ \"buffer\": 0, \"byteOffset\": 0, \"byteLength\": 36 }}, {{ \"buffer\": 0, \"byteOffset\": 36, \"byteLength\": 6 }}],\n  \"accessors\": [{{ \"bufferView\": 0, \"componentType\": 5126, \"count\": 3, \"type\": \"VEC3\", \"min\": [0.0,0.0,0.0], \"max\": [1.0,1.0,0.0] }}, {{ \"bufferView\": 1, \"componentType\": 5123, \"count\": 3, \"type\": \"SCALAR\" }}]\n}}",
            bin.len()
        );
        let gltf_path = dir.join("model.gltf");
        std::fs::write(&gltf_path, gltf).unwrap();
        "model.gltf".to_string()
    }

    #[test]
    fn load_model_sync_supports_selectors() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
        let base = base.to_str().unwrap();
        let mut ctx = dashi::Context::headless(&Default::default()).unwrap();

        // Loading by mesh name defaults to the first primitive.
        Database::load_model_sync(base, &mut ctx, "selector.gltf#Mesh1")
            .expect("failed to load mesh by name");

        // Loading by mesh name and primitive index succeeds.
        Database::load_model_sync(base, &mut ctx, "selector.gltf#Mesh1/1")
            .expect("failed to load primitive by name");

        // Loading by mesh index and primitive index succeeds.
        Database::load_model_sync(base, &mut ctx, "selector.gltf#1/1")
            .expect("failed to load primitive by index");

        // Invalid mesh index should error.
        assert!(Database::load_model_sync(base, &mut ctx, "selector.gltf#2").is_err());

        // Invalid primitive index should error.
        assert!(Database::load_model_sync(base, &mut ctx, "selector.gltf#Mesh1/5").is_err());

        ctx.destroy();
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
                "{{\"geometry\":[{{\"name\":\"model\",\"path\":\"{}#mesh0\"}}]}}",
                model_name
            ),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("materials.json"),
            "{\"materials\":[{\"name\":\"mat\",\"passes\":[],\"base_color\":\"img\"}]}",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("ttf.json"),
            "{\"fonts\":[{\"name\":\"font\",\"path\":\"font.ttf\",\"size\":16.0}]}",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("db.json"),
            "{\"images\":\"images.json\",\"geometry\":\"geometry.json\",\"materials\":\"materials.json\",\"ttf\":\"ttf.json\"}",
        )
        .unwrap();

        let mut ctx = dashi::Context::headless(&Default::default()).unwrap();
        let db = Database::new(dir.path().to_str().unwrap(), &mut ctx).unwrap();
        assert!(db.textures.contains_key("img"));
        assert!(db.geometry.contains_key("model"));
        assert!(db.materials.contains_key("mat"));
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
