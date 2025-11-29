use glam::{Mat4, Vec4};

#[derive(Default)]
pub struct MeshObject {
    //pub mesh: MeshResource,
    pub transform: Mat4,
    pub renderer_handle: Option<usize>,
}

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

#[derive(Default, Clone, Copy)]
#[repr(C)]
pub struct DirectionalLightInfo {
    pub direction: Vec4,
    pub color: Vec4,
    pub intensity: f32,
}

#[derive(Default)]
pub struct DirectionalLight {
    pub transform: Mat4,
    pub info: DirectionalLightInfo,
}

pub struct RgbaImage {

}

#[derive(Default)]
pub struct RenderEngineInfo {
    pub database_path: String,
    pub headless: bool,
    pub canvas_extent: Option<[u32; 2]>,
}

