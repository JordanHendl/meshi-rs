pub mod event;
use glam::*;
use std::ffi::{c_char, c_void};

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct RenderObjectInfo {
    pub mesh: *const c_char,
    pub material: *const c_char,
    pub transform: glam::Mat4,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct WindowInfo {
    pub title: *const c_char,
    pub width: u32,
    pub height: u32,
    pub resizable: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DisplayInfo {
    pub vsync: i32,
    pub window: WindowInfo,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SkyboxSettingsInfo {
    pub intensity: f32,
    pub use_procedural_cubemap: i32,
    pub update_interval_frames: u32,
}

impl Default for SkyboxSettingsInfo {
    fn default() -> Self {
        Self {
            intensity: 1.0,
            use_procedural_cubemap: 1,
            update_interval_frames: 1,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SkySettingsInfo {
    pub enabled: i32,
    pub has_sun_direction: i32,
    pub sun_direction: Vec3,
}

impl Default for SkySettingsInfo {
    fn default() -> Self {
        Self {
            enabled: 1,
            has_sun_direction: 0,
            sun_direction: Vec3::new(0.0, -1.0, 0.0),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EnvironmentLightingInfo {
    pub sky: SkySettingsInfo,
    pub sun_light_intensity: f32,
    pub moon_light_intensity: f32,
}

impl Default for EnvironmentLightingInfo {
    fn default() -> Self {
        Self {
            sky: SkySettingsInfo::default(),
            sun_light_intensity: 1.0,
            moon_light_intensity: 0.1,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OceanSettingsInfo {
    pub enabled: i32,
    pub wind_speed: f32,
    pub wave_amplitude: f32,
    pub gerstner_amplitude: f32,
}

impl Default for OceanSettingsInfo {
    fn default() -> Self {
        Self {
            enabled: 1,
            wind_speed: 2.0,
            wave_amplitude: 4.0,
            gerstner_amplitude: 0.35,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CloudSettingsInfo {
    pub enabled: i32,
}

impl Default for CloudSettingsInfo {
    fn default() -> Self {
        Self { enabled: 1 }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TerrainSettingsInfo {
    pub enabled: i32,
    pub clipmap_resolution: u32,
    pub max_tiles: u32,
    pub lod_levels: u32,
}

impl Default for TerrainSettingsInfo {
    fn default() -> Self {
        Self {
            enabled: 1,
            clipmap_resolution: 18,
            max_tiles: 12 * 12,
            lod_levels: 6,
        }
    }
}

#[deprecated(note = "Use RenderObjectInfo instead.")]
pub type MeshObjectInfo = RenderObjectInfo;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightType {
    Directional = 0,
    Point = 1,
    Spot = 2,
    RectArea = 3,
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

#[repr(C)]
pub struct FFIImage {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub pixels: *const u8,
}

pub struct EventCallbackInfo {
    pub event_cb: extern "C" fn(*mut event::Event, *mut c_void),
    pub user_data: *mut c_void,
}
