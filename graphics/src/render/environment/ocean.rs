use super::EnvironmentRendererInfo;
use bento::builder::{AttachmentDesc, CSOBuilder, PSO, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use dashi::cmd::{Executable, PendingGraphics};
use dashi::driver::command::{Dispatch, Draw};
use dashi::{
    AspectMask, Buffer, BufferInfo, BufferUsage, CommandStream, Context, DynamicAllocator, Format,
    Handle, ImageInfo, ImageView, ImageViewType, MemoryVisibility, Sampler, SamplerInfo,
    ShaderResource, SubresourceRange, UsageBits, Viewport,
};
use furikake::BindlessState;
use furikake::PSOBuilderFurikakeExt;
use furikake::types::Camera;
use glam::{Mat4, Vec2, Vec3, Vec4};
use tracing::warn;

use crate::gui::Slider;
use crate::gui::debug::{
    DebugRadialOption, DebugRegistryValue, PageType, debug_register_int_with_description,
    debug_register_radial_with_description, debug_register_with_description,
};

#[derive(Clone, Copy)]
pub struct OceanInfo {
    /// World-space half-size of a single ocean patch in meters.
    pub patch_size: f32,
    /// Tessellation resolution for each patch; higher values add detail at higher cost.
    pub vertex_resolution: u32,
    /// FFT grid sizes for near, mid, and far cascades.
    pub cascade_fft_sizes: [u32; 3],
    /// Patch size multipliers for near, mid, and far cascades.
    pub cascade_patch_scales: [f32; 3],
    /// Base tile radius (1 -> 3x3 grid).
    pub base_tile_radius: u32,
    /// Maximum tile radius for far-field coverage.
    pub max_tile_radius: u32,
    /// Maximum tile radius used when matching the camera far plane.
    pub far_tile_radius: u32,
    /// Camera-height step (meters) before expanding tiles.
    pub tile_height_step: f32,
}

impl Default for OceanInfo {
    fn default() -> Self {
        Self {
            patch_size: 200.0,
            vertex_resolution: 128,
            cascade_fft_sizes: [256, 128, 64],
            cascade_patch_scales: [0.1, 1.0, 4.0],
            base_tile_radius: 1,
            max_tile_radius: 8,
            far_tile_radius: 8,
            tile_height_step: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OceanDebugView {
    None = 0,
    Normals = 1,
    WaveHeight = 2,
    FoamMask = 3,
    Velocity = 4,
}

impl Default for OceanDebugView {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Copy)]
pub struct OceanFrameSettings {
    pub enabled: bool,
    /// Enables an endless ocean clipmap by always rendering the maximum tile radius.
    pub endless: bool,
    /// Normalized wind direction used to align the wave spectrum.
    pub wind_dir: Vec2,
    /// Wind speed in meters per second; higher values create taller, faster waves.
    pub wind_speed: f32,
    /// Fetch length in meters for spectral peak tuning.
    pub fetch_length: f32,
    /// Swell direction to bias long-wavelength waves.
    pub swell_dir: Vec2,
    /// Surface current velocity in meters per second.
    pub current: Vec2,
    /// Scales overall wave height, slope, and velocity (1.0 = default).
    pub wave_amplitude: f32,
    /// Multiplier for Gerstner wave amplitude relative to `wave_amplitude`.
    pub gerstner_amplitude: f32,
    /// Per-cascade spectrum amplitude multipliers for near, mid, and far cascades.
    pub cascade_spectrum_scales: [f32; 3],
    /// Per-cascade swell blend factors for near, mid, and far cascades.
    pub cascade_swell_strengths: [f32; 3],
    /// Water depth in meters used for shallow-water damping.
    pub depth_meters: f32,
    /// Blend factor for depth-dependent damping (0 = off, 1 = full).
    pub depth_damping: f32,
    /// Base reflectance for the Fresnel term.
    pub fresnel_bias: f32,
    /// Scales Fresnel reflectance contribution when blending reflections.
    pub fresnel_strength: f32,
    /// Overall foam intensity multiplier.
    pub foam_strength: f32,
    /// Foam threshold for the breaking/curvature mask.
    pub foam_threshold: f32,
    /// Scales foam texture advection speed.
    pub foam_advection_strength: f32,
    /// Foam decay rate (higher values fade foam faster).
    pub foam_decay_rate: f32,
    /// Foam texture scale for procedural noise.
    pub foam_noise_scale: f32,
    /// Scales high-frequency capillary detail (1.0 = default).
    pub capillary_strength: f32,
    /// Time multiplier for wave evolution; values above 1.0 speed up the animation.
    pub time_scale: f32,
    /// Beer-Lambert absorption coefficients (per meter) for RGB channels.
    pub absorption_coeff: Vec3,
    /// Shallow-water tint used for depth-based turbidity ramps.
    pub shallow_color: Vec3,
    /// Deep-water tint used for depth-based turbidity ramps.
    pub deep_color: Vec3,
    /// Single-scatter tint color for volumetric approximation.
    pub scattering_color: Vec3,
    /// Strength of the volumetric single-scatter approximation.
    pub scattering_strength: f32,
    /// Depth range (meters) used for shallow-water color ramps.
    pub turbidity_depth: f32,
    /// Screen-space refraction distortion strength.
    pub refraction_strength: f32,
    /// Screen-space reflection blend strength.
    pub ssr_strength: f32,
    /// Maximum ray distance (meters) for SSR.
    pub ssr_max_distance: f32,
    /// Depth thickness tolerance for SSR hits (meters).
    pub ssr_thickness: f32,
    /// Max number of SSR raymarch steps.
    pub ssr_steps: u32,
    /// Debug rendering mode for the ocean surface.
    pub debug_view: OceanDebugView,
}

impl Default for OceanFrameSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            endless: false,
            wind_dir: Vec2::new(0.9, 0.2),
            wind_speed: 2.0,
            fetch_length: 5000.0,
            swell_dir: Vec2::new(0.8, 0.1),
            current: Vec2::ZERO,
            wave_amplitude: 2.0,
            gerstner_amplitude: 0.12,
            cascade_spectrum_scales: [400.0, 40.85, 4.65],
            cascade_swell_strengths: [0.35, 0.55, 0.75],
            depth_meters: 200.0,
            depth_damping: 0.3,
            fresnel_bias: 0.02,
            fresnel_strength: 0.85,
            foam_strength: 1.0,
            foam_threshold: 0.55,
            foam_advection_strength: 0.25,
            foam_decay_rate: 0.08,
            foam_noise_scale: 0.2,
            capillary_strength: 1.0,
            time_scale: 1.0,
            absorption_coeff: Vec3::new(0.18, 0.07, 0.03),
            shallow_color: Vec3::new(0.05, 0.08, 0.1),
            deep_color: Vec3::new(0.0, 0.08, 0.2),
            scattering_color: Vec3::new(0.05, 0.05, 0.15),
            scattering_strength: 0.06,
            turbidity_depth: 8.0,
            refraction_strength: 0.02,
            ssr_strength: 0.7,
            ssr_max_distance: 120.0,
            ssr_thickness: 1.2,
            ssr_steps: 24,
            debug_view: OceanDebugView::None,
        }
    }
}

