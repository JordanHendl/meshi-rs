use glam::Mat4;

use std::ffi::{c_char, CStr};
use std::fmt;

#[repr(C)]
pub struct FFIMeshObjectInfo {
    pub mesh: *const c_char,
    pub material: *const c_char,
    pub transform: glam::Mat4,
}
