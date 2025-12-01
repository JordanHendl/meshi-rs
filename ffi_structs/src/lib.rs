use glam::{Mat4, Vec4};

use std::ffi::{c_char, CStr};
use std::fmt;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct MeshObjectInfo {
    pub mesh: *const c_char,
    pub material: *const c_char,
    pub transform: glam::Mat4,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct DirectionalLightInfo {
    pub direction: Vec4,
    pub color: Vec4,
    pub intensity: f32,
}

