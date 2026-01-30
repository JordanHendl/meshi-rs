use bento::builder::{AttachmentDesc, PSO, PSOBuilder};
use dashi::UsageBits;
use dashi::cmd::PendingGraphics;
use dashi::cmd::*;
use dashi::driver::command::Draw;
use dashi::*;
use dashi::{
    BufferInfo, BufferUsage, CommandStream, Context, DepthInfo, Format, GraphicsPipelineDetails,
    Handle, IndexedResource, MemoryVisibility, Sampler, SamplerInfo, ShaderResource, ShaderType,
    Viewport,
};
use tare::utils::StagedBuffer;

use crate::CloudDebugView;
use crate::render::environment::clouds::cloud_assets::CloudAssets;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CloudCompositeParams {
    pub output_resolution: [u32; 2],
    pub low_resolution: [u32; 2],
    pub camera_near: f32,
    pub camera_far: f32,
    pub depth_sigma: f32,
    pub debug_view: u32,
    pub history_weight_scale: f32,
    pub shadow_resolution: f32,
    pub history_index: u32,
    pub atmosphere_view_strength: f32,
    pub atmosphere_view_extinction: f32,
    pub atmosphere_haze_strength: f32,
    pub atmosphere_haze_color: [f32; 4],
    pub shadow_cascade_count: u32,
    pub shadow_cascade_resolutions: [u32; 4],
    pub shadow_cascade_offsets: [u32; 4],
}

pub struct CloudCompositePass {
    pipeline: PSO,
    params: StagedBuffer,
    sampler: Handle<Sampler>,
}

