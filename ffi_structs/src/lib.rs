
use glam::*;
use std::ffi::{c_char, CStr};
use std::fmt;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct MeshObjectInfo {
    pub mesh: *const c_char,
    pub material: *const c_char,
    pub transform: glam::Mat4,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightType {
    Directional = 0,
    Point       = 1,
    Spot        = 2,
    RectArea    = 3,
}

bitflags::bitflags! {
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct LightFlags: u32 {
        const NONE          = 0;
        const CASTS_SHADOWS = 1 << 0;
        const VOLUMETRIC    = 1 << 1;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LightInfo {
    pub ty: LightType,
    pub flags: u32,

    pub intensity: f32,
    pub range: f32,

    pub color_r: f32,
    pub color_g: f32,
    pub color_b: f32,

    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,

    pub dir_x: f32,
    pub dir_y: f32,
    pub dir_z: f32,

    pub spot_inner_angle_rad: f32,
    pub spot_outer_angle_rad: f32,

    pub rect_half_width: f32,
    pub rect_half_height: f32,
}

