mod cloud;

use super::EnvironmentRendererInfo;
use bento::builder::{AttachmentDesc, PSO, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use cloud::CloudSimulation;
use dashi::cmd::{Executable, PendingGraphics};
use dashi::driver::command::{BlitImage, Draw};
use dashi::structs::*;
use dashi::*;
use dashi::{
    AspectMask, CommandStream, DynamicAllocator, Format, ImageView, ImageViewType, Sampler,
    SamplerInfo, ShaderResource, SubresourceRange, Viewport,
};
use furikake::PSOBuilderFurikakeExt;
use furikake::{
    BindlessState, reservations::bindless_camera::ReservedBindlessCamera, types::Camera,
};
use glam::*;
use noren::rdb::imagery::{GPUImageInfo, HostCubemap, ImageInfo as NorenImageInfo};
use tare::utils::StagedBuffer;
use tracing::warn;

use crate::gui::debug::{debug_register, PageType};
use crate::gui::Slider;
#[derive(Clone)]
pub struct SkyboxInfo {
    pub cubemap: Option<noren::rdb::imagery::DeviceCubemap>,
    pub intensity: f32,
    pub use_procedural_cubemap: bool,
    pub update_interval_frames: u32,
}

#[derive(Clone)]
pub struct SkyboxFrameSettings {
    pub intensity: f32,
    pub cubemap: Option<noren::rdb::imagery::DeviceCubemap>,
    pub use_procedural_cubemap: bool,
    pub update_interval_frames: u32,
}

#[derive(Clone, Debug)]
pub struct SkyFrameSettings {
    pub enabled: bool,
    pub sun_direction: Option<Vec3>,
    pub sun_color: Vec3,
    pub sun_intensity: f32,
    pub sun_angular_radius: f32,
    pub moon_direction: Option<Vec3>,
    pub moon_color: Vec3,
    pub moon_intensity: f32,
    pub moon_angular_radius: f32,
    pub time_of_day: Option<f32>,
    pub latitude_degrees: Option<f32>,
    pub longitude_degrees: Option<f32>,
}

impl Default for SkyboxInfo {
    fn default() -> Self {
        Self {
            cubemap: None,
            intensity: 1.0,
            use_procedural_cubemap: true,
            update_interval_frames: 1,
        }
    }
}

impl Default for SkyboxFrameSettings {
    fn default() -> Self {
        let mut settings = Self {
            cubemap: None,
            intensity: 1.0,
            use_procedural_cubemap: true,
            update_interval_frames: 1,
        };
        settings.register_debug();
        settings
    }
}

impl Default for SkyFrameSettings {
    fn default() -> Self {
        let mut settings = Self {
            enabled: false,
            sun_direction: Some(Vec3::Y),
            sun_color: Vec3::ONE,
            sun_intensity: 1.0,
            sun_angular_radius: 0.0045,
            moon_direction: Some(-Vec3::Y),
            moon_color: Vec3::ONE,
            moon_intensity: 0.1,
            moon_angular_radius: 0.0045,
            time_of_day: None,
            latitude_degrees: None,
            longitude_degrees: None,
        };
        settings.register_debug();
        settings
    }
}

impl SkyboxFrameSettings {
    pub fn register_debug(&mut self) {
        unsafe {
            debug_register(
                PageType::Sky,
                Slider::new(0, "Skybox Intensity", 0.2, 2.0, 0.0),
                &mut self.intensity as *mut f32,
                "Skybox Intensity",
            );
        }
    }
}

impl SkyFrameSettings {
    pub fn register_debug(&mut self) {
        unsafe {
            debug_register(
                PageType::Sky,
                Slider::new(0, "Sun Intensity", 0.1, 5.0, 0.0),
                &mut self.sun_intensity as *mut f32,
                "Sun Intensity",
            );
            debug_register(
                PageType::Sky,
                Slider::new(0, "Sun Angular Radius", 0.001, 0.05, 0.0),
                &mut self.sun_angular_radius as *mut f32,
                "Sun Angular Radius",
            );
            debug_register(
                PageType::Sky,
                Slider::new(0, "Moon Intensity", 0.0, 2.0, 0.0),
                &mut self.moon_intensity as *mut f32,
                "Moon Intensity",
            );
            debug_register(
                PageType::Sky,
                Slider::new(0, "Moon Angular Radius", 0.001, 0.05, 0.0),
                &mut self.moon_angular_radius as *mut f32,
                "Moon Angular Radius",
            );
        }
    }
}

#[repr(C)]
#[derive(Default)]
struct SkyConfig {
    horizon_init: Vec3,
    intensity_scale: f32,
    zenith_tint: Vec3,
    _padding: f32,
    sun_dir: Vec3,
    sun_intensity: f32,
    sun_color: Vec3,
    sun_angular_radius: f32,
    moon_dir: Vec3,
    moon_intensity: f32,
    moon_color: Vec3,
    moon_angular_radius: f32,
}

#[repr(C)]
struct SkyboxParams {
    camera_index: u32,
    intensity: f32,
    _padding: [f32; 2],
}

#[repr(C)]
struct SkyDrawParams {
    camera_index: u32,
    _padding: [u32; 3],
}

pub struct SkyCubemapPass {
    pub viewport: Viewport,
    pub face_views: [ImageView; 6],
}

pub struct SkyRenderer {
    pipeline: PSO,
    skybox_pipeline: PSO,
    skybox_sampler: Handle<Sampler>,
    skybox_fallback_view: ImageView,
    skybox_swap_info: Option<GPUImageInfo>,
    skybox_intensity: f32,
    use_procedural_cubemap: bool,
    cubemap_update_interval: u32,
    cubemap_frame_index: u32,
    cubemap_dirty: bool,
    cubemap_size: u32,
    cubemap_viewport: Viewport,
    cubemap_face_views: Option<[ImageView; 6]>,
    cubemap_camera_handles: Option<[Handle<Camera>; 6]>,
    procedural_cubemap: Option<noren::rdb::imagery::DeviceCubemap>,
    pending_cubemap_swap: Option<noren::rdb::imagery::DeviceCubemap>,
    cubemap_format: Format,
    sky_settings: SkyFrameSettings,
    clouds: CloudSimulation,
    cfg: StagedBuffer,
    enabled: bool,
}

fn compile_skybox_shaders() -> [bento::CompilationResult; 2] {
    let compiler = Compiler::new().expect("Failed to create shader compiler");
    let base_request = Request {
        name: Some("skybox".to_string()),
        lang: ShaderLang::Slang,
        optimization: OptimizationLevel::Performance,
        debug_symbols: true,
        ..Default::default()
    };

    let vertex = compiler
        .compile(
            include_str!("shaders/skybox_vert.slang").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Vertex,
                ..base_request.clone()
            },
        )
        .expect("Failed to compile skybox vertex shader");

    let fragment = compiler
        .compile(
            include_str!("shaders/skybox_frag.slang").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Fragment,
                ..base_request
            },
        )
        .expect("Failed to compile skybox fragment shader");

    [vertex, fragment]
}

