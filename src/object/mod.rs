use crate::render::database::{self, Database, MeshResource};
use dashi::utils::Handle;
use glam::Mat4;
use tracing::{info, warn};

use std::ffi::{c_char, CStr};
use std::fmt;

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

#[derive(Debug)]
pub enum Error {
    InvalidString,
    Database(database::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidString => write!(f, "invalid string pointer"),
            Error::Database(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Database(err) => Some(err),
            _ => None,
        }
    }
}

impl From<database::Error> for Error {
    fn from(value: database::Error) -> Self {
        Error::Database(value)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(_: std::str::Utf8Error) -> Self {
        Error::InvalidString
    }
}

impl TryFrom<&FFIMeshObjectInfo> for MeshObjectInfo {
    type Error = Error;

    fn try_from(value: &FFIMeshObjectInfo) -> Result<Self, Self::Error> {
        unsafe {
            if value.mesh.is_null() || value.material.is_null() {
                return Err(Error::InvalidString);
            }

            let mesh = CStr::from_ptr(value.mesh).to_str()?;
            let mut material = CStr::from_ptr(value.material).to_str()?;

            if material.is_empty() {
                material = mesh;
            }

            Ok(Self {
                mesh,
                material,
                transform: value.transform,
            })
        }
    }
}

impl MeshObjectInfo {
    pub fn make_object(&self, db: &mut Database) -> Result<MeshObject, Error> {
        info!(
            "Registering Mesh Renderable {} with material {}",
            self.mesh, self.material
        );

        let mesh = db.fetch_mesh(self.mesh, true)?;
        let material = match db.fetch_material(self.material) {
            Ok(mat) => mat,
            Err(e) => {
                warn!(
                    "Failed to fetch material '{}': {}; falling back to default",
                    self.material, e
                );
                db.fetch_material("DEFAULT")?
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::database::Database;
    use dashi::{utils::Handle, Context};
    use tempfile::tempdir;

    #[test]
    fn missing_material_falls_back_to_default() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("db.json"), "{}").unwrap();
        let mut ctx = Context::headless(&Default::default()).unwrap();
        let mut db = Database::new(dir.path().to_str().unwrap(), &mut ctx).unwrap();

        let info = MeshObjectInfo {
            mesh: "MESHI_CUBE",
            material: "NON_EXISTENT",
            transform: Mat4::IDENTITY,
        };

        let obj = info.make_object(&mut db).unwrap();
        assert_eq!(obj.targets.len(), 1);
        assert_eq!(obj.targets[0].material, Handle::default());
        ctx.destroy();
    }
}
