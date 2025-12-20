use glam::{Mat4, Vec4};
use glam::*;
use furikake::types::*;
use meshi_ffi_structs::*;
use noren::{DB, meta::DeviceModel};

#[derive(Default)]
pub struct MeshObject {
    //pub mesh: MeshResource,
    pub transform: Mat4,
    pub renderer_handle: Option<usize>,
}

pub enum RenderObjectInfo {
    Model(DeviceModel),
}
pub struct RenderObject;

#[repr(C)]
pub struct CubePrimitiveInfo {
    pub size: f32,
}

impl Default for CubePrimitiveInfo {
    fn default() -> Self {
        Self { size: 1.0 }
    }
}

#[repr(C)]
pub struct SpherePrimitiveInfo {
    pub radius: f32,
    pub segments: u32,
    pub rings: u32,
}

impl Default for SpherePrimitiveInfo {
    fn default() -> Self {
        Self {
            radius: 1.0,
            segments: 32,
            rings: 16,
        }
    }
}

#[repr(C)]
pub struct CylinderPrimitiveInfo {
    pub radius: f32,
    pub height: f32,
    pub segments: u32,
}

impl Default for CylinderPrimitiveInfo {
    fn default() -> Self {
        Self {
            radius: 1.0,
            height: 1.0,
            segments: 32,
        }
    }
}

#[repr(C)]
pub struct ConePrimitiveInfo {
    pub radius: f32,
    pub height: f32,
    pub segments: u32,
}

impl Default for ConePrimitiveInfo {
    fn default() -> Self {
        Self {
            radius: 1.0,
            height: 1.0,
            segments: 32,
        }
    }
}

#[repr(C)]
pub struct PlanePrimitiveInfo {
    pub size: f32,
}

impl Default for PlanePrimitiveInfo {
    fn default() -> Self {
        Self { size: 1.0 }
    }
}


#[inline]
pub fn pack_gpu_light(s: LightInfo) -> Light {
    let mut out = Light {
        position_type: Vec4::ZERO,
        direction_range: Vec4::ZERO,
        color_intensity: Vec4::ZERO,
        spot_area: Vec4::ZERO,
        extra: Vec4::ZERO,
    };

    // position_type
    out.position_type.x = s.pos_x;
    out.position_type.y = s.pos_y;
    out.position_type.z = s.pos_z;
    out.position_type.w = s.ty as u32 as f32;

    // direction_range
    out.direction_range.x = s.dir_x;
    out.direction_range.y = s.dir_y;
    out.direction_range.z = s.dir_z;
    out.direction_range.w = s.range;

    // color_intensity
    out.color_intensity.x = s.color_r;
    out.color_intensity.y = s.color_g;
    out.color_intensity.z = s.color_b;
    out.color_intensity.w = s.intensity;

    // spot / area params
    out.spot_area.x = s.spot_inner_angle_rad.cos();
    out.spot_area.y = s.spot_outer_angle_rad.cos();
    out.spot_area.z = s.rect_half_width;
    out.spot_area.w = s.rect_half_height;

    // flags (bitwise packed into f32)
    out.extra.x = f32::from_bits(s.flags);

    // Enforce your documented semantics
    match s.ty {
        LightType::Directional => {
            out.position_type  = Vec3::ZERO.extend(out.position_type.w);
            out.direction_range.w = 0.0; // infinite
            out.spot_area = Vec4::ZERO;
        }
        LightType::Point => {
            out.direction_range = Vec3::ZERO.extend(out.direction_range.w);
            out.spot_area = Vec4::ZERO;
        }
        LightType::Spot => {
            out.spot_area.z = 0.0;
            out.spot_area.w = 0.0;
        }
        LightType::RectArea => {
            out.spot_area.x = 0.0;
            out.spot_area.y = 0.0;
        }
    }

    out
}

#[derive(Default)]
pub struct RenderEngineInfo {
    pub headless: bool,
    pub canvas_extent: Option<[u32; 2]>,
}