impl OceanFrameSettings {
    pub fn register_debug(&mut self) {
        unsafe {
            debug_register_radial_with_description(
                PageType::Ocean,
                "Ocean Enabled",
                DebugRegistryValue::Bool(&mut self.enabled),
                &[
                    DebugRadialOption {
                        label: "On",
                        value: 1.0,
                    },
                    DebugRadialOption {
                        label: "Off",
                        value: 0.0,
                    },
                ],
                Some("Toggle ocean rendering in the scene."),
            );
            debug_register_radial_with_description(
                PageType::Ocean,
                "Ocean Endless",
                DebugRegistryValue::Bool(&mut self.endless),
                &[
                    DebugRadialOption {
                        label: "On",
                        value: 1.0,
                    },
                    DebugRadialOption {
                        label: "Off",
                        value: 0.0,
                    },
                ],
                Some("Forces the ocean grid to render at its maximum radius."),
            );
            debug_register_radial_with_description(
                PageType::Ocean,
                "Debug View",
                DebugRegistryValue::OceanDebugView(&mut self.debug_view),
                &[
                    DebugRadialOption {
                        label: "None",
                        value: 0.0,
                    },
                    DebugRadialOption {
                        label: "Normals",
                        value: 1.0,
                    },
                    DebugRadialOption {
                        label: "Wave Height",
                        value: 2.0,
                    },
                    DebugRadialOption {
                        label: "Foam Mask",
                        value: 3.0,
                    },
                    DebugRadialOption {
                        label: "Velocity",
                        value: 4.0,
                    },
                ],
                Some("Selects an ocean debug visualization mode."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Wind Speed", 0.1, 20.0, 0.0),
                &mut self.wind_speed as *mut f32,
                "Wind Speed",
                Some("Controls the wind speed driving wave generation."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Fetch Length", 10.0, 200000.0, 0.0),
                &mut self.fetch_length as *mut f32,
                "Fetch Length",
                Some("Distance over which wind generates the wave spectrum."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Swell Dir X", -1.0, 1.0, 0.0),
                &mut self.swell_dir.x as *mut f32,
                "Swell Dir X",
                Some("X component of the swell direction."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Swell Dir Y", -1.0, 1.0, 0.0),
                &mut self.swell_dir.y as *mut f32,
                "Swell Dir Y",
                Some("Y component of the swell direction."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Current X", -5.0, 5.0, 0.0),
                &mut self.current.x as *mut f32,
                "Current X",
                Some("X component of the ocean current vector."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Current Y", -5.0, 5.0, 0.0),
                &mut self.current.y as *mut f32,
                "Current Y",
                Some("Y component of the ocean current vector."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Wave Amplitude", 0.1, 10.0, 0.0),
                &mut self.wave_amplitude as *mut f32,
                "Wave Amplitude",
                Some("Scales the overall wave height."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Gerstner Amplitude", 0.0, 1.0, 0.0),
                &mut self.gerstner_amplitude as *mut f32,
                "Gerstner Amplitude",
                Some("Controls the contribution of Gerstner waves."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Spectrum Near", 0.0, 100.0, 0.0),
                &mut self.cascade_spectrum_scales[0] as *mut f32,
                "Cascade Spectrum Near",
                Some("Spectrum scale for the near cascade."),
            );

            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Spectrum Mid", 0.0, 100.0, 0.0),
                &mut self.cascade_spectrum_scales[1] as *mut f32,
                "Cascade Spectrum Mid",
                Some("Spectrum scale for the mid cascade."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Spectrum Far", 0.0, 100.0, 0.0),
                &mut self.cascade_spectrum_scales[2] as *mut f32,
                "Cascade Spectrum Far",
                Some("Spectrum scale for the far cascade."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Swell Near", 0.0, 1.0, 0.0),
                &mut self.cascade_swell_strengths[0] as *mut f32,
                "Cascade Swell Near",
                Some("Swell strength for the near cascade."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Swell Mid", 0.0, 1.0, 0.0),
                &mut self.cascade_swell_strengths[1] as *mut f32,
                "Cascade Swell Mid",
                Some("Swell strength for the mid cascade."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Swell Far", 0.0, 1.0, 0.0),
                &mut self.cascade_swell_strengths[2] as *mut f32,
                "Cascade Swell Far",
                Some("Swell strength for the far cascade."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Depth Meters", 0.0, 5000.0, 0.0),
                &mut self.depth_meters as *mut f32,
                "Depth Meters",
                Some("Water depth used for attenuation and refraction."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Depth Damping", 0.0, 1.0, 0.0),
                &mut self.depth_damping as *mut f32,
                "Depth Damping",
                Some("Controls how quickly waves damp with depth."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Fresnel Bias", 0.0, 0.2, 0.0),
                &mut self.fresnel_bias as *mut f32,
                "Fresnel Bias",
                Some("Bias applied to the Fresnel term."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Fresnel Strength", 0.0, 1.5, 0.0),
                &mut self.fresnel_strength as *mut f32,
                "Fresnel Strength",
                Some("Strength of the Fresnel reflectance."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Foam Strength", 0.0, 4.0, 0.0),
                &mut self.foam_strength as *mut f32,
                "Foam Strength",
                Some("Intensity of foam shading."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Foam Threshold", 0.0, 1.0, 0.0),
                &mut self.foam_threshold as *mut f32,
                "Foam Threshold",
                Some("Threshold for generating foam."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Foam Advection", 0.0, 2.0, 0.0),
                &mut self.foam_advection_strength as *mut f32,
                "Foam Advection",
                Some("Controls how foam advects across the surface."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Foam Decay", 0.0, 1.0, 0.0),
                &mut self.foam_decay_rate as *mut f32,
                "Foam Decay",
                Some("Rate at which foam fades over time."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Foam Noise Scale", 0.01, 1.0, 0.0),
                &mut self.foam_noise_scale as *mut f32,
                "Foam Noise Scale",
                Some("Noise scale applied to foam patterns."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Capillary Strength", 0.0, 2.0, 0.0),
                &mut self.capillary_strength as *mut f32,
                "Capillary Strength",
                Some("Strength of small capillary waves."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Time Scale", 0.1, 4.0, 0.0),
                &mut self.time_scale as *mut f32,
                "Time Scale",
                Some("Scales the simulation time for waves."),
            );

        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct OceanSpectrumParams {
    fft_size: u32,
    time: f32,
    time_scale: f32,
    wave_amplitude: f32,
    wind_dir: Vec2,
    wind_speed: f32,
    patch_size: f32,
    spectrum_scale: f32,
    fetch_length: f32,
    swell_dir: Vec2,
    swell_strength: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct OceanFftParams {
    fft_size: u32,
    stage: u32,
    direction: u32,
    bit_reverse: u32,
    inverse: f32,
    _padding: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct OceanFinalizeParams {
    fft_size: u32,
    _padding: [u32; 3],
}

#[repr(packed)]
#[derive(Debug, Clone, Copy)]
struct OceanDrawParams {
    cascade_fft_sizes: [u32; 4],
    cascade_patch_sizes: [f32; 4],
    cascade_blend_ranges: [f32; 4],
    vertex_resolution: u32,
    camera_index: u32,
    base_tile_radius: u32,
    max_tile_radius: u32,
    far_tile_radius: u32,
    tile_height_step: f32,
    endless: u32,
    time: f32,
    wind_dir: Vec2,
    wind_speed: f32,
    gerstner_amplitude: f32,
    fresnel_bias: f32,
    fresnel_strength: f32,
    foam_strength: f32,
    foam_threshold: f32,
    foam_advection_strength: f32,
    foam_decay_rate: f32,
    foam_noise_scale: f32,
    current: Vec2,
    _padding1: Vec3,
    absorption_coeff: Vec4,
    shallow_color: Vec4,
    deep_color: Vec4,
    scattering_color: Vec4,
    scattering_strength: f32,
    turbidity_depth: f32,
    refraction_strength: f32,
    ssr_strength: f32,
    ssr_max_distance: f32,
    ssr_thickness: f32,
    ssr_steps: u32,
    debug_view: f32,
    _padding2: Vec3,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct OceanShadowParams {
    shadow_cascade_count: u32,
    shadow_resolution: u32,
    shadow_padding0: u32,
    shadow_padding1: u32,
    shadow_splits: Vec4,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct OceanShadowMatrices {
    shadow_matrices: [Mat4; 4],
}


#[derive(Debug)]
struct OceanCascade {
    fft_size: u32,
    patch_size: f32,
    wave_buffer: Handle<Buffer>,
    spectrum_buffer: Handle<Buffer>,
    ping_buffer: Handle<Buffer>,
    pong_buffer: Handle<Buffer>,
    spectrum_pipeline: Option<bento::builder::CSO>,
    fft_spectrum_to_ping: Option<bento::builder::CSO>,
    fft_ping_to_pong: Option<bento::builder::CSO>,
    fft_pong_to_ping: Option<bento::builder::CSO>,
    finalize_from_ping: Option<bento::builder::CSO>,
    finalize_from_pong: Option<bento::builder::CSO>,
}

pub struct OceanRenderer {
    pipeline: PSO,
    cascades: [OceanCascade; 3],
    vertex_resolution: u32,
    base_tile_radius: u32,
    max_tile_radius: u32,
    far_tile_radius: u32,
    tile_height_step: f32,
    endless: bool,
    wind_dir: Vec2,
    wind_speed: f32,
    wave_amplitude: f32,
    gerstner_amplitude: f32,
    fresnel_bias: f32,
    fresnel_strength: f32,
    foam_strength: f32,
    foam_threshold: f32,
    foam_advection_strength: f32,
    foam_decay_rate: f32,
    foam_noise_scale: f32,
    capillary_strength: f32,
    time_scale: f32,
    fetch_length: f32,
    swell_dir: Vec2,
    current: Vec2,
    cascade_spectrum_scales: [f32; 3],
    cascade_swell_strengths: [f32; 3],
    depth_meters: f32,
    depth_damping: f32,
    absorption_coeff: Vec3,
    shallow_color: Vec3,
    deep_color: Vec3,
    scattering_color: Vec3,
    scattering_strength: f32,
    turbidity_depth: f32,
    refraction_strength: f32,
    ssr_strength: f32,
    ssr_max_distance: f32,
    ssr_thickness: f32,
    ssr_steps: u32,
    debug_view: OceanDebugView,
    use_depth: bool,
    environment_sampler: Handle<Sampler>,
    scene_sampler: Handle<Sampler>,
    scene_color_fallback: ImageView,
    scene_depth_fallback: ImageView,
    shadow_cascade_count: u32,
    shadow_resolution: u32,
    shadow_splits: Vec4,
    shadow_matrices: [Mat4; 4],
    enabled: bool,
}

fn create_scene_color_fallback(
    ctx: &mut Context,
    format: Format,
    sample_count: dashi::SampleCount,
) -> ImageView {
    let data = vec![0u8, 0, 0, 255];
    let info = ImageInfo {
        debug_name: "[MESHI GFX OCEAN] Scene Color Fallback",
        dim: [1, 1, 1],
        layers: 1,
        format,
        mip_levels: 1,
        samples: sample_count,
        initial_data: Some(&data),
        ..Default::default()
    };
    let image = ctx
        .make_image(&info)
        .expect("Failed to create ocean scene color fallback image");

    ImageView {
        img: image,
        aspect: AspectMask::Color,
        view_type: ImageViewType::Type2D,
        range: SubresourceRange::new(0, 1, 0, 1),
    }
}

fn create_scene_depth_fallback(ctx: &mut Context, sample_count: dashi::SampleCount) -> ImageView {
    let info = ImageInfo {
        debug_name: "[MESHI GFX OCEAN] Scene Depth Fallback",
        dim: [1, 1, 1],
        layers: 1,
        format: Format::D24S8,
        mip_levels: 1,
        samples: sample_count,
        initial_data: None,
        ..Default::default()
    };
    let image = ctx
        .make_image(&info)
        .expect("Failed to create ocean scene depth fallback image");

    ImageView {
        img: image,
        aspect: AspectMask::Depth,
        view_type: ImageViewType::Type2D,
        range: SubresourceRange::new(0, 1, 0, 1),
    }
}

fn compile_ocean_shaders() -> [bento::CompilationResult; 2] {
    let compiler = Compiler::new().expect("Failed to create shader compiler");
    let base_request = Request {
        name: Some("environment_ocean".to_string()),
        lang: ShaderLang::Glsl,
        optimization: OptimizationLevel::Performance,
        debug_symbols: true,
        ..Default::default()
    };

    let vertex = compiler
        .compile(
            include_str!("shaders/environment_ocean.vert.glsl").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Vertex,
                ..base_request.clone()
            },
        )
        .expect("Failed to compile ocean vertex shader");

    let fragment = compiler
        .compile(
            include_str!("shaders/environment_ocean.frag.glsl").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Fragment,
                ..base_request
            },
        )
        .expect("Failed to compile ocean fragment shader");

    [vertex, fragment]
}

impl OceanRenderer {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        info: &EnvironmentRendererInfo,
        dynamic: &DynamicAllocator,
        environment_map: ImageView,
    ) -> Self {
        let ocean_info = info.ocean;
        let mut cascades = Vec::with_capacity(3);
        for (index, fft_size) in ocean_info.cascade_fft_sizes.iter().enumerate() {
            let patch_scale = ocean_info
                .cascade_patch_scales
                .get(index)
                .copied()
                .unwrap_or(1.0);
            let patch_size = ocean_info.patch_size * patch_scale;
            let wave_buffer = ctx
                .make_buffer(&BufferInfo {
                    debug_name: "[MESHI GFX OCEAN] Wave Buffer",
                    byte_size: fft_size * fft_size * 16,
                    visibility: MemoryVisibility::Gpu,
                    usage: BufferUsage::STORAGE,
                    initial_data: None,
                })
                .expect("Failed to create ocean wave buffer");
            let spectrum_buffer = ctx
                .make_buffer(&BufferInfo {
                    debug_name: "[MESHI GFX OCEAN] Spectrum Buffer",
                    byte_size: fft_size * fft_size * 16,
                    visibility: MemoryVisibility::Gpu,
                    usage: BufferUsage::STORAGE,
                    initial_data: None,
                })
                .expect("Failed to create ocean spectrum buffer");
            let ping_buffer = ctx
                .make_buffer(&BufferInfo {
                    debug_name: "[MESHI GFX OCEAN] FFT Ping Buffer",
                    byte_size: fft_size * fft_size * 16,
                    visibility: MemoryVisibility::Gpu,
                    usage: BufferUsage::STORAGE,
                    initial_data: None,
                })
                .expect("Failed to create ocean FFT ping buffer");
            let pong_buffer = ctx
                .make_buffer(&BufferInfo {
                    debug_name: "[MESHI GFX OCEAN] FFT Pong Buffer",
                    byte_size: fft_size * fft_size * 16,
                    visibility: MemoryVisibility::Gpu,
                    usage: BufferUsage::STORAGE,
                    initial_data: None,
                })
                .expect("Failed to create ocean FFT pong buffer");

            let spectrum_pipeline = match CSOBuilder::new()
                .shader(Some(
                    include_str!("shaders/environment_ocean_spectrum.comp.glsl").as_bytes(),
                ))
                .set_debug_name("[MESHI] Ocean Spectrum")
                .add_reserved_table_variables(state)
                .unwrap()
                .add_variable(
                    "ocean_spectrum",
                    ShaderResource::StorageBuffer(spectrum_buffer.into()),
                )
                .add_variable("params", ShaderResource::DynamicStorage(dynamic.state()))
                .build(ctx)
            {
                Ok(pipeline) => Some(pipeline),
                Err(err) => {
                    warn!(
                        "Ocean spectrum pipeline creation failed: {err}. Falling back to static waves."
                    );
                    None
                }
            };

            let fft_spectrum_to_ping = match CSOBuilder::new()
                .shader(Some(
                    include_str!("shaders/environment_ocean_fft.comp.glsl").as_bytes(),
                ))
                .set_debug_name("[MESHI] Ocean FFT Spectrum->Ping")
                .add_reserved_table_variables(state)
                .unwrap()
                .add_variable(
                    "spectrum_in",
                    ShaderResource::StorageBuffer(spectrum_buffer.into()),
                )
                .add_variable(
                    "spectrum_out",
                    ShaderResource::StorageBuffer(ping_buffer.into()),
                )
                .add_variable("params", ShaderResource::DynamicStorage(dynamic.state()))
                .build(ctx)
            {
                Ok(pipeline) => Some(pipeline),
                Err(err) => {
                    warn!(
                        "Ocean FFT spectrum pipeline creation failed: {err}. Falling back to static waves."
                    );
                    None
                }
            };

            let fft_ping_to_pong = match CSOBuilder::new()
                .shader(Some(
                    include_str!("shaders/environment_ocean_fft.comp.glsl").as_bytes(),
                ))
                .set_debug_name("[MESHI] Ocean FFT Ping->Pong")
                .add_reserved_table_variables(state)
                .unwrap()
                .add_variable(
                    "spectrum_in",
                    ShaderResource::StorageBuffer(ping_buffer.into()),
                )
                .add_variable(
                    "spectrum_out",
                    ShaderResource::StorageBuffer(pong_buffer.into()),
                )
                .add_variable("params", ShaderResource::DynamicStorage(dynamic.state()))
                .build(ctx)
            {
                Ok(pipeline) => Some(pipeline),
                Err(err) => {
                    warn!(
                        "Ocean FFT ping pipeline creation failed: {err}. Falling back to static waves."
                    );
                    None
                }
            };

            let fft_pong_to_ping = match CSOBuilder::new()
                .shader(Some(
                    include_str!("shaders/environment_ocean_fft.comp.glsl").as_bytes(),
                ))
                .set_debug_name("[MESHI] Ocean FFT Pong->Ping")
                .add_reserved_table_variables(state)
                .unwrap()
                .add_variable(
                    "spectrum_in",
                    ShaderResource::StorageBuffer(pong_buffer.into()),
                )
                .add_variable(
                    "spectrum_out",
                    ShaderResource::StorageBuffer(ping_buffer.into()),
                )
                .add_variable("params", ShaderResource::DynamicStorage(dynamic.state()))
                .build(ctx)
            {
                Ok(pipeline) => Some(pipeline),
                Err(err) => {
                    warn!(
                        "Ocean FFT pong pipeline creation failed: {err}. Falling back to static waves."
                    );
                    None
                }
            };

            let finalize_from_ping = match CSOBuilder::new()
                .shader(Some(
                    include_str!("shaders/environment_ocean_finalize.comp.glsl").as_bytes(),
                ))
                .set_debug_name("[MESHI] Ocean FFT Finalize Ping")
                .add_reserved_table_variables(state)
                .unwrap()
                .add_variable(
                    "spectrum_spatial",
                    ShaderResource::StorageBuffer(ping_buffer.into()),
                )
                .add_variable(
                    "ocean_waves",
                    ShaderResource::StorageBuffer(wave_buffer.into()),
                )
                .add_variable("params", ShaderResource::DynamicStorage(dynamic.state()))
                .build(ctx)
            {
                Ok(pipeline) => Some(pipeline),
                Err(err) => {
                    warn!(
                        "Ocean FFT finalize pipeline creation failed: {err}. Falling back to static waves."
                    );
                    None
                }
            };

            let finalize_from_pong = match CSOBuilder::new()
                .shader(Some(
                    include_str!("shaders/environment_ocean_finalize.comp.glsl").as_bytes(),
                ))
                .set_debug_name("[MESHI] Ocean FFT Finalize Pong")
                .add_reserved_table_variables(state)
                .unwrap()
                .add_variable(
                    "spectrum_spatial",
                    ShaderResource::StorageBuffer(pong_buffer.into()),
                )
                .add_variable(
                    "ocean_waves",
                    ShaderResource::StorageBuffer(wave_buffer.into()),
                )
                .add_variable("params", ShaderResource::DynamicStorage(dynamic.state()))
                .build(ctx)
            {
                Ok(pipeline) => Some(pipeline),
                Err(err) => {
                    warn!(
                        "Ocean FFT finalize pipeline creation failed: {err}. Falling back to static waves."
                    );
                    None
                }
            };

            cascades.push(OceanCascade {
                fft_size: *fft_size,
                patch_size,
                wave_buffer,
                spectrum_buffer,
                ping_buffer,
                pong_buffer,
                spectrum_pipeline,
                fft_spectrum_to_ping,
                fft_ping_to_pong,
                fft_pong_to_ping,
                finalize_from_ping,
                finalize_from_pong,
            });
        }

        let cascades: [OceanCascade; 3] =
            cascades.try_into().expect("Expected three ocean cascades");

        let shaders = compile_ocean_shaders();
        let environment_sampler = ctx
            .make_sampler(&SamplerInfo::default())
            .expect("Failed to create ocean environment sampler");
        let scene_sampler = ctx
            .make_sampler(&SamplerInfo::default())
            .expect("Failed to create ocean scene sampler");
        let scene_color_fallback =
            create_scene_color_fallback(ctx, info.color_format, info.sample_count);
        let scene_depth_fallback = create_scene_depth_fallback(ctx, info.sample_count);
        let wave_resources = cascades
            .iter()
            .enumerate()
            .map(|(slot, cascade)| dashi::IndexedResource {
                resource: ShaderResource::StorageBuffer(cascade.wave_buffer.into()),
                slot: slot as u32,
            })
            .collect();
        let mut pso_builder = PSOBuilder::new()
            .set_debug_name("[MESHI] Ocean")
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .set_attachment_format(0, info.color_format)
            .add_reserved_table_variables(state)
            .unwrap()
            .add_table_variable_with_resources("ocean_waves", wave_resources)
            .add_table_variable_with_resources(
                "params",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::DynamicStorage(dynamic.state()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "shadow_params",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::DynamicStorage(dynamic.state()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "shadow_matrices",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::DynamicStorage(dynamic.state()),
                    slot: 0,
                }],
            )

            .add_table_variable_with_resources(
                "ocean_env_map",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::Image(environment_map),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "ocean_env_sampler",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::Sampler(environment_sampler),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "ocean_scene_color",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::Image(scene_color_fallback),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "ocean_scene_depth",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::Image(scene_depth_fallback),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "ocean_scene_sampler",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::Sampler(scene_sampler),
                    slot: 0,
                }],
            );
        pso_builder = pso_builder
            .add_table_variable_with_resources(
                "ocean_shadow_map",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::Image(scene_depth_fallback),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "ocean_shadow_sampler",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::Sampler(scene_sampler),
                    slot: 0,
                }],
            );

        pso_builder = pso_builder
            .add_reserved_table_variables(state)
            .unwrap()
            .add_reserved_table_variable(state, "meshi_bindless_cameras")
            .unwrap()
            .add_reserved_table_variable(state, "meshi_bindless_lights")
            .unwrap();

        if info.use_depth {
            pso_builder = pso_builder.add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: info.sample_count,
            });
        }

        let depth_test = if info.use_depth {
            Some(dashi::DepthInfo {
                should_test: true,
                should_write: false,
            })
        } else {
            None
        };

        let pipeline = pso_builder
            .set_details(dashi::GraphicsPipelineDetails {
                color_blend_states: vec![Default::default(); 1],
                sample_count: info.sample_count,
                depth_test,
                ..Default::default()
            })
            .build(ctx)
            .expect("Failed to build ocean PSO");

        state.register_pso_tables(&pipeline);
        let default_frame = OceanFrameSettings::default();
        let base_tile_radius = ocean_info.base_tile_radius.max(1);
        let max_tile_radius = ocean_info.max_tile_radius.max(base_tile_radius);
        let far_tile_radius = ocean_info.far_tile_radius.max(base_tile_radius);
        Self {
            pipeline,
            cascades,
            vertex_resolution: ocean_info.vertex_resolution,
            base_tile_radius,
            max_tile_radius,
            far_tile_radius,
            tile_height_step: ocean_info.tile_height_step.max(1.0),
            endless: default_frame.endless,
            wind_dir: default_frame.wind_dir,
            wind_speed: default_frame.wind_speed,
            wave_amplitude: default_frame.wave_amplitude,
            gerstner_amplitude: default_frame.gerstner_amplitude,
            fresnel_bias: default_frame.fresnel_bias,
            fresnel_strength: default_frame.fresnel_strength,
            foam_strength: default_frame.foam_strength,
            foam_threshold: default_frame.foam_threshold,
            foam_advection_strength: default_frame.foam_advection_strength,
            foam_decay_rate: default_frame.foam_decay_rate,
            foam_noise_scale: default_frame.foam_noise_scale,
            capillary_strength: default_frame.capillary_strength,
            time_scale: default_frame.time_scale,
            fetch_length: default_frame.fetch_length,
            swell_dir: default_frame.swell_dir,
            current: default_frame.current,
            cascade_spectrum_scales: default_frame.cascade_spectrum_scales,
            cascade_swell_strengths: default_frame.cascade_swell_strengths,
            depth_meters: default_frame.depth_meters,
            depth_damping: default_frame.depth_damping,
            absorption_coeff: default_frame.absorption_coeff,
            shallow_color: default_frame.shallow_color,
            deep_color: default_frame.deep_color,
            scattering_color: default_frame.scattering_color,
            scattering_strength: default_frame.scattering_strength,
            turbidity_depth: default_frame.turbidity_depth,
            refraction_strength: default_frame.refraction_strength,
            ssr_strength: default_frame.ssr_strength,
            ssr_max_distance: default_frame.ssr_max_distance,
            ssr_thickness: default_frame.ssr_thickness,
            ssr_steps: default_frame.ssr_steps,
            debug_view: default_frame.debug_view,
            use_depth: info.use_depth,
            environment_sampler,
            scene_sampler,
            scene_color_fallback,
            scene_depth_fallback,
            shadow_cascade_count: 0,
            shadow_resolution: 0,
            shadow_splits: Vec4::ZERO,
            shadow_matrices: [Mat4::IDENTITY; 4],
            enabled: default_frame.enabled,
        }
    }

    pub fn update(&mut self, settings: OceanFrameSettings) {
        self.enabled = settings.enabled;
        self.endless = settings.endless;
        self.wind_dir = settings.wind_dir;
        self.wind_speed = settings.wind_speed;
        self.fetch_length = settings.fetch_length;
        self.swell_dir = settings.swell_dir;
        self.current = settings.current;
        self.wave_amplitude = settings.wave_amplitude;
        self.gerstner_amplitude = settings.gerstner_amplitude;
        self.cascade_spectrum_scales = settings.cascade_spectrum_scales;
        self.cascade_swell_strengths = settings.cascade_swell_strengths;
        self.depth_meters = settings.depth_meters;
        self.depth_damping = settings.depth_damping;
        self.fresnel_bias = settings.fresnel_bias;
        self.fresnel_strength = settings.fresnel_strength;
        self.foam_strength = settings.foam_strength;
        self.foam_threshold = settings.foam_threshold;
        self.foam_advection_strength = settings.foam_advection_strength;
        self.foam_decay_rate = settings.foam_decay_rate;
        self.foam_noise_scale = settings.foam_noise_scale;
        self.capillary_strength = settings.capillary_strength;
        self.time_scale = settings.time_scale;
        self.absorption_coeff = settings.absorption_coeff;
        self.shallow_color = settings.shallow_color;
        self.deep_color = settings.deep_color;
        self.scattering_color = settings.scattering_color;
        self.scattering_strength = settings.scattering_strength;
        self.turbidity_depth = settings.turbidity_depth;
        self.refraction_strength = settings.refraction_strength;
        self.ssr_strength = settings.ssr_strength;
        self.ssr_max_distance = settings.ssr_max_distance;
        self.ssr_thickness = settings.ssr_thickness;
        self.ssr_steps = settings.ssr_steps;
        self.debug_view = settings.debug_view;
    }

    pub fn register_debug(&mut self) {
        unsafe {
            debug_register_radial_with_description(
                PageType::Ocean,
                "Ocean Endless",
                DebugRegistryValue::Bool(&mut self.endless),
                &[
                    DebugRadialOption {
                        label: "On",
                        value: 1.0,
                    },
                    DebugRadialOption {
                        label: "Off",
                        value: 0.0,
                    },
                ],
                Some("Forces the ocean grid to render at its maximum radius."),
            );
            debug_register_radial_with_description(
                PageType::Ocean,
                "Debug View",
                DebugRegistryValue::OceanDebugView(&mut self.debug_view),
                &[
                    DebugRadialOption {
                        label: "None",
                        value: 0.0,
                    },
                    DebugRadialOption {
                        label: "Normals",
                        value: 1.0,
                    },
                    DebugRadialOption {
                        label: "Wave Height",
                        value: 2.0,
                    },
                    DebugRadialOption {
                        label: "Foam Mask",
                        value: 3.0,
                    },
                    DebugRadialOption {
                        label: "Velocity",
                        value: 4.0,
                    },
                ],
                Some("Selects an ocean debug visualization mode."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Wind Speed", 0.1, 20.0, 0.0),
                &mut self.wind_speed as *mut f32,
                "Wind Speed",
                Some("Controls the wind speed driving wave generation."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Wind Dir X", -1.0, 1.0, 0.0),
                &mut self.wind_dir.x as *mut f32,
                "Wind Dir X",
                Some("X component of the wind direction."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Wind Dir Y", -1.0, 1.0, 0.0),
                &mut self.wind_dir.y as *mut f32,
                "Wind Dir Y",
                Some("Y component of the wind direction."),
            );

            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Fetch Length", 10.0, 200000.0, 0.0),
                &mut self.fetch_length as *mut f32,
                "Fetch Length",
                Some("Distance over which wind generates the wave spectrum."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Swell Dir X", -1.0, 1.0, 0.0),
                &mut self.swell_dir.x as *mut f32,
                "Swell Dir X",
                Some("X component of the swell direction."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Swell Dir Y", -1.0, 1.0, 0.0),
                &mut self.swell_dir.y as *mut f32,
                "Swell Dir Y",
                Some("Y component of the swell direction."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Current X", -5.0, 5.0, 0.0),
                &mut self.current.x as *mut f32,
                "Current X",
                Some("X component of the ocean current vector."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Current Y", -5.0, 5.0, 0.0),
                &mut self.current.y as *mut f32,
                "Current Y",
                Some("Y component of the ocean current vector."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Wave Amplitude", 0.1, 10.0, 0.0),
                &mut self.wave_amplitude as *mut f32,
                "Wave Amplitude",
                Some("Scales the overall wave height."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Gerstner Amplitude", 0.0, 1.0, 0.0),
                &mut self.gerstner_amplitude as *mut f32,
                "Gerstner Amplitude",
                Some("Controls the contribution of Gerstner waves."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Spectrum Near", 0.0, 2.0, 0.0),
                &mut self.cascade_spectrum_scales[0] as *mut f32,
                "Cascade Spectrum Near",
                Some("Spectrum scale for the near cascade."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Spectrum Mid", 0.0, 2.0, 0.0),
                &mut self.cascade_spectrum_scales[1] as *mut f32,
                "Cascade Spectrum Mid",
                Some("Spectrum scale for the mid cascade."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Spectrum Far", 0.0, 2.0, 0.0),
                &mut self.cascade_spectrum_scales[2] as *mut f32,
                "Cascade Spectrum Far",
                Some("Spectrum scale for the far cascade."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Swell Near", 0.0, 1.0, 0.0),
                &mut self.cascade_swell_strengths[0] as *mut f32,
                "Cascade Swell Near",
                Some("Swell strength for the near cascade."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Swell Mid", 0.0, 1.0, 0.0),
                &mut self.cascade_swell_strengths[1] as *mut f32,
                "Cascade Swell Mid",
                Some("Swell strength for the mid cascade."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Cascade Swell Far", 0.0, 1.0, 0.0),
                &mut self.cascade_swell_strengths[2] as *mut f32,
                "Cascade Swell Far",
                Some("Swell strength for the far cascade."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Depth Meters", 0.0, 5000.0, 0.0),
                &mut self.depth_meters as *mut f32,
                "Depth Meters",
                Some("Water depth used for attenuation and refraction."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Depth Damping", 0.0, 1.0, 0.0),
                &mut self.depth_damping as *mut f32,
                "Depth Damping",
                Some("Controls how quickly waves damp with depth."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Fresnel Bias", 0.0, 0.2, 0.0),
                &mut self.fresnel_bias as *mut f32,
                "Fresnel Bias",
                Some("Bias applied to the Fresnel term."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Fresnel Strength", 0.0, 1.5, 0.0),
                &mut self.fresnel_strength as *mut f32,
                "Fresnel Strength",
                Some("Strength of the Fresnel reflectance."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Foam Strength", 0.0, 4.0, 0.0),
                &mut self.foam_strength as *mut f32,
                "Foam Strength",
                Some("Intensity of foam shading."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Foam Threshold", 0.0, 1.0, 0.0),
                &mut self.foam_threshold as *mut f32,
                "Foam Threshold",
                Some("Threshold for generating foam."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Foam Advection", 0.0, 2.0, 0.0),
                &mut self.foam_advection_strength as *mut f32,
                "Foam Advection",
                Some("Controls how foam advects across the surface."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Foam Decay", 0.0, 1.0, 0.0),
                &mut self.foam_decay_rate as *mut f32,
                "Foam Decay",
                Some("Rate at which foam fades over time."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Foam Noise Scale", 0.01, 1.0, 0.0),
                &mut self.foam_noise_scale as *mut f32,
                "Foam Noise Scale",
                Some("Noise scale applied to foam patterns."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Capillary Strength", 0.0, 2.0, 0.0),
                &mut self.capillary_strength as *mut f32,
                "Capillary Strength",
                Some("Strength of small capillary waves."),
            );
            debug_register_with_description(
                PageType::Ocean,
                Slider::new(0, "Time Scale", 0.1, 4.0, 0.0),
                &mut self.time_scale as *mut f32,
                "Time Scale",
                Some("Scales the simulation time for waves."),
            );

            let mut ocean_shadow_cascades =
                Slider::new_int(0, "Ocean Shadow Cascades", 0.0, 4.0, 0.0);
            ocean_shadow_cascades.enabled = false;
            debug_register_int_with_description(
                PageType::Shadow,
                ocean_shadow_cascades,
                &mut self.shadow_cascade_count as *mut u32,
                "Ocean Shadow Cascades",
                Some("Read-only cascade count used by ocean shadow sampling."),
            );
            let mut ocean_shadow_resolution =
                Slider::new_int(0, "Ocean Shadow Resolution", 0.0, 4096.0, 0.0);
            ocean_shadow_resolution.enabled = false;
            debug_register_int_with_description(
                PageType::Shadow,
                ocean_shadow_resolution,
                &mut self.shadow_resolution as *mut u32,
                "Ocean Shadow Resolution",
                Some("Read-only resolution used by ocean shadow sampling."),
            );
            let mut ocean_shadow_split0 =
                Slider::new(0, "Ocean Shadow Split 0", 0.0, 200000.0, 0.0);
            ocean_shadow_split0.enabled = false;
            debug_register_with_description(
                PageType::Shadow,
                ocean_shadow_split0,
                &mut self.shadow_splits.x as *mut f32,
                "Ocean Shadow Split 0",
                Some("Read-only split distance for the nearest ocean shadow cascade."),
            );
            let mut ocean_shadow_split1 =
                Slider::new(0, "Ocean Shadow Split 1", 0.0, 200000.0, 0.0);
            ocean_shadow_split1.enabled = false;
            debug_register_with_description(
                PageType::Shadow,
                ocean_shadow_split1,
                &mut self.shadow_splits.y as *mut f32,
                "Ocean Shadow Split 1",
                Some("Read-only split distance for the second ocean shadow cascade."),
            );
            let mut ocean_shadow_split2 =
                Slider::new(0, "Ocean Shadow Split 2", 0.0, 200000.0, 0.0);
            ocean_shadow_split2.enabled = false;
            debug_register_with_description(
                PageType::Shadow,
                ocean_shadow_split2,
                &mut self.shadow_splits.z as *mut f32,
                "Ocean Shadow Split 2",
                Some("Read-only split distance for the third ocean shadow cascade."),
            );
            let mut ocean_shadow_split3 =
                Slider::new(0, "Ocean Shadow Split 3", 0.0, 200000.0, 0.0);
            ocean_shadow_split3.enabled = false;
            debug_register_with_description(
                PageType::Shadow,
                ocean_shadow_split3,
                &mut self.shadow_splits.w as *mut f32,
                "Ocean Shadow Split 3",
                Some("Read-only split distance for the furthest ocean shadow cascade."),
            );
        }
    }

    pub fn set_environment_map(&mut self, view: ImageView) {
        self.pipeline.update_table(
            "ocean_env_map",
            dashi::IndexedResource {
                resource: ShaderResource::Image(view),
                slot: 0,
            },
        );
        self.pipeline.update_table(
            "ocean_env_sampler",
            dashi::IndexedResource {
                resource: ShaderResource::Sampler(self.environment_sampler),
                slot: 0,
            },
        );
    }

    pub fn set_scene_textures(&mut self, color: Option<ImageView>, depth: Option<ImageView>) {
        let color_view = color.unwrap_or(self.scene_color_fallback);
        let depth_view = depth.unwrap_or(self.scene_depth_fallback);
        self.pipeline.update_table(
            "ocean_scene_color",
            dashi::IndexedResource {
                resource: ShaderResource::Image(color_view),
                slot: 0,
            },
        );
        self.pipeline.update_table(
            "ocean_scene_depth",
            dashi::IndexedResource {
                resource: ShaderResource::Image(depth_view),
                slot: 0,
            },
        );
        self.pipeline.update_table(
            "ocean_scene_sampler",
            dashi::IndexedResource {
                resource: ShaderResource::Sampler(self.scene_sampler),
                slot: 0,
            },
        );
    }

    pub fn set_shadow_map(
        &mut self,
        shadow_map: Option<ImageView>,
        cascade_count: u32,
        resolution: u32,
        splits: Vec4,
        matrices: [Mat4; 4],
    ) {
        let shadow_view = shadow_map.unwrap_or(self.scene_depth_fallback);
        self.pipeline.update_table(
            "ocean_shadow_map",
            dashi::IndexedResource {
                resource: ShaderResource::Image(shadow_view),
                slot: 0,
            },
        );
        self.pipeline.update_table(
            "ocean_shadow_sampler",
            dashi::IndexedResource {
                resource: ShaderResource::Sampler(self.scene_sampler),
                slot: 0,
            },
        );
        self.shadow_cascade_count = cascade_count;
        self.shadow_resolution = resolution;
        self.shadow_splits = splits;
        self.shadow_matrices = matrices;
    }

    pub fn record_compute(
        &mut self,
        dynamic: &mut DynamicAllocator,
        time: f32,
    ) -> CommandStream<Executable> {
        if !self.enabled {
            return CommandStream::new().begin().end();
        }

        // Per-cascade FFT pipeline:
        // 1) Build spectrum parameters and generate the frequency-domain spectrum.
        // 2) Run horizontal and vertical FFT passes with bit-reversal.
        // 3) Finalize the spatial-domain output into the packed wave buffer.
        let mut stream = CommandStream::new().begin();
        for (cascade_index, cascade) in self.cascades.iter().enumerate() {
            let Some(spectrum_pipeline) = cascade.spectrum_pipeline.as_ref() else {
                continue;
            };
            let Some(fft_spectrum_to_ping) = cascade.fft_spectrum_to_ping.as_ref() else {
                continue;
            };
            let Some(fft_ping_to_pong) = cascade.fft_ping_to_pong.as_ref() else {
                continue;
            };
            let Some(fft_pong_to_ping) = cascade.fft_pong_to_ping.as_ref() else {
                continue;
            };
            let Some(finalize_from_ping) = cascade.finalize_from_ping.as_ref() else {
                continue;
            };
            let Some(finalize_from_pong) = cascade.finalize_from_pong.as_ref() else {
                continue;
            };

            let mut spectrum_alloc = dynamic
                .bump()
                .expect("Failed to allocate ocean spectrum params");
            let spectrum_params = &mut spectrum_alloc.slice::<OceanSpectrumParams>()[0];
            // Per-cascade spectrum scaling to balance amplitudes across FFT sizes.
            let spectrum_scale = self
                .cascade_spectrum_scales
                .get(cascade_index)
                .copied()
                .unwrap_or(1.0);
            let swell_strength = self
                .cascade_swell_strengths
                .get(cascade_index)
                .copied()
                .unwrap_or(0.0);
            *spectrum_params = OceanSpectrumParams {
                fft_size: cascade.fft_size,
                time,
                time_scale: self.time_scale,
                wave_amplitude: self.wave_amplitude,
                wind_dir: self.wind_dir,
                wind_speed: self.wind_speed,
                patch_size: cascade.patch_size,
                spectrum_scale,
                fetch_length: self.fetch_length,
                swell_dir: self.swell_dir,
                swell_strength,
            };

            // Spectrum synthesis into the cascade spectrum buffer.
            stream = stream
                .prepare_buffer(cascade.spectrum_buffer, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(cascade.ping_buffer, UsageBits::COMPUTE_SHADER)
                .dispatch(&Dispatch {
                    x: (cascade.fft_size + 7) / 8,
                    y: (cascade.fft_size + 7) / 8,
                    z: 1,
                    pipeline: spectrum_pipeline.handle,
                    bind_tables: spectrum_pipeline.tables(),
                    dynamic_buffers: [None, Some(spectrum_alloc), None, None],
                })
                .unbind_pipeline();

            // FFT setup: first stage does bit-reversal, followed by log2(N) passes.
            let log_n = cascade.fft_size.trailing_zeros();
            let mut current_is_ping = true;

            let mut fft_alloc = dynamic.bump().expect("Failed to allocate ocean FFT params");
            let fft_params = &mut fft_alloc.slice::<OceanFftParams>()[0];
            *fft_params = OceanFftParams {
                fft_size: cascade.fft_size,
                stage: 0,
                direction: 0,
                bit_reverse: 1,
                inverse: 1.0,
                _padding: [0.0; 3],
            };

            // Initial FFT step (bit-reversal + horizontal pass into ping buffer).
            stream = stream
                .prepare_buffer(cascade.spectrum_buffer, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(cascade.ping_buffer, UsageBits::COMPUTE_SHADER)
                .dispatch(&Dispatch {
                    x: (cascade.fft_size + 7) / 8,
                    y: (cascade.fft_size + 7) / 8,
                    z: 1,
                    pipeline: fft_spectrum_to_ping.handle,
                    bind_tables: fft_spectrum_to_ping.tables(),
                    dynamic_buffers: [None, Some(fft_alloc), None, None],
                })
                .unbind_pipeline();

            // Horizontal FFT passes.
            for stage in 0..log_n {
                let mut pass_alloc = dynamic.bump().expect("Failed to allocate ocean FFT params");
                let pass_params = &mut pass_alloc.slice::<OceanFftParams>()[0];
                *pass_params = OceanFftParams {
                    fft_size: cascade.fft_size,
                    stage,
                    direction: 0,
                    bit_reverse: 0,
                    inverse: 1.0,
                    _padding: [0.0; 3],
                };
                let pipeline = if current_is_ping {
                    fft_ping_to_pong
                } else {
                    fft_pong_to_ping
                };
                let (input, output) = if current_is_ping {
                    (cascade.ping_buffer, cascade.pong_buffer)
                } else {
                    (cascade.pong_buffer, cascade.ping_buffer)
                };
                stream = stream
                    .prepare_buffer(input, UsageBits::COMPUTE_SHADER)
                    .prepare_buffer(output, UsageBits::COMPUTE_SHADER)
                    .dispatch(&Dispatch {
                        x: (cascade.fft_size + 7) / 8,
                        y: (cascade.fft_size + 7) / 8,
                        z: 1,
                        pipeline: pipeline.handle,
                        bind_tables: pipeline.tables(),
                        dynamic_buffers: [None, Some(pass_alloc), None, None],
                    })
                    .unbind_pipeline();
                current_is_ping = !current_is_ping;
            }

            // Bit-reversal for the vertical FFT.
            let mut bitrev_alloc = dynamic.bump().expect("Failed to allocate ocean FFT params");
            let bitrev_params = &mut bitrev_alloc.slice::<OceanFftParams>()[0];
            *bitrev_params = OceanFftParams {
                fft_size: cascade.fft_size,
                stage: 0,
                direction: 1,
                bit_reverse: 1,
                inverse: 1.0,
                _padding: [0.0; 3],
            };
            let bitrev_pipeline = if current_is_ping {
                fft_ping_to_pong
            } else {
                fft_pong_to_ping
            };
            let (bitrev_input, bitrev_output) = if current_is_ping {
                (cascade.ping_buffer, cascade.pong_buffer)
            } else {
                (cascade.pong_buffer, cascade.ping_buffer)
            };
            stream = stream
                .prepare_buffer(bitrev_input, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(bitrev_output, UsageBits::COMPUTE_SHADER)
                .dispatch(&Dispatch {
                    x: (cascade.fft_size + 7) / 8,
                    y: (cascade.fft_size + 7) / 8,
                    z: 1,
                    pipeline: bitrev_pipeline.handle,
                    bind_tables: bitrev_pipeline.tables(),
                    dynamic_buffers: [None, Some(bitrev_alloc), None, None],
                })
                .unbind_pipeline();
            current_is_ping = !current_is_ping;

            // Vertical FFT passes.
            for stage in 0..log_n {
                let mut pass_alloc = dynamic.bump().expect("Failed to allocate ocean FFT params");
                let pass_params = &mut pass_alloc.slice::<OceanFftParams>()[0];
                *pass_params = OceanFftParams {
                    fft_size: cascade.fft_size,
                    stage,
                    direction: 1,
                    bit_reverse: 0,
                    inverse: 1.0,
                    _padding: [0.0; 3],
                };
                let pipeline = if current_is_ping {
                    fft_ping_to_pong
                } else {
                    fft_pong_to_ping
                };
                let (input, output) = if current_is_ping {
                    (cascade.ping_buffer, cascade.pong_buffer)
                } else {
                    (cascade.pong_buffer, cascade.ping_buffer)
                };
                stream = stream
                    .prepare_buffer(input, UsageBits::COMPUTE_SHADER)
                    .prepare_buffer(output, UsageBits::COMPUTE_SHADER)
                    .dispatch(&Dispatch {
                        x: (cascade.fft_size + 7) / 8,
                        y: (cascade.fft_size + 7) / 8,
                        z: 1,
                        pipeline: pipeline.handle,
                        bind_tables: pipeline.tables(),
                        dynamic_buffers: [None, Some(pass_alloc), None, None],
                    })
                    .unbind_pipeline();
                current_is_ping = !current_is_ping;
            }

            // Finalize spatial-domain waves into the packed wave buffer.
            let mut finalize_alloc = dynamic
                .bump()
                .expect("Failed to allocate ocean finalize params");
            let finalize_params = &mut finalize_alloc.slice::<OceanFinalizeParams>()[0];
            *finalize_params = OceanFinalizeParams {
                fft_size: cascade.fft_size,
                _padding: [0; 3],
            };
            let finalize_pipeline = if current_is_ping {
                finalize_from_ping
            } else {
                finalize_from_pong
            };
            let finalize_input = if current_is_ping {
                cascade.ping_buffer
            } else {
                cascade.pong_buffer
            };
            stream = stream
                .prepare_buffer(finalize_input, UsageBits::COMPUTE_SHADER)
                .prepare_buffer(cascade.wave_buffer, UsageBits::COMPUTE_SHADER)
                .dispatch(&Dispatch {
                    x: (cascade.fft_size + 7) / 8,
                    y: (cascade.fft_size + 7) / 8,
                    z: 1,
                    pipeline: finalize_pipeline.handle,
                    bind_tables: finalize_pipeline.tables(),
                    dynamic_buffers: [None, Some(finalize_alloc), None, None],
                })
                .unbind_pipeline();
        }

        stream.end()
    }

    pub fn record_draws(
        &mut self,
        viewport: &Viewport,
        dynamic: &mut DynamicAllocator,
        camera: Handle<Camera>,
        time: f32,
    ) -> CommandStream<PendingGraphics> {
        if !self.enabled {
            return CommandStream::subdraw();
        }

        // Populate draw parameters consumed by the ocean vertex/fragment shaders.
        let mut alloc = dynamic
            .bump()
            .expect("Failed to allocate ocean draw params");
            
        assert_eq!(std::mem::size_of::<OceanDrawParams>(),  252);
        let params = &mut alloc.slice::<OceanDrawParams>()[0];
        let mut cascade_fft_sizes = [0u32; 4];
        let mut cascade_patch_sizes = [0.0f32; 4];
        // Collect per-cascade FFT and patch sizes for sampling and blend distances.
        for (index, cascade) in self.cascades.iter().enumerate() {
            cascade_fft_sizes[index] = cascade.fft_size;
            cascade_patch_sizes[index] = cascade.patch_size;
        }
        // Blend ranges determine how quickly we transition between cascades by distance.
        let blend_ranges = [
            cascade_patch_sizes[0] * 1.0,
            cascade_patch_sizes[1] * 32.0,
            cascade_patch_sizes[2] * 64.0,
            0.0,
        ];

        // Copy the current frame settings into the GPU draw parameter block.
        *params = OceanDrawParams {
            cascade_fft_sizes,
            cascade_patch_sizes,
            cascade_blend_ranges: blend_ranges,
            vertex_resolution: self.vertex_resolution,
            camera_index: camera.slot as u32,
            base_tile_radius: self.base_tile_radius,
            max_tile_radius: self.max_tile_radius,
            far_tile_radius: self.far_tile_radius,
            tile_height_step: self.tile_height_step,
            endless: self.endless as u32,
            time,
            wind_dir: self.wind_dir,
            wind_speed: self.wind_speed,
            gerstner_amplitude: self.gerstner_amplitude,
            fresnel_bias: self.fresnel_bias,
            fresnel_strength: self.fresnel_strength,
            foam_strength: self.foam_strength,
            foam_threshold: self.foam_threshold,
            foam_advection_strength: self.foam_advection_strength,
            foam_decay_rate: self.foam_decay_rate,
            foam_noise_scale: self.foam_noise_scale,
            current: self.current,
            _padding1: Default::default(),
            absorption_coeff: Vec4::new(
                self.absorption_coeff.x,
                self.absorption_coeff.y,
                self.absorption_coeff.z,
                0.0,
            ),
            shallow_color: Vec4::new(
                self.shallow_color.x,
                self.shallow_color.y,
                self.shallow_color.z,
                0.0,
            ),
            deep_color: Vec4::new(self.deep_color.x, self.deep_color.y, self.deep_color.z, 0.0),
            scattering_color: Vec4::new(
                self.scattering_color.x,
                self.scattering_color.y,
                self.scattering_color.z,
                0.0,
            ),
            scattering_strength: self.scattering_strength,
            turbidity_depth: self.turbidity_depth,
            refraction_strength: self.refraction_strength,
            ssr_strength: self.ssr_strength,
            ssr_max_distance: self.ssr_max_distance,
            ssr_thickness: self.ssr_thickness,
            ssr_steps: self.ssr_steps,
            debug_view: self.debug_view as u32 as f32,
            _padding2: Default::default(),
        };
        let mut shadow_alloc = dynamic
            .bump()
            .expect("Failed to allocate ocean shadow params");
        let mut shadow_matrix_alloc = dynamic
            .bump()
            .expect("Failed to allocate ocean shadow params");

        shadow_alloc.slice::<OceanShadowParams>()[0] = OceanShadowParams {
            shadow_cascade_count: self.shadow_cascade_count,
            shadow_resolution: self.shadow_resolution,
            shadow_padding0: 0,
            shadow_padding1: 0,
            shadow_splits: self.shadow_splits,
        };

        shadow_matrix_alloc.slice::<OceanShadowMatrices>()[0] = OceanShadowMatrices {
            shadow_matrices: self.shadow_matrices,
        };

        
        let grid_resolution = self.vertex_resolution.max(2);
        let quad_count = (grid_resolution - 1) * (grid_resolution - 1);
        let vertex_count = quad_count * 6;
        let max_tile_count = self.max_tile_radius.max(1) * 2 + 1;
        let instance_count = max_tile_count * max_tile_count;

        // TODO: Evaluate meshlet-based culling/LOD once mesh/cluster rendering is available.
        CommandStream::<PendingGraphics>::subdraw()
            .bind_graphics_pipeline(self.pipeline.handle)
            .update_viewport(viewport)
            .draw(&Draw {
                bind_tables: self.pipeline.tables(),
                dynamic_buffers: [Some(alloc), Some(shadow_alloc), Some(shadow_matrix_alloc), None],
                instance_count,
                count: vertex_count,
                ..Default::default()
            })
            .unbind_graphics_pipeline()
    }
}
