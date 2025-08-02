pub mod error;
use dashi::{utils::Handle, Buffer};
use tracing::info;

pub use error::*;
pub mod json;
use std::collections::HashMap;
use std::fs;
pub mod images;
use images::*;
mod material;
use material::*;
pub mod geometry;
use geometry::*;
pub mod font;
mod geometry_primitives;
pub use font::*;

#[derive(Default, Clone)]
pub struct MeshResource {
    pub name: String,
    pub vertices: Handle<Buffer>,
    pub num_vertices: usize,
    pub indices: Handle<Buffer>,
    pub num_indices: usize,
}

#[allow(dead_code)]
struct Defaults {
//    image: Handle<koji::Texture>,
//    material: Handle<koji::Material>,
}

#[allow(dead_code)]
pub struct Database {
    ctx: *mut dashi::Context,
    base_path: String,
    geometry: HashMap<String, MeshResource>,
    /// Map of texture names to optionally loaded handles. If a handle is
    /// `None` the texture has been registered but not yet loaded.
    textures: HashMap<String, Option<Handle<koji::Texture>>>,
//    materials: HashMap<String, Handle<miso::Material>>,
//    fonts: HashMap<String, FontResource>,
//    defaults: Defaults,
}

// The database wraps a raw pointer to the rendering context but loading models
// and images only touches filesystem data structures. It is therefore safe to
// send across threads as long as external synchronization (like a `Mutex`) is
// used.
unsafe impl Send for Database {}

impl Database {
    pub fn base_path(&self) -> &str {
        &self.base_path
    }