fn compile_sky_shaders() -> [bento::CompilationResult; 2] {
    let compiler = Compiler::new().expect("Failed to create shader compiler");
    let base_request = Request {
        name: Some("sky".to_string()),
        lang: ShaderLang::Slang,
        optimization: OptimizationLevel::Performance,
        debug_symbols: true,
        ..Default::default()
    };

    let vertex = compiler
        .compile(
            include_str!("shaders/sky_vert.slang").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Vertex,
                ..base_request.clone()
            },
        )
        .expect("Failed to compile sky vertex shader");

    let fragment = compiler
        .compile(
            include_str!("shaders/sky_frag.slang").as_bytes(),
            &Request {
                stage: dashi::ShaderType::Fragment,
                ..base_request
            },
        )
        .expect("Failed to compile sky fragment shader");

    [vertex, fragment]
}

fn default_skybox_view(ctx: &mut dashi::Context) -> ImageView {
    let face = vec![135, 206, 235, 255];
    let faces = [
        face.clone(),
        face.clone(),
        face.clone(),
        face.clone(),
        face.clone(),
        face,
    ];

    let info = NorenImageInfo {
        name: "[MESHI GFX SKY] Default Skybox".to_string(),
        dim: [1, 1, 1],
        layers: 6,
        format: Format::RGBA8,
        mip_levels: 1,
    };

    let cubemap = HostCubemap::from_faces(info, faces).expect("create default skybox cubemap");
    let mut dashi_info = cubemap.info.dashi_cube();
    dashi_info.initial_data = Some(cubemap.data());

    let image = ctx
        .make_image(&dashi_info)
        .expect("Failed to create default skybox image");

    ImageView {
        img: image,
        aspect: AspectMask::Color,
        view_type: ImageViewType::Cube,
        range: SubresourceRange::new(0, cubemap.info.mip_levels, 0, 6),
    }
}

