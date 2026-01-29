use dashi::SampleCount;
use furikake::types::Material;
use furikake::types::*;
use glam::*;
use glam::{Mat4, Vec2, Vec4};
use meshi_ffi_structs::{LightInfo, LightType};
use noren::meta::DeviceModel;
use resource_pool::Handle;

#[derive(Default)]
pub struct RenderObjectInstance {
    //pub mesh: MeshResource,
    pub transform: Mat4,
    pub renderer_handle: Option<usize>,
}

#[deprecated(note = "Use RenderObjectInstance instead.")]
pub type MeshObject = RenderObjectInstance;

pub enum RenderObjectInfo {
    Empty,
    Model(DeviceModel),
    SkinnedModel(SkinnedModelInfo),
    Billboard(BillboardInfo),
}
pub struct RenderObject;

pub struct TextObject;

pub struct GuiObject;

#[derive(Clone, Debug)]
pub enum TextRenderMode {
    Plain,
    Sdf { font: String },
}

impl Default for TextRenderMode {
    fn default() -> Self {
        Self::Plain
    }
}

#[derive(Clone, Debug)]
pub struct TextInfo {
    pub text: String,
    pub position: Vec2,
    pub color: Vec4,
    pub scale: f32,
    pub render_mode: TextRenderMode,
}

impl Default for TextInfo {
    fn default() -> Self {
        Self {
            text: String::new(),
            position: Vec2::ZERO,
            color: Vec4::ONE,
            scale: 1.0,
            render_mode: TextRenderMode::Plain,
        }
    }
}

#[derive(Clone, Debug)]
pub enum GuiRenderMode {
    Solid,
    Textured {
        texture_id: Option<u32>,
        uv_min: Vec2,
        uv_max: Vec2,
    },
}

