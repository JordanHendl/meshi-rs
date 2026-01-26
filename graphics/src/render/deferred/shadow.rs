use bento::builder::{AttachmentDesc, PSO, PSOBuilder};
use bento::{Compiler, OptimizationLevel, Request, ShaderLang};
use dashi::cmd::PendingGraphics;
use dashi::driver::command::DrawIndexedIndirect;
use dashi::{
    ClearValue, CommandStream, Context, DepthInfo, Format, GraphicsPipelineDetails, Handle,
    IndexedResource, SampleCount, ShaderResource, ShaderType, Viewport,
};
use furikake::PSOBuilderFurikakeExt;

use crate::ShadowCascadeSettings;
use crate::render::gpu_draw_builder::GPUDrawBuilder;

pub struct ShadowPassInfo {
    pub resolution: u32,
    pub sample_count: SampleCount,
    pub cascades: ShadowCascadeSettings,
}

impl Default for ShadowPassInfo {
    fn default() -> Self {
        Self {
            resolution: 2048,
            sample_count: SampleCount::S1,
            cascades: ShadowCascadeSettings::default(),
        }
    }
}

pub struct ShadowPass {
    pipeline: PSO,
    resolution: u32,
    sample_count: SampleCount,
    cascades: ShadowCascadeSettings,
}

impl ShadowPass {
    pub fn new(
        ctx: &mut Context,
        state: &mut furikake::BindlessState,
        draw_builder: &GPUDrawBuilder,
        dynamic: &dashi::DynamicAllocator,
        info: ShadowPassInfo,
    ) -> Self {
        let compiler = Compiler::new().expect("Failed to create shader compiler");
        let base_request = Request {
            name: Some("meshi_shadow_map".to_string()),
            lang: ShaderLang::Slang,
            stage: ShaderType::Vertex,
            optimization: OptimizationLevel::Performance,
            debug_symbols: true,
            defines: Default::default(),
        };

        let vertex = compiler
            .compile(
                include_str!("shaders/shadow_vert.slang").as_bytes(),
                &Request {
                    stage: ShaderType::Vertex,
                    ..base_request.clone()
                },
            )
            .expect("Failed to compile shadow vertex shader");
        let fragment = compiler
            .compile(
                include_str!("shaders/shadow_frag.slang").as_bytes(),
                &Request {
                    stage: ShaderType::Fragment,
                    ..base_request
                },
            )
            .expect("Failed to compile shadow fragment shader");

        let pso = PSOBuilder::new()
            .set_debug_name("[MESHI] Shadow Map")
            .vertex_compiled(Some(vertex))
            .fragment_compiled(Some(fragment))
            .add_table_variable_with_resources(
                "per_draw_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(draw_builder.per_draw_data().into()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "per_scene_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::DynamicStorage(dynamic.state()),
                    slot: 0,
                }],
            )
            .add_reserved_table_variables(state)
            .unwrap()
            .add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: info.sample_count,
            })
            .set_details(GraphicsPipelineDetails {
                color_blend_states: Vec::new(),
                sample_count: info.sample_count,
                depth_test: Some(DepthInfo {
                    should_test: true,
                    should_write: true,
                }),
                ..Default::default()
            })
            .build(ctx)
            .expect("Failed to build shadow pipeline");

        state.register_pso_tables(&pso);

        Self {
            pipeline: pso,
            resolution: info.resolution,
            sample_count: info.sample_count,
            cascades: info.cascades,
        }
    }

    pub fn resolution(&self) -> u32 {
        self.resolution
    }

    pub fn sample_count(&self) -> SampleCount {
        self.sample_count
    }

    pub fn cascades(&self) -> ShadowCascadeSettings {
        self.cascades
    }

    pub fn depth_clear_value(&self) -> ClearValue {
        ClearValue::DepthStencil {
            depth: 1.0,
            stencil: 0,
        }
    }

    pub fn record(
        &self,
        viewport: &Viewport,
        dynamic: &mut dashi::DynamicAllocator,
        light_view_proj: glam::Mat4,
        indices_handle: Handle<dashi::Buffer>,
        draw_list: Handle<dashi::Buffer>,
        draw_count: u32,
    ) -> CommandStream<PendingGraphics> {
        #[repr(C)]
        struct PerSceneData {
            light_view_proj: glam::Mat4,
        }

        let mut alloc = dynamic
            .bump()
            .expect("Failed to allocate shadow pass dynamic buffer");
        alloc.slice::<PerSceneData>()[0].light_view_proj = light_view_proj;

        CommandStream::<PendingGraphics>::subdraw()
            .bind_graphics_pipeline(self.pipeline.handle)
            .update_viewport(viewport)
            .draw_indexed_indirect(&DrawIndexedIndirect {
                indices: indices_handle,
                indirect: draw_list,
                bind_tables: self.pipeline.tables(),
                dynamic_buffers: [None, None, Some(alloc), None],
                draw_count,
                ..Default::default()
            })
            .unbind_graphics_pipeline()
    }
}