impl CloudCompositePass {
    pub fn new(
        ctx: &mut Context,
        assets: &CloudAssets,
        history_color: [Handle<dashi::Buffer>; 2],
        history_transmittance: [Handle<dashi::Buffer>; 2],
        history_depth: [Handle<dashi::Buffer>; 2],
        cloud_steps: Handle<dashi::Buffer>,
        history_weight: [Handle<dashi::Buffer>; 2],
        shadow_buffer: Handle<dashi::Buffer>,
        depth_view: dashi::ImageView,
        sample_count: dashi::SampleCount,
    ) -> Self {
        let params = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[CLOUD] Composite Params",
                byte_size: (std::mem::size_of::<CloudCompositeParams>() as u32).max(256),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::UNIFORM,
                initial_data: None,
            },
        );

        let sampler = ctx
            .make_sampler(&SamplerInfo::default())
            .expect("create cloud composite sampler");

        let compiler = bento::Compiler::new().expect("create shader compiler");
        let base_request = bento::Request {
            name: Some("cloud_composite".to_string()),
            lang: bento::ShaderLang::Glsl,
            stage: ShaderType::Vertex,
            optimization: bento::OptimizationLevel::Performance,
            debug_symbols: true,
            defines: Default::default(),
        };

        let vertex = compiler
            .compile(
                include_str!("shaders/cloud_composite.vert.glsl").as_bytes(),
                &bento::Request {
                    stage: ShaderType::Vertex,
                    ..base_request.clone()
                },
            )
            .expect("compile cloud composite vertex");
        let fragment = compiler
            .compile(
                include_str!("shaders/cloud_composite.frag.glsl").as_bytes(),
                &bento::Request {
                    stage: ShaderType::Fragment,
                    ..base_request
                },
            )
            .expect("compile cloud composite fragment");

        let pipeline = PSOBuilder::new()
            .vertex_compiled(Some(vertex))
            .fragment_compiled(Some(fragment))
            .add_table_variable_with_resources(
                "params",
                vec![IndexedResource {
                    resource: ShaderResource::ConstBuffer(params.device().into()),
                    slot: 0,
                }],
            )
            // set = 1, binding = 0..9 (each SSBO as its own variable name to match the shader bindings)
            .add_table_variable_with_resources(
                "cloud_color_a",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(history_color[0].into()),
                    slot: 0, // binding 0
                }],
            )
            .add_table_variable_with_resources(
                "cloud_color_b",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(history_color[1].into()),
                    slot: 0, // binding 1
                }],
            )
            .add_table_variable_with_resources(
                "cloud_trans_a",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(history_transmittance[0].into()),
                    slot: 0, // binding 2
                }],
            )
            .add_table_variable_with_resources(
                "cloud_trans_b",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(history_transmittance[1].into()),
                    slot: 0, // binding 3
                }],
            )
            .add_table_variable_with_resources(
                "cloud_depth_a",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(history_depth[0].into()),
                    slot: 0, // binding 4
                }],
            )
            .add_table_variable_with_resources(
                "cloud_depth_b",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(history_depth[1].into()),
                    slot: 0, // binding 5
                }],
            )
            .add_table_variable_with_resources(
                "cloud_steps",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(cloud_steps.into()),
                    slot: 0, // binding 6
                }],
            )
            .add_table_variable_with_resources(
                "cloud_weight_a",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(history_weight[0].into()),
                    slot: 0, // binding 7
                }],
            )
            .add_table_variable_with_resources(
                "cloud_weight_b",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(history_weight[1].into()),
                    slot: 0, // binding 8
                }],
            )
            .add_table_variable_with_resources(
                "cloud_shadow",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(shadow_buffer.into()),
                    slot: 0, // binding 9
                }],
            )
            // set = 2, binding = 0..2
            .add_table_variable_with_resources(
                "cloud_weather_map",
                vec![IndexedResource {
                    resource: ShaderResource::Image(assets.weather_map_view()),
                    slot: 0, // binding 0
                }],
            )
            .add_table_variable_with_resources(
                "scene_depth",
                vec![IndexedResource {
                    resource: ShaderResource::Image(depth_view),
                    slot: 0, // binding 1
                }],
            )
            .add_table_variable_with_resources(
                "cloud_sampler",
                vec![IndexedResource {
                    resource: ShaderResource::Sampler(sampler),
                    slot: 0, // binding 2
                }],
            )
            .set_attachment_format(0, Format::BGRA8)
            .add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: sample_count,
            })
            .set_details(GraphicsPipelineDetails {
                color_blend_states: vec![dashi::ColorBlendState {
                    enable: true,
                    src_blend: dashi::BlendFactor::One,
                    dst_blend: dashi::BlendFactor::InvSrcAlpha,
                    blend_op: dashi::BlendOp::Add,
                    src_alpha_blend: dashi::BlendFactor::One,
                    dst_alpha_blend: dashi::BlendFactor::InvSrcAlpha,
                    alpha_blend_op: dashi::BlendOp::Add,
                    write_mask: Default::default(),
                }],
                sample_count,
                depth_test: Some(DepthInfo {
                    should_test: true,
                    should_write: false,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .build(ctx)
            .expect("build cloud composite pipeline");

        Self {
            pipeline,
            params,
            sampler,
        }
    }

    pub fn update_params(
        &mut self,
        output_resolution: [u32; 2],
        low_resolution: [u32; 2],
        camera_near: f32,
        camera_far: f32,
        depth_sigma: f32,
        debug_view: CloudDebugView,
        history_weight_scale: f32,
        shadow_resolution: u32,
        history_index: u32,
        atmosphere_view_strength: f32,
        atmosphere_view_extinction: f32,
        atmosphere_haze_strength: f32,
        atmosphere_haze_color: [f32; 4],
        shadow_cascade_count: u32,
        shadow_cascade_resolutions: [u32; 4],
        shadow_cascade_offsets: [u32; 4],
    ) {
        let params = &mut self.params.as_slice_mut::<CloudCompositeParams>()[0];
        *params = CloudCompositeParams {
            output_resolution,
            low_resolution,
            camera_near,
            camera_far,
            depth_sigma,
            debug_view: debug_view as u32,
            history_weight_scale,
            shadow_resolution: shadow_resolution as f32,
            history_index,
            atmosphere_view_strength,
            atmosphere_view_extinction,
            atmosphere_haze_strength,
            atmosphere_haze_color,
            shadow_cascade_count,
            shadow_cascade_resolutions,
            shadow_cascade_offsets,
        };
    }

    pub fn record(
        &mut self,
        viewport: &Viewport,
        timer_index: u32,
    ) -> CommandStream<PendingGraphics> {
        CommandStream::<PendingGraphics>::subdraw()
            .combine(self.params.sync_up())
            .gpu_timer_begin(timer_index)
            .bind_graphics_pipeline(self.pipeline.handle)
            .update_viewport(viewport)
            .draw(&Draw {
                bind_tables: self.pipeline.tables(),
                count: 3,
                instance_count: 1,
                ..Default::default()
            })
            .unbind_graphics_pipeline()
            .gpu_timer_end(timer_index)
    }

    pub fn pipeline(&self) -> &PSO {
        &self.pipeline
    }

    pub fn sampler(&self) -> Handle<Sampler> {
        self.sampler
    }
}
