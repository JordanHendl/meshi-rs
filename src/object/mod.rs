use crate::render::database::{self, Database, MeshResource};
use dashi::utils::Handle;
use glam::Mat4;
use tracing::info;

use std::ffi::{c_char, CStr};

#[repr(C)]
pub struct FFIMeshObjectInfo {
    pub mesh: *const c_char,
    pub material: *const c_char,
    pub transform: glam::Mat4,
}

pub struct MeshObjectInfo {
    pub mesh: &'static str,
    pub material: &'static str,
    pub transform: glam::Mat4,
}

impl From<&FFIMeshObjectInfo> for MeshObjectInfo {
    fn from(value: &FFIMeshObjectInfo) -> Self {
        unsafe {
            let mesh = CStr::from_ptr(value.mesh).to_str().unwrap_or_default();
            let mut material = CStr::from_ptr(value.material).to_str().unwrap_or_default();

            if material.is_empty() {
                material = mesh;
            }
            Self {
                mesh,
                material,
                transform: value.transform,
            }
        }
    }
}

impl MeshObjectInfo {
    pub fn make_object(&self, db: &mut Database) -> Result<MeshObject, database::Error> {
        info!(
            "Registering Mesh Renderable {}||{}",
            self.mesh, self.material
        );

        let mesh = db.fetch_mesh(self.mesh)?;
        let material = match db.fetch_material(self.material) {
            Ok(mat) => mat,
            Err(_) => db.fetch_material("DEFAULT")?,
        };

        let targets = vec![MeshTarget {
            mesh: mesh.clone(),
            material,
        }];

        Ok(MeshObject {
            targets,
            mesh,
            transform: self.transform,
        })
    }
}

#[derive(Default, Clone)]
pub struct MeshTarget {
    pub mesh: MeshResource,
    pub material: Handle<koji::Texture>,
}

#[derive(Default)]
pub struct MeshObject {
    pub targets: Vec<MeshTarget>,
    pub mesh: MeshResource,
    pub transform: Mat4,
}