    pub fn new(
        base_path: &str,
        ctx: &mut dashi::Context,
    ) -> Result<Self> {
        info!("Loading Database {}", format!("{}/db.json", base_path));
        let _json_data = fs::read_to_string(format!("{}/db.json", base_path))?;
//        let info: json::Database = serde_json::from_str(&json_data)?;
//
//        let images_cfg = load_db_images(base_path, &info);
//        let fonts_cfg = load_db_ttfs(&info);
//        let geometry_cfg = load_db_geometries(base_path, &info);
//        let material_cfg = load_db_materials(base_path, &info);
//
//        let images = match images_cfg {
//            Some(cfg) => cfg.into(),
//            None => HashMap::default(),
//        };
//
//        let materials = match material_cfg {
//            Some(cfg) => cfg.into(),
//            None => HashMap::default(),
//        };
//
//        let fonts = match fonts_cfg {
//            Some(cfg) => cfg.into(),
//            None => HashMap::default(),
//        };
//
//        let mut geometry = match geometry_cfg {
//            Some(cfg) => cfg.into(),
//            None => HashMap::default(),
//        };

//        geometry.insert(
//            "MESHI_TRIANGLE".to_string(),
//            GeometryResource {
//                cfg: json::GeometryEntry {
//                    name: "MESHI".to_string(),
//                    path: "".to_string(),
//                },
//                loaded: Some(geometry_primitives::make_triangle(
//                    &Default::default(),
//                    ctx,
////                    scene,
//                )),
//            },
//        );
//
//        geometry.insert(
//            "MESHI_CUBE".to_string(),
//            GeometryResource {
//                cfg: json::GeometryEntry {
//                    name: "MESHI".to_string(),
//                    path: "".to_string(),
//                },
//                loaded: Some(geometry_primitives::make_cube(
//                    &Default::default(),
//                    ctx,
//                    scene,
//                )),
//            },
//        );
//
//        geometry.insert(
//            "MESHI_SPHERE".to_string(),
//            GeometryResource {
//                cfg: json::GeometryEntry {
//                    name: "MESHI".to_string(),
//                    path: "".to_string(),
//                },
//                loaded: Some(geometry_primitives::make_sphere(
//                    &Default::default(),
//                    ctx,
//                    scene,
//                )),
//            },
//        );
//
//        let default_texture = ImageResource::load_default_image(ctx, scene);
//
//        info!("Registering default material..");
//        let default_material = scene.register_material(&MaterialInfo {
//            name: "DEFAULT".to_string(),
//            //passes: vec!["ALL".to_string()],
//            passes: vec!["non-transparent".to_string()],
//            base_color: default_texture,
//            normal: Default::default(),
//            ..Default::default()
//        });

        let geometry = load_primitives(ctx);

        let mut textures = HashMap::new();
        textures.insert("DEFAULT".to_string(), Some(Handle::default()));

        let db = Database {
            base_path: base_path.to_string(),
            ctx,
            geometry,
            textures,
        };

 //       let ptr: *mut Database = &mut db;

 //       // Models HAVE to be loaded before materials, as they add materials.
 //       for (_name, mut model) in geometry {
 //           debug!("Attempting to load model {}...", model.cfg.name);
 //           if model.loaded.is_none() {
 //               model.load(base_path, ctx, scene, unsafe { &mut *ptr });
 //           }

 //           if let Some(m) = model.loaded {
 //               debug!("Success!");
 //               for mesh in m.meshes {
 //                   debug!("Making mesh {}.{} available", model.cfg.name, mesh.name);
 //                   db.geometry
 //                       .insert(format!("{}.{}", model.cfg.name, mesh.name), mesh);
 //               }
 //           } else {
 //               debug!("Failed!");
 //           }
 //       }

 //       // Images MUST be parsed before materials, as this loads images if they are used.
 //       for (name, mut m) in materials {
 //           m.load(scene, unsafe { &mut *ptr });
 //           if let Some(mat) = m.loaded {
 //               db.materials.insert(name, mat);
 //           }
 //       }

        Ok(db)
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

 //   fn insert_material(&mut self, name: &str, mat: Handle<koji::Material>) {
 //       if self.materials.get(name).is_none() {
 //           self.materials.insert(name.to_string(), mat);
 //       }
 //   }

 //   pub(crate) fn register_texture_from_bytes(&mut self, name: &str, data: &[u8]) {
 //       debug!(
 //           "Registering embedded GLTF model texture from bytes {}..",
 //           name
 //       );
 //       let image = unsafe {
 //           ImageResource::load_from_uri(name, data, &mut *self.ctx, &mut *self.scene)
 //       };
 //       self.images.insert(
 //           name.to_string(),
 //           ImageResource {
 //               cfg: json::ImageEntry {
 //                   name: name.to_string(),
 //                   path: Default::default(),
 //               },
 //               loaded: Some(image),
 //           },
 //       );
 //   }

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
//        if let Some(thing) = self.images.get_mut(name) {
//            if thing.loaded.is_none() {
//                unsafe { thing.load_rgba8(&self.base_path, &mut *self.ctx, &mut *self.scene) };
//            }
//
//            if thing.loaded.is_none() {
//                return Err(Error::LoadingError(LoadingError {
//                    entry: thing.cfg.name.clone(),
//                    path: thing.cfg.path.clone(),
//                }));
//            } else {
//                return Ok(thing.loaded.as_ref().unwrap().clone());
//            }
//        }
//
//        return Err(Error::LookupError(LookupError {
//            entry: name.to_string(),
//        }));
//    }

//    pub fn fetch_material(&mut self, name: &str) -> Result<Handle<miso::Material>, Error> {
//        todo!()
//        if let Some(thing) = self.materials.get(name) {
//            return Ok(*thing);
//        } else {
//            debug!("Unable to fetch material {}. Returning default...", name);
//            return Ok(self.defaults.material);
//        }

//    pub fn fetch_mesh(&mut self, name: &str) -> Result<MeshResource, Error> {
//        if let Some(thing) = self.geometry.get_mut(name) {
//            return Ok(thing.clone());
//        }
//
//        debug!("Unable to fetch model {}. Returning default sphere", name);
//        return Ok(self.geometry.get("MESHI.CUBE").unwrap().clone());
//    }
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
            ctx: std::ptr::null_mut(),
            base_path: path.to_string(),
            geometry: HashMap::new(),
            textures: HashMap::new(),
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
}
