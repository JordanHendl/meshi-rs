mod cloud;

use super::EnvironmentRendererInfo;
use bento::builder::{AttachmentDesc, PSO, PSOBuilder};
use cloud::CloudSimulation;
use dashi::driver::command::Draw;
use dashi::structs::*;
use dashi::*;
use dashi::{ClearValue, DynamicAllocator, Format, ImageView, ShaderResource, Viewport};
use furikake::PSOBuilderFurikakeExt;
use furikake::{BindlessState, types::Camera};
use glam::*;
use tare::graph::{RenderGraph, SubpassInfo};
use tare::utils::StagedBuffer;

#[repr(C)]
#[derive(Default)]
struct SkyConfig {
    horizon_init: Vec3,
    intensity_scale: f32,
    zenith_tint: Vec3,
    _padding: f32,
}

pub struct SkyRenderer {
    pipeline: PSO,
    clouds: CloudSimulation,
    cfg: StagedBuffer,
}

impl SkyRenderer {
    pub fn new(
        ctx: &mut dashi::Context,
        state: &mut BindlessState,
        info: EnvironmentRendererInfo,
        dynamic: &DynamicAllocator,
    ) -> Self {
        let clouds = CloudSimulation::new(ctx);
        let shaders = miso::stdsky(&[]);

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

        Self {
            pipeline,
            clouds,
            cfg,
        }
    }

    pub fn add_pass(
        &mut self,
        graph: &mut RenderGraph,
        dynamic: &mut DynamicAllocator,
        viewport: Viewport,
        attachments: [Option<ImageView>; 8],
        clear_values: [Option<ClearValue>; 8],
        depth: Option<ImageView>,
        camera: dashi::Handle<Camera>,
        time: f32,
        delta_time: f32,
    ) {

        return ;
        self.clouds.update(time, delta_time);
        graph.add_subpass(
            &SubpassInfo {
                viewport,
                color_attachments: attachments,
                depth_attachment: depth,
                clear_values,
                depth_clear: None,
            },
            |mut cmd| {
                let mut alloc = dynamic
                    .bump()
                    .expect("Failed to allocate sky dynamic buffer");

                #[repr(C)]
                struct EnvParams {
                    camera: dashi::Handle<Camera>,
                    time: f32,
                    delta_time: f32,
                    _padding: [f32; 2],
                }

                let params = &mut alloc.slice::<EnvParams>()[0];
                params.camera = camera;
                params.time = time;
                params.delta_time = delta_time;
                params._padding = [0.0; 2];

                cmd = cmd
                    .bind_graphics_pipeline(self.pipeline.handle)
                    .update_viewport(&viewport)
                    .draw(&Draw {
                        bind_tables: self.pipeline.tables(),
                        dynamic_buffers: [None, Some(alloc), None, None],
                        instance_count: 1,
                        count: 3,
                        ..Default::default()
                    })
                    .unbind_graphics_pipeline();

                cmd
            },
        );
    }
}
