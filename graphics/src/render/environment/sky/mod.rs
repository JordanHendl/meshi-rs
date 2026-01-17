mod cloud;

use super::EnvironmentRendererInfo;
use bento::builder::{AttachmentDesc, PSO, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use cloud::CloudSimulation;
use dashi::cmd::{Executable, PendingGraphics};
use dashi::driver::command::Draw;
use dashi::structs::*;
use dashi::*;
use dashi::{
    AspectMask, CommandStream, DynamicAllocator, Format, ImageView, ImageViewType, Sampler,
    SamplerInfo, ShaderResource, SubresourceRange, Viewport,
};
use furikake::PSOBuilderFurikakeExt;
use furikake::{BindlessState, types::Camera};
use glam::*;
use noren::rdb::imagery::{HostCubemap, ImageInfo as NorenImageInfo};
use tare::utils::StagedBuffer;

#[derive(Clone)]
pub struct SkyboxInfo {
    pub cubemap: Option<noren::rdb::imagery::DeviceCubemap>,
    pub intensity: f32,
}

impl Default for SkyboxInfo {
    fn default() -> Self {
        Self {
            cubemap: None,
            intensity: 1.0,
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
}

#[repr(C)]
struct SkyboxParams {
    camera_index: u32,
    intensity: f32,
    _padding: [f32; 2],
}

pub struct SkyRenderer {
    pipeline: PSO,
    skybox_pipeline: PSO,
    skybox_sampler: Handle<Sampler>,
    skybox_intensity: f32,
    clouds: CloudSimulation,
    cfg: StagedBuffer,
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

impl SkyRenderer {
    pub fn new(
        ctx: &mut dashi::Context,
        state: &mut BindlessState,
        info: &EnvironmentRendererInfo,
        dynamic: &DynamicAllocator,
    ) -> Self {
        let clouds = CloudSimulation::new(ctx);
        let shaders = miso::stdsky(&[]);
        let skybox_shaders = compile_skybox_shaders();

        let skybox_view = info
            .skybox
            .cubemap
            .as_ref()
            .map(|cubemap| cubemap.view)
            .unwrap_or_else(|| default_skybox_view(ctx));
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
                    resource: ShaderResource::ConstBuffer(cfg.device()),
                    slot: 0,
                }],
            );

        pso_builder = pso_builder
            .add_reserved_table_variable(state, "meshi_bindless_cameras")
            .unwrap()
            .add_reserved_table_variable(state, "meshi_bindless_lights")
            .unwrap()
            .add_reserved_table_variable(state, "meshi_bindless_textures")
            .unwrap()
            .add_reserved_table_variable(state, "meshi_bindless_samplers")
            .unwrap()
            .add_reserved_table_variable(state, "meshi_bindless_materials")
            .unwrap()
            .add_reserved_table_variable(state, "meshi_bindless_transformations")
            .unwrap();

        if info.use_depth {
            pso_builder = pso_builder.add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: info.sample_count,
            });
        }

        let depth_test = if info.use_depth {
            Some(dashi::DepthInfo {
                should_test: false,
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
            skybox_intensity: info.skybox.intensity,
            clouds,
            cfg,
        }
    }

    pub fn record_draws(
        &mut self,
        viewport: &Viewport,
        dynamic: &mut DynamicAllocator,
        camera: dashi::Handle<Camera>,
        time: f32,
        delta_time: f32,
    ) -> CommandStream<PendingGraphics> {
        self.clouds.update(time, delta_time);

        let mut alloc = dynamic
            .bump()
            .expect("Failed to allocate sky dynamic buffer");

        let params = &mut alloc.slice::<SkyboxParams>()[0];
        params.camera_index = camera.slot as u32;
        params.intensity = self.skybox_intensity;
        params._padding = [0.0; 2];

        CommandStream::<PendingGraphics>::subdraw()
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