fn create_skybox_swap_view(ctx: &mut dashi::Context, info: &GPUImageInfo) -> ImageView {
    let image_info = NorenImageInfo {
        name: "[MESHI GFX SKY] Skybox Swap".to_string(),
        dim: info.dim,
        layers: info.layers,
        format: info.format,
        mip_levels: info.mip_levels,
    };

    let mut dashi_info = image_info.dashi_cube();
    dashi_info.initial_data = None;

    let image = ctx
        .make_image(&dashi_info)
        .expect("Failed to create skybox swap image");

    ImageView {
        img: image,
        aspect: AspectMask::Color,
        view_type: ImageViewType::Cube,
        range: SubresourceRange::new(0, info.mip_levels, 0, info.layers),
    }
}

impl SkyRenderer {
    pub fn new(
        ctx: &mut dashi::Context,
        state: &mut BindlessState,
        info: &EnvironmentRendererInfo,
        dynamic: &DynamicAllocator,
    ) -> Self {
        let clouds = CloudSimulation::new(ctx);
        let shaders = compile_sky_shaders();
        let skybox_shaders = compile_skybox_shaders();

        let (skybox_view, skybox_swap_info) = if let Some(cubemap) = info.skybox.cubemap.as_ref() {
            (
                create_skybox_swap_view(ctx, &cubemap.info),
                Some(cubemap.info.clone()),
            )
        } else {
            (
                default_skybox_view(ctx),
                Some(GPUImageInfo {
                    dim: [1, 1, 1],
                    layers: 6,
                    format: Format::RGBA8,
                    mip_levels: 1,
                }),
            )
        };
        let skybox_sampler = ctx
            .make_sampler(&SamplerInfo::default())
            .expect("Failed to create skybox sampler");

        let initial_config = [SkyConfig {
            ..Default::default()
        }];

        let cfg = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[MESHI GFX SKY] Configuration",
                byte_size: (std::mem::size_of::<SkyConfig>() as u32),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::UNIFORM,
                initial_data: unsafe { Some(&initial_config.align_to::<u8>().1) },
            },
        );

        let mut pso_builder = PSOBuilder::new()
            .vertex_compiled(Some(shaders[0].clone()))
            .fragment_compiled(Some(shaders[1].clone()))
            .set_attachment_format(0, info.color_format)
            .add_table_variable_with_resources(
                "sky_draw_ssbo",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::DynamicStorage(dynamic.state()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "SkyParams",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::StorageBuffer(cfg.device()),
                    slot: 0,
                }],
            );

        pso_builder = pso_builder.add_reserved_table_variables(state).unwrap();

        if info.use_depth {
            pso_builder = pso_builder.add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: info.sample_count,
            });
        }

        let sky_depth_test = if info.use_depth {
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
                depth_test: sky_depth_test,
                ..Default::default()
            })
            .build(ctx)
            .expect("Failed to build sky PSO");

        state.register_pso_tables(&pipeline);

        let mut skybox_builder = PSOBuilder::new()
            .vertex_compiled(Some(skybox_shaders[0].clone()))
            .fragment_compiled(Some(skybox_shaders[1].clone()))
            .set_attachment_format(0, info.color_format)
            .add_table_variable_with_resources(
                "skybox_texture",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::Image(skybox_view),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "skybox_sampler",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::Sampler(skybox_sampler),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "skybox_params",
                vec![dashi::IndexedResource {
                    resource: ShaderResource::DynamicStorage(dynamic.state()),
                    slot: 0,
                }],
            );

        skybox_builder = skybox_builder
            .add_reserved_table_variable(state, "meshi_bindless_cameras")
            .unwrap();

        if info.use_depth {
            skybox_builder = skybox_builder.add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: info.sample_count,
            });
        }

        let skybox_depth_test = if info.use_depth {
            Some(dashi::DepthInfo {
                should_test: true,
                should_write: false,
            })
        } else {
            None
        };

        let skybox_pipeline = skybox_builder
            .set_details(dashi::GraphicsPipelineDetails {
                color_blend_states: vec![Default::default(); 1],
                sample_count: info.sample_count,
                depth_test: skybox_depth_test,
                ..Default::default()
            })
            .build(ctx)
            .expect("Failed to build skybox PSO");

        state.register_pso_tables(&skybox_pipeline);

        Self {
            pipeline,
            skybox_pipeline,
            skybox_sampler,
            skybox_fallback_view: skybox_view,
            skybox_swap_info,
            skybox_intensity: info.skybox.intensity,
            use_procedural_cubemap: info.skybox.use_procedural_cubemap,
            cubemap_update_interval: info.skybox.update_interval_frames.max(1),
            cubemap_frame_index: 0,
            cubemap_dirty: true,
            cubemap_size: 0,
            cubemap_viewport: Viewport::default(),
            cubemap_face_views: None,
            cubemap_camera_handles: None,
            procedural_cubemap: None,
            pending_cubemap_swap: info.skybox.cubemap.clone(),
            cubemap_format: info.color_format,
            sky_settings: SkyFrameSettings::default(),
            clouds,
            cfg,
            enabled: true,
        }
    }

    pub fn update_skybox(&mut self, settings: SkyboxFrameSettings) {
        self.skybox_intensity = settings.intensity;
        let procedural_changed = self.use_procedural_cubemap != settings.use_procedural_cubemap;
        self.use_procedural_cubemap = settings.use_procedural_cubemap;
        self.cubemap_update_interval = settings.update_interval_frames.max(1);

        if let Some(cubemap) = settings.cubemap {
            self.pending_cubemap_swap = Some(cubemap);
        }

        if procedural_changed {
            self.cubemap_dirty = true;
        }

        self.apply_skybox_binding();
    }

    pub fn update_sky(&mut self, settings: SkyFrameSettings) {
        self.enabled = settings.enabled;
        self.sky_settings = settings;
        self.cubemap_dirty = true;
    }

    pub fn sun_direction(&self) -> Vec3 {
        resolve_celestial_direction(
            self.sky_settings.sun_direction,
            self.sky_settings.time_of_day,
            self.sky_settings.latitude_degrees,
            self.sky_settings.longitude_degrees,
            false,
        )
    }

    pub fn environment_cubemap_view(&self) -> ImageView {
        if self.use_procedural_cubemap {
            self.procedural_cubemap
                .as_ref()
                .map(|cubemap| cubemap.view)
                .unwrap_or(self.skybox_fallback_view)
        } else {
            self.skybox_fallback_view
        }
    }

    pub fn prepare_cubemap_pass(
        &mut self,
        ctx: &mut dashi::Context,
        state: &mut BindlessState,
        viewport: &Viewport,
        camera: dashi::Handle<Camera>,
    ) -> Option<SkyCubemapPass> {
        if !self.enabled {
            return None;
        }

        if !self.use_procedural_cubemap {
            self.apply_skybox_binding();
            return None;
        }

        let size = cubemap_size_from_viewport(viewport);
        let recreated = self.ensure_cubemap_resources(ctx, size);

        if recreated {
            self.cubemap_dirty = true;
        }

        self.cubemap_frame_index = self.cubemap_frame_index.wrapping_add(1);
        let interval_ready = self.cubemap_frame_index % self.cubemap_update_interval == 0;
        let should_render = self.cubemap_dirty || interval_ready;

        if !should_render {
            return None;
        }

        self.update_cubemap_cameras(state, camera, size)?;
        self.cubemap_dirty = false;

        Some(SkyCubemapPass {
            viewport: self.cubemap_viewport,
            face_views: self.cubemap_face_views?,
        })
    }

    pub fn record_cubemap_face(
        &mut self,
        viewport: &Viewport,
        dynamic: &mut DynamicAllocator,
        face_index: usize,
    ) -> CommandStream<PendingGraphics> {
        self.update_sky_config();

        let mut alloc = dynamic.bump().expect("Failed to allocate sky draw params");

        let params = &mut alloc.slice::<SkyDrawParams>()[0];
        params.camera_index = self
            .cubemap_camera_handles
            .and_then(|handles| handles.get(face_index).copied())
            .map(|handle| handle.slot as u32)
            .unwrap_or_default();
        params._padding = [0; 3];

        CommandStream::<PendingGraphics>::subdraw()
            .combine(self.cfg.sync_up())
            .bind_graphics_pipeline(self.pipeline.handle)
            .update_viewport(viewport)
            .draw(&Draw {
                bind_tables: self.pipeline.tables(),
                dynamic_buffers: [None, Some(alloc), None, None],
                instance_count: 1,
                count: 3,
                ..Default::default()
            })
            .unbind_graphics_pipeline()
    }

    pub fn record_draws(
        &mut self,
        viewport: &Viewport,
        dynamic: &mut DynamicAllocator,
        camera: dashi::Handle<Camera>,
        time: f32,
        delta_time: f32,
    ) -> CommandStream<PendingGraphics> {
        if !self.enabled {
            return CommandStream::subdraw();
        }

        if self.use_procedural_cubemap {
            self.update_sky_config();

            let mut alloc = dynamic
                .bump()
                .expect("Failed to allocate sky dynamic buffer");

            let params = &mut alloc.slice::<SkyDrawParams>()[0];
            params.camera_index = camera.slot as u32;
            params._padding = [0; 3];

            CommandStream::<PendingGraphics>::subdraw()
                .combine(self.cfg.sync_up())
                .bind_graphics_pipeline(self.pipeline.handle)
                .update_viewport(viewport)
                .draw(&Draw {
                    bind_tables: self.pipeline.tables(),
                    dynamic_buffers: [None, Some(alloc), None, None],
                    instance_count: 1,
                    count: 3,
                    ..Default::default()
                })
                .unbind_graphics_pipeline()
        } else {
            self.apply_skybox_binding();

            let mut alloc = dynamic
                .bump()
                .expect("Failed to allocate sky dynamic buffer");

            let params = &mut alloc.slice::<SkyboxParams>()[0];
            params.camera_index = camera.slot as u32;
            params.intensity = self.skybox_intensity;
            params._padding = [0.0; 2];

            CommandStream::<PendingGraphics>::subdraw()
                .combine(self.cfg.sync_up())
                .bind_graphics_pipeline(self.skybox_pipeline.handle)
                .update_viewport(viewport)
                .draw(&Draw {
                    bind_tables: self.skybox_pipeline.tables(),
                    dynamic_buffers: [None, Some(alloc), None, None],
                    instance_count: 1,
                    count: 3,
                    ..Default::default()
                })
                .unbind_graphics_pipeline()
        }
    }

    pub fn record_compute(
        &mut self,
        ctx: &mut dashi::Context,
        time: f32,
        delta_time: f32,
    ) -> CommandStream<Executable> {
        let mut stream = CommandStream::new().begin();

        if let Some(cubemap) = self.pending_cubemap_swap.take() {
            let target_info = self.ensure_skybox_swap_target(ctx, &cubemap.info);
            let src_range =
                SubresourceRange::new(0, cubemap.info.mip_levels, 0, cubemap.info.layers);
            let dst_range = SubresourceRange::new(0, target_info.mip_levels, 0, target_info.layers);

            stream = stream.blit_images(&BlitImage {
                src: cubemap.view.img,
                dst: self.skybox_fallback_view.img,
                src_range,
                dst_range,
                filter: Filter::Linear,
                src_region: Rect2D {
                    x: 0,
                    y: 0,
                    w: cubemap.info.dim[0],
                    h: cubemap.info.dim[1],
                },
                dst_region: Rect2D {
                    x: 0,
                    y: 0,
                    w: target_info.dim[0],
                    h: target_info.dim[1],
                },
            });
            self.apply_skybox_binding();
        }

        if self.enabled {
            stream = stream.combine(self.clouds.record_compute(time, delta_time));
        }

        stream.end()
    }
}