impl Default for GuiRenderMode {
    fn default() -> Self {
        Self::Solid
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GuiScissorInfo {
    pub enabled: bool,
    pub position: Vec2,
    pub size: Vec2,
}

impl Default for GuiScissorInfo {
    fn default() -> Self {
        Self {
            enabled: false,
            position: Vec2::ZERO,
            size: Vec2::ZERO,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GuiInfo {
    pub position: Vec2,
    pub size: Vec2,
    pub color: Vec4,
    pub layer: i32,
    pub render_mode: GuiRenderMode,
    pub scissor: GuiScissorInfo,
}

impl Default for GuiInfo {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            size: Vec2::ZERO,
            color: Vec4::ONE,
            layer: 0,
            render_mode: GuiRenderMode::Solid,
            scissor: GuiScissorInfo::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SkinnedModelInfo {
    pub model: DeviceModel,
    pub animation: AnimationState,
}

#[derive(Clone, Copy, Debug)]
pub struct AnimationState {
    pub clip_index: u32,
    pub time_seconds: f32,
    pub speed: f32,
    pub looping: bool,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            clip_index: 0,
            time_seconds: 0.0,
            speed: 1.0,
            looping: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BillboardInfo {
    pub texture_id: u32,
    pub material: Option<Handle<Material>>,
    pub billboard_type: BillboardType,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BillboardType {
    #[default]
    ScreenAligned,
    AxisAligned,
    Fixed,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudResolutionScale {
    Half,
    Quarter,
}

impl Default for CloudResolutionScale {
    fn default() -> Self {
        Self::Quarter
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudDebugView {
    None = 0,
    WeatherMap = 1,
    ShadowMap = 2,
    Transmittance = 3,
    StepHeatmap = 4,
    TemporalWeight = 5,
    Stats = 6,
    LayerA = 7,
    LayerB = 8,
    SingleScatter = 9,
    MultiScatter = 10,
    ShadowCascade0 = 11,
    ShadowCascade1 = 12,
    ShadowCascade2 = 13,
    ShadowCascade3 = 14,
    OpaqueShadowCascade0 = 20,
    OpaqueShadowCascade1 = 21,
    OpaqueShadowCascade2 = 22,
    OpaqueShadowCascade3 = 23,
}

#[derive(Clone, Copy, Debug)]
pub struct ShadowCascadeSettings {
    pub cascade_count: u32,
    pub split_lambda: f32,
    pub cascade_splits: [f32; 4],
    pub cascade_extents: [f32; 4],
    pub cascade_resolutions: [u32; 4],
    pub cascade_strengths: [f32; 4],
}

impl Default for ShadowCascadeSettings {
    fn default() -> Self {
        Self {
            cascade_count: 4,
            split_lambda: 0.1,
            cascade_splits:  [0.05, 0.15, 0.35, 1.0],
            cascade_extents:  [2000.0, 4000.0, 8000.0, 12000.0],
            cascade_resolutions: [256; 4],
            cascade_strengths: [1.0; 4],
        }
    }
}

impl ShadowCascadeSettings {
    pub fn compute_splits(&self, near: f32, far: f32) -> [f32; 4] {
        let mut splits = [far; 4];
        let count = self.cascade_count.clamp(1, 4) as usize;
        if count == 0 {
            return splits;
        }

        let safe_near = near.max(0.01);
        let safe_far = far.max(safe_near + 0.01);
        let lambda = self.split_lambda.clamp(0.0, 1.0);

        for cascade_index in 0..count {
            let p = (cascade_index + 1) as f32 / count as f32;
            let log = safe_near * (safe_far / safe_near).powf(p);
            let uniform = safe_near + (safe_far - safe_near) * p;
            splits[cascade_index] = log * lambda + uniform * (1.0 - lambda);
        }

        splits
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CloudShadowSettings {
    pub enabled: bool,
    pub resolution: u32,
    pub extent: f32,
    pub strength: f32,
    pub cascades: ShadowCascadeSettings,
}

impl Default for CloudShadowSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            resolution: 128,
            extent: 50000.0,
            strength: 1.0,
            cascades: ShadowCascadeSettings::default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CloudTemporalSettings {
    pub blend_factor: f32,
    pub clamp_strength: f32,
    pub depth_sigma: f32,
    pub history_weight_scale: f32,
}

impl Default for CloudTemporalSettings {
    fn default() -> Self {
        Self {
            blend_factor: 0.95,
            clamp_strength: 0.7,
            depth_sigma: 15.0,
            history_weight_scale: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CloudLayerSettings {
    pub base_altitude: f32,
    pub top_altitude: f32,
    pub density_scale: f32,
    pub noise_scale: f32,
    pub wind: Vec2,
    pub wind_speed: f32,
}

impl Default for CloudLayerSettings {
    fn default() -> Self {
        Self {
            base_altitude: 300.0,
            top_altitude: 400.0,
            density_scale: 0.5,
            noise_scale: 1.0,
            wind: Vec2::new(1.0, 0.0),
            wind_speed: 0.2,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CloudSettings {
    pub enabled: bool,
    pub layer_a: CloudLayerSettings,
    pub layer_b: CloudLayerSettings,
    pub step_count: u32,
    pub light_step_count: u32,
    pub phase_g: f32,
    pub multi_scatter_strength: f32,
    pub multi_scatter_respects_shadow: bool,
    pub low_res_scale: CloudResolutionScale,
    pub coverage_power: f32,
    pub detail_strength: f32,
    pub curl_strength: f32,
    pub jitter_strength: f32,
    pub epsilon: f32,
    pub sun_radiance: Vec3,
    pub sun_direction: Vec3,
    pub atmosphere_view_strength: f32,
    pub atmosphere_view_extinction: f32,
    pub atmosphere_light_transmittance: f32,
    pub atmosphere_haze_strength: f32,
    pub atmosphere_haze_color: Vec3,
    pub shadow: CloudShadowSettings,
    pub temporal: CloudTemporalSettings,
    pub debug_view: CloudDebugView,
    pub performance_budget_ms: f32,
    pub debug_enabled: f32,
    pub debug_step_count: f32,
    pub debug_light_step_count: f32,
    pub debug_low_res_scale: f32,
    pub debug_multi_scatter_shadowed: f32,
    pub debug_shadow_enabled: f32,
    pub debug_shadow_resolution: f32,
    pub debug_shadow_cascade_count: f32,
    pub debug_view_value: f32,
}

impl Default for CloudSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            layer_a: CloudLayerSettings::default(),
            layer_b: CloudLayerSettings {
                base_altitude: 650.0,
                top_altitude: 900.0,
                density_scale: 0.22,
                noise_scale: 0.7,
                wind: Vec2::new(-0.4, 0.2),
                wind_speed: 0.35,
            },
            step_count: 64,
            light_step_count: 12,
            phase_g: 0.6,
            multi_scatter_strength: 0.35,
            multi_scatter_respects_shadow: true,
            low_res_scale: CloudResolutionScale::Quarter,
            coverage_power: 1.2,
            detail_strength: 0.6,
            curl_strength: 0.0,
            jitter_strength: 1.0,
            epsilon: 0.01,
            sun_radiance: Vec3::new(1.0, 1.0, 1.0),
            sun_direction: Vec3::new(0.0, -1.0, 0.0),
            atmosphere_view_strength: 0.6,
            atmosphere_view_extinction: 0.00025,
            atmosphere_light_transmittance: 0.9,
            atmosphere_haze_strength: 0.35,
            atmosphere_haze_color: Vec3::new(0.62, 0.72, 0.85),
            shadow: CloudShadowSettings::default(),
            temporal: CloudTemporalSettings::default(),
            debug_view: CloudDebugView::None,
            performance_budget_ms: 4.0,
            debug_enabled: 0.0,
            debug_step_count: 64.0,
            debug_light_step_count: 12.0,
            debug_low_res_scale: 1.0,
            debug_multi_scatter_shadowed: 1.0,
            debug_shadow_enabled: 0.0,
            debug_shadow_resolution: 128.0,
            debug_shadow_cascade_count: 1.0,
            debug_view_value: 0.0,
        }
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
            out.position_type = Vec3::ZERO.extend(out.position_type.w);
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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RendererSelect {
    #[default]
    Deferred,
    Forward,
}

#[derive(Default)]
pub struct RenderEngineInfo {
    pub headless: bool,
    pub canvas_extent: Option<[u32; 2]>,
    pub renderer: RendererSelect,
    pub sample_count: Option<SampleCount>,
    pub skybox_cubemap_entry: Option<String>,
    pub debug_mode: bool,
    pub shadow_cascades: ShadowCascadeSettings,
}
