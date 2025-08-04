use crate::render::database::{self, Database, MeshResource};
use dashi::utils::Handle;
use glam::Mat4;
use tracing::info;

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