fn cubemap_size_from_viewport(viewport: &Viewport) -> u32 {
    let size = viewport.area.w.min(viewport.area.h).max(1.0);
    size.round() as u32
}

impl SkyRenderer {
    fn apply_skybox_binding(&mut self) {
        let view = if self.use_procedural_cubemap {
            self.procedural_cubemap
                .as_ref()
                .map(|cubemap| cubemap.view)
                .unwrap_or(self.skybox_fallback_view)
        } else {
            self.skybox_fallback_view
        };

        self.skybox_pipeline.update_table(
            "skybox_texture",
            dashi::IndexedResource {
                resource: ShaderResource::Image(view),
                slot: 0,
            },
        );
    }

    fn ensure_cubemap_resources(&mut self, ctx: &mut dashi::Context, size: u32) -> bool {
        if self.cubemap_size == size && self.procedural_cubemap.is_some() {
            return false;
        }

        let info = NorenImageInfo {
            name: "[MESHI GFX SKY] Procedural Cubemap".to_string(),
            dim: [size, size, 1],
            layers: 6,
            format: self.cubemap_format,
            mip_levels: 1,
        };

        let mut dashi_info = info.dashi_cube();
        dashi_info.initial_data = None;

        let image = ctx
            .make_image(&dashi_info)
            .expect("Failed to create procedural sky cubemap image");

        let view = ImageView {
            img: image,
            aspect: AspectMask::Color,
            view_type: ImageViewType::Cube,
            range: SubresourceRange::new(0, info.mip_levels, 0, 6),
        };

        self.procedural_cubemap = Some(noren::rdb::imagery::DeviceCubemap {
            view,
            info: info.gpu(),
        });

        let mut faces = [ImageView::default(); 6];
        for (index, face) in faces.iter_mut().enumerate() {
            *face = ImageView {
                img: image,
                aspect: AspectMask::Color,
                view_type: ImageViewType::Type2D,
                range: SubresourceRange::new(0, info.mip_levels, index as u32, 1),
            };
        }

        self.cubemap_face_views = Some(faces);
        self.cubemap_size = size;
        self.cubemap_viewport = Viewport {
            area: FRect2D {
                x: 0.0,
                y: 0.0,
                w: size as f32,
                h: size as f32,
            },
            scissor: Rect2D {
                x: 0,
                y: 0,
                w: size,
                h: size,
            },
            min_depth: 0.0,
            max_depth: 1.0,
        };

        self.apply_skybox_binding();
        true
    }

