use super::{ShadowPipelineMode, ShadowProcessInfo, ShadowSystemInfo};
use bento::{
    Compiler, OptimizationLevel, Request, ShaderLang,
    builder::{AttachmentDesc, PSO, PSOBuilder},
};
use dashi::{
    AspectMask, BufferView, ClearValue, Context, DepthInfo, Format, GraphicsPipelineDetails,
    Handle, ImageInfo, IndexedResource, Rect2D, ShaderResource, Viewport,
    driver::command::DrawIndexedIndirect,
};
use furikake::{BindlessState, PSOBuilderFurikakeExt};
use glam::{Mat4, Vec3};
use meshi_ffi_structs::LightInfo;
use tare::{graph::*, transient::TransientImage};

pub struct SpotShadowResult {
    pub shadow_map: Option<TransientImage>,
    pub shadow_bindless_id: u32,
    pub shadow_resolution: u32,
    pub shadow_matrix: Mat4,
}

pub struct SpotShadows {
    mode: ShadowPipelineMode,
    info: ShadowSystemInfo,
    pipeline: PSO,
}

impl SpotShadows {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        per_draw_buffer: Handle<dashi::Buffer>,
        per_scene_dynamic: dashi::DynamicAllocatorState,
        info: ShadowSystemInfo,
        mode: ShadowPipelineMode,
    ) -> Self {
        let compiler = Compiler::new().expect("Failed to create spot shadow compiler");
        let request = Request {
            name: Some("meshi_spot_shadow".to_string()),
            lang: ShaderLang::Slang,
            optimization: OptimizationLevel::Performance,
            debug_symbols: true,
            ..Default::default()
        };
        let vertex = compiler
            .compile(
                include_str!("shaders/shadow_vert.slang").as_bytes(),
                &Request {
                    stage: dashi::ShaderType::Vertex,
                    ..request.clone()
                },
            )
            .expect("Failed to compile spot shadow vertex");
        let fragment = compiler
            .compile(
                include_str!("shaders/shadow_frag.slang").as_bytes(),
                &Request {
                    stage: dashi::ShaderType::Fragment,
                    ..request
                },
            )
            .expect("Failed to compile spot shadow fragment");

        let pipeline = PSOBuilder::new()
            .set_debug_name("[MESHI] Spot Shadow Pass")
            .vertex_compiled(Some(vertex))
            .fragment_compiled(Some(fragment))
            .add_table_variable_with_resources(
                "per_draw_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::StorageBuffer(per_draw_buffer.into()),
                    slot: 0,
                }],
            )
            .add_table_variable_with_resources(
                "per_scene_ssbo",
                vec![IndexedResource {
                    resource: ShaderResource::DynamicStorage(per_scene_dynamic),
                    slot: 0,
                }],
            )
            .add_reserved_table_variables(state)
            .expect("Failed to add reserved spot shadow variables")
            .add_depth_target(AttachmentDesc {
                format: Format::D24S8,
                samples: info.sample_count,
            })
            .set_details(GraphicsPipelineDetails {
                color_blend_states: vec![],
                sample_count: info.sample_count,
                depth_test: Some(DepthInfo {
                    should_test: true,
                    should_write: true,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .build(ctx)
            .expect("Failed to build spot shadow pass");
        state.register_pso_tables(&pipeline);

        Self {
            mode,
            info,
            pipeline,
        }
    }

    fn clip_space_fixup() -> Mat4 {
        Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 1.0,
        ])
    }

    fn spot_shadow_matrix(light: &LightInfo) -> Mat4 {
        let position = Vec3::new(light.pos_x, light.pos_y, light.pos_z);
        let mut direction = Vec3::new(light.dir_x, light.dir_y, light.dir_z);
        direction = if direction.length_squared() > 0.0 {
            direction.normalize()
        } else {
            Vec3::NEG_Z
        };
        let up = if direction.abs().dot(Vec3::Y) > 0.9 {
            Vec3::X
        } else {
            Vec3::Y
        };
        let view = Mat4::look_at_rh(position, position + direction, up);
        let fov =
            (light.spot_outer_angle_rad.max(0.01) * 2.0).clamp(0.01, std::f32::consts::PI - 0.01);
        let near = 0.1;
        let far = if light.range > near {
            light.range
        } else {
            1000.0
        };
        Self::clip_space_fixup() * Mat4::perspective_rh(fov, 1.0, near, far) * view
    }

    pub fn process(&mut self, info: ShadowProcessInfo) -> SpotShadowResult {
        let Some(light) = info.spot_light else {
            return SpotShadowResult {
                shadow_map: None,
                shadow_bindless_id: 0,
                shadow_resolution: 0,
                shadow_matrix: Mat4::IDENTITY,
            };
        };

        let resolution = self.info.spot_resolution.max(1);
        let shadow_matrix = Self::spot_shadow_matrix(&light.info);

        let shadow_map = info.subrenderer.graph_mut().make_image(&ImageInfo {
            debug_name: &format!(
                "[MESHI {}] Spot Shadow Map {}",
                self.mode.label(),
                info.view_idx
            ),
            dim: [resolution, resolution, 1],
            layers: 1,
            format: Format::D24S8,
            mip_levels: 1,
            samples: self.info.sample_count,
            initial_data: None,
            ..Default::default()
        });
        let mut shadow_map = shadow_map;
        shadow_map.view.aspect = AspectMask::Depth;

        let viewport = Viewport {
            area: dashi::FRect2D {
                x: 0.0,
                y: 0.0,
                w: resolution as f32,
                h: resolution as f32,
            },
            scissor: Rect2D {
                x: 0,
                y: 0,
                w: resolution,
                h: resolution,
            },
            ..Default::default()
        };
        let clear: [Option<ClearValue>; 8] = [None; 8];

        let subrenderer = info.subrenderer;
        let pipeline = &self.pipeline;
        info.subrenderer.graph_mut().add_subpass(
            &SubpassInfo {
                name: Some("[MESHI] SPOT SHADOW PASS".to_string()),
                viewport,
                color_attachments: [None; 8],
                depth_attachment: Some(shadow_map.view),
                clear_values: clear,
                depth_clear: Some(self.info.depth_clear),
            },
            move |cmd| {
                let mut alloc = subrenderer
                    .dynamic_mut()
                    .bump()
                    .expect("Failed to allocate spot scene data");
                #[repr(C)]
                struct PerSceneData {
                    light_view_proj: Mat4,
                }
                alloc.slice::<PerSceneData>()[0].light_view_proj = shadow_matrix;
                cmd.bind_graphics_pipeline(pipeline.handle)
                    .update_viewport(&viewport)
                    .draw_indexed_indirect(&DrawIndexedIndirect {
                        indirect: subrenderer.draw_list,
                        bind_tables: pipeline.tables(),
                        dynamic_buffers: [None, None, Some(alloc), None],
                        draw_count: subrenderer.draw_count,
                        ..Default::default()
                    })
                    .unbind_graphics_pipeline()
            },
        );

        SpotShadowResult {
            shadow_bindless_id: shadow_map.bindless_id.unwrap_or(0) as u32,
            shadow_map: Some(shadow_map),
            shadow_resolution: resolution,
            shadow_matrix,
        }
    }
}