    fn ensure_skybox_swap_target(
        &mut self,
        ctx: &mut dashi::Context,
        info: &GPUImageInfo,
    ) -> GPUImageInfo {
        let needs_rebuild = self
            .skybox_swap_info
            .as_ref()
            .map(|current| {
                current.dim != info.dim
                    || current.layers != info.layers
                    || current.format != info.format
                    || current.mip_levels != info.mip_levels
            })
            .unwrap_or(true);

        if needs_rebuild {
            self.skybox_fallback_view = create_skybox_swap_view(ctx, info);
            self.skybox_swap_info = Some(info.clone());
        }

        self.skybox_swap_info
            .clone()
            .unwrap_or_else(|| info.clone())
    }

    fn update_cubemap_cameras(
        &mut self,
        state: &mut BindlessState,
        camera: dashi::Handle<Camera>,
        size: u32,
    ) -> Option<()> {
        let camera_position =
            match state.reserved::<ReservedBindlessCamera>("meshi_bindless_cameras") {
                Ok(cameras) => cameras.camera(camera).position(),
                Err(_) => {
                    warn!("SkyRenderer failed to access bindless cameras for cubemap update");
                    Vec3::ZERO
                }
            };

        if self.cubemap_camera_handles.is_none() {
            let mut handles: Option<[Handle<Camera>; 6]> = None;
            let _ = state.reserved_mut(
                "meshi_bindless_cameras",
                |cameras: &mut ReservedBindlessCamera| {
                    handles = Some([
                        cameras.add_camera(),
                        cameras.add_camera(),
                        cameras.add_camera(),
                        cameras.add_camera(),
                        cameras.add_camera(),
                        cameras.add_camera(),
                    ]);
                },
            );
            self.cubemap_camera_handles = handles;
        }

        let handles = self.cubemap_camera_handles?;

        let face_settings = [
            (Vec3::X, -Vec3::Y),
            (-Vec3::X, -Vec3::Y),
            (Vec3::Y, Vec3::Z),
            (-Vec3::Y, -Vec3::Z),
            (Vec3::Z, -Vec3::Y),
            (-Vec3::Z, -Vec3::Y),
        ];

        let _ = state.reserved_mut(
            "meshi_bindless_cameras",
            |cameras: &mut ReservedBindlessCamera| {
                for ((direction, up), handle) in face_settings.iter().zip(handles.iter()) {
                    let view = Mat4::look_to_rh(camera_position, *direction, *up);
                    let cam = cameras.camera_mut(*handle);
                    cam.set_transform(view.inverse());
                    cam.set_perspective(
                        std::f32::consts::FRAC_PI_2,
                        size as f32,
                        size as f32,
                        0.1,
                        1000.0,
                    );
                }
            },
        );

        Some(())
    }

    fn update_sky_config(&mut self) {
        let config = &mut self.cfg.as_slice_mut::<SkyConfig>()[0];
        let sun_dir = resolve_celestial_direction(
            self.sky_settings.sun_direction,
            self.sky_settings.time_of_day,
            self.sky_settings.latitude_degrees,
            self.sky_settings.longitude_degrees,
            false,
        );
        let moon_dir = resolve_celestial_direction(
            self.sky_settings.moon_direction,
            self.sky_settings.time_of_day,
            self.sky_settings.latitude_degrees,
            self.sky_settings.longitude_degrees,
            true,
        );
        let sun_height = sun_dir.y.clamp(-1.0, 1.0);
        let day_factor = smoothstep(0.0, 0.25, sun_height);
        let night_factor = 1.0 - smoothstep(-0.2, 0.05, sun_height);
        let twilight_factor = (1.0 - day_factor - night_factor).clamp(0.0, 1.0);

        let day_horizon = Vec3::new(0.529, 0.808, 0.922);
        let day_zenith = Vec3::new(0.247, 0.52, 0.9);
        let twilight_horizon = Vec3::new(0.98, 0.58, 0.35);
        let twilight_zenith = Vec3::new(0.6, 0.32, 0.52);
        let night_horizon = Vec3::new(0.02, 0.02, 0.08);
        let night_zenith = Vec3::new(0.01, 0.01, 0.04);

        let horizon_tint = night_horizon * night_factor
            + twilight_horizon * twilight_factor
            + day_horizon * day_factor;
        let zenith_tint = night_zenith * night_factor
            + twilight_zenith * twilight_factor
            + day_zenith * day_factor;
        let intensity_scale = night_factor * 0.25 + twilight_factor * 0.7 + day_factor * 1.0;
        let sun_intensity_scale = day_factor + twilight_factor * 0.6;
        let moon_intensity_scale = night_factor + twilight_factor * 0.5;

        config.horizon_init = horizon_tint;
        config.zenith_tint = zenith_tint;
        config.sun_dir = sun_dir;
        config.sun_color = self.sky_settings.sun_color;
        config.sun_intensity = self.sky_settings.sun_intensity * sun_intensity_scale;
        config.sun_angular_radius = self.sky_settings.sun_angular_radius;
        config.moon_dir = moon_dir;
        config.moon_color = self.sky_settings.moon_color;
        config.moon_intensity = self.sky_settings.moon_intensity * moon_intensity_scale;
        config.moon_angular_radius = self.sky_settings.moon_angular_radius;
        config.intensity_scale = intensity_scale;
    }
}

fn resolve_celestial_direction(
    explicit: Option<Vec3>,
    time_of_day: Option<f32>,
    latitude_degrees: Option<f32>,
    longitude_degrees: Option<f32>,
    is_moon: bool,
) -> Vec3 {
    if let Some(direction) = explicit {
        if direction.length_squared() > 0.0 {
            return direction.normalize();
        }
    }

    if let Some(time) = time_of_day {
        let day_time = time.rem_euclid(24.0);
        let angle = day_time / 24.0 * std::f32::consts::TAU;
        let elevation = (angle - std::f32::consts::FRAC_PI_2).sin();
        let base = Vec3::new(angle.cos(), elevation, angle.sin());
        let latitude = latitude_degrees.unwrap_or(0.0).to_radians();
        let longitude = longitude_degrees.unwrap_or(0.0).to_radians();
        let rotation = Mat3::from_rotation_y(longitude) * Mat3::from_rotation_x(latitude);
        let mut dir = rotation * base;
        if is_moon {
            dir = -dir;
        }
        if dir.length_squared() > 0.0 {
            return dir.normalize();
        }
    }

    if is_moon { -Vec3::Y } else { Vec3::Y }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
