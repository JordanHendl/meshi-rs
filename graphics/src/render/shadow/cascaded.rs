use super::{ShadowPipelineMode, ShadowProcessInfo, ShadowSystemInfo};
use bento::{
    Compiler, OptimizationLevel, Request, ShaderLang,
    builder::{AttachmentDesc, PSO, PSOBuilder},
};
use dashi::{
    AspectMask, BufferView, ClearValue, Context, DepthInfo, DynamicAllocator, Format,
    GraphicsPipelineDetails, Handle, ImageInfo, IndexedResource, Rect2D, ShaderResource, Viewport,
    driver::command::DrawIndexedIndirect,
};
use furikake::{
    BindlessState, PSOBuilderFurikakeExt, reservations::bindless_camera::ReservedBindlessCamera,
};
use glam::{Mat4, Vec2, Vec3};
use tare::{graph::*, transient::TransientImage, utils::StagedBuffer};

#[derive(Clone, Copy, Debug)]
pub struct ShadowCascadeData {
    pub count: u32,
    pub splits: [f32; 4],
    pub matrices: [Mat4; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ShadowCascadeInfo {
    pub splits: [f32; 4],
    pub matrices: [Mat4; 4],
}

pub struct CascadedShadowResult {
    pub shadow_map: TransientImage,
    pub shadow_resolution: u32,
    pub cascade_data: ShadowCascadeData,
}

pub struct CascadedShadows {
    mode: ShadowPipelineMode,
    info: ShadowSystemInfo,
    pipeline: PSO,
    cascade_buffer: StagedBuffer,
}

impl CascadedShadows {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        per_draw_buffer: Handle<dashi::Buffer>,
        per_scene_dynamic: dashi::DynamicAllocatorState,
        info: ShadowSystemInfo,
        mode: ShadowPipelineMode,
        cascade_buffer: StagedBuffer,
    ) -> Self {
        let compiler = Compiler::new().expect("Failed to create shadow shader compiler");
        let request = Request {
            name: Some("meshi_shadow".to_string()),
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
            .expect("Failed to compile shadow vertex");
        let fragment = compiler
            .compile(
                include_str!("shaders/shadow_frag.slang").as_bytes(),
                &Request {
                    stage: dashi::ShaderType::Fragment,
                    ..request
                },
            )
            .expect("Failed to compile shadow fragment");

        let pipeline = PSOBuilder::new()
            .set_debug_name("[MESHI] Shadow Pass")
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
            .expect("Failed to add reserved variables for shadow pass")
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
            .expect("Failed to build shadow pass");
        state.register_pso_tables(&pipeline);

        Self {
            mode,
            info,
            pipeline,
            cascade_buffer,
        }
    }

    pub fn cascade_buffer(&self) -> &StagedBuffer {
        &self.cascade_buffer
    }

    fn compute_frustum_corners(
        camera: &furikake::types::Camera,
        split_near: f32,
        split_far: f32,
    ) -> [Vec3; 8] {
        let aspect = if camera.viewport.y.abs() > f32::EPSILON {
            camera.viewport.x / camera.viewport.y
        } else {
            1.0
        };
        let tan_fov = (camera.fov_y_radians.max(0.001) * 0.5).tan();
        let near_height = split_near * tan_fov;
        let near_width = near_height * aspect;
        let far_height = split_far * tan_fov;
        let far_width = far_height * aspect;

        let view_corners = [
            Vec3::new(-near_width, -near_height, -split_near),
            Vec3::new(near_width, -near_height, -split_near),
            Vec3::new(near_width, near_height, -split_near),
            Vec3::new(-near_width, near_height, -split_near),
            Vec3::new(-far_width, -far_height, -split_far),
            Vec3::new(far_width, -far_height, -split_far),
            Vec3::new(far_width, far_height, -split_far),
            Vec3::new(-far_width, far_height, -split_far),
        ];

        let mut corners = [Vec3::ZERO; 8];
        for (idx, corner) in view_corners.iter().enumerate() {
            corners[idx] = camera.world_from_camera.transform_point3(*corner);
        }
        corners
    }

    fn clip_space_fixup() -> Mat4 {
        Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 1.0,
        ])
    }

    fn compute_shadow_cascade_data(
        &self,
        camera: &furikake::types::Camera,
        light_direction: Vec3,
    ) -> ShadowCascadeData {
        let cascades = self.info.cascades;
        let cascade_count = cascades.cascade_count.clamp(1, 4);
        let mut splits = [camera.far; 4];
        let safe_near = camera.near.max(0.01);
        let safe_far = camera.far.max(safe_near + 0.01);
        let range = safe_far - safe_near;
        for cascade_index in 0..cascade_count as usize {
            let split = cascades.cascade_splits[cascade_index].clamp(0.0, 1.0);
            splits[cascade_index] = safe_near + range * split;
        }

        let light_dir = if light_direction.length_squared() > 0.0 {
            -light_direction.normalize()
        } else {
            Vec3::Y
        };
        let mut matrices = [Mat4::IDENTITY; 4];

        for cascade_index in 0..cascade_count as usize {
            let split_near = if cascade_index == 0 {
                camera.near
            } else {
                splits[cascade_index - 1]
            };
            let split_far = splits[cascade_index];
            let corners = Self::compute_frustum_corners(camera, split_near, split_far);
            let center = corners.iter().copied().sum::<Vec3>() / corners.len() as f32;
            let up = if light_dir.abs().dot(Vec3::Y) > 0.9 {
                Vec3::X
            } else {
                Vec3::Y
            };
            let eye = center - light_dir;
            let light_view = Mat4::look_at_rh(eye, center, up);

            let mut min = Vec3::splat(f32::MAX);
            let mut max = Vec3::splat(f32::MIN);
            for corner in corners.iter() {
                let light_space = light_view.transform_point3(*corner);
                min = min.min(light_space);
                max = max.max(light_space);
            }

            let extent = cascades.cascade_extents[cascade_index].max(0.01);
            let center_xy = Vec2::new((min.x + max.x) * 0.5, (min.y + max.y) * 0.5);
            let half_xy = Vec2::new(
                ((max.x - min.x) * 0.5).max(extent),
                ((max.y - min.y) * 0.5).max(extent),
            );
            min.x = center_xy.x - half_xy.x;
            max.x = center_xy.x + half_xy.x;
            min.y = center_xy.y - half_xy.y;
            max.y = center_xy.y + half_xy.y;

            let near = (-max.z).max(0.1);
            let far = (-min.z).max(near + 0.001);
            let light_proj = Mat4::orthographic_rh(min.x, max.x, min.y, max.y, near, far);
            matrices[cascade_index] = Self::clip_space_fixup() * light_proj * light_view;
        }

        ShadowCascadeData {
            count: cascade_count,
            splits,
            matrices,
        }
    }

    pub fn process(&mut self, info: ShadowProcessInfo) -> CascadedShadowResult {
        let camera = info
            .subrenderer
            .state()
            .reserved::<ReservedBindlessCamera>("meshi_bindless_cameras")
            .expect("Bindless camera table unavailable")
            .camera(info.subrenderer.camera);

        let cascade_data = self.compute_shadow_cascade_data(&camera, info.primary_light_direction);
        let resolution = self.info.cascaded_resolution.max(1);
        let cascade_count = cascade_data.count.max(1);
        let grid_x = if cascade_count > 1 { 2 } else { 1 };
        let grid_y = if cascade_count > 2 { 2 } else { 1 };

        {
            let cascade_info = &mut self.cascade_buffer.as_slice_mut::<ShadowCascadeInfo>()[0];
            cascade_info.splits = cascade_data.splits;
            cascade_info.matrices = cascade_data.matrices;
        }

        let shadow_map = info.subrenderer.graph_mut().make_image(&ImageInfo {
            debug_name: &format!("[MESHI {}] Shadow Map {}", self.mode.label(), info.view_idx),
            dim: [resolution * grid_x, resolution * grid_y, 1],
            layers: 1,
            format: Format::D24S8,
            mip_levels: 1,
            samples: self.info.sample_count,
            initial_data: None,
            ..Default::default()
        });
        let mut shadow_map = shadow_map;
        shadow_map.view.aspect = AspectMask::Depth;

        let atlas_viewport = Viewport {
            area: dashi::FRect2D {
                x: 0.0,
                y: 0.0,
                w: (resolution * grid_x) as f32,
                h: (resolution * grid_y) as f32,
            },
            scissor: Rect2D {
                x: 0,
                y: 0,
                w: resolution * grid_x,
                h: resolution * grid_y,
            },
            ..Default::default()
        };

        let clear: [Option<ClearValue>; 8] = [None; 8];
        let subrenderer = info.subrenderer;
        let pipeline = &self.pipeline;
        info.subrenderer.graph_mut().add_subpass(
            &SubpassInfo {
                name: Some("[MESHI] SHADOW PASS".to_string()),
                viewport: atlas_viewport,
                color_attachments: [None; 8],
                depth_attachment: Some(shadow_map.view),
                clear_values: clear,
                depth_clear: Some(self.info.depth_clear),
            },
            move |mut cmd| {
                for cascade_index in 0..cascade_count as usize {
                    let tile_x = (cascade_index as u32) % grid_x;
                    let tile_y = (cascade_index as u32) / grid_x;
                    let mut alloc = subrenderer
                        .dynamic_mut()
                        .bump()
                        .expect("Failed to allocate shadow scene data");
                    #[repr(C)]
                    struct PerSceneData {
                        light_view_proj: Mat4,
                    }
                    alloc.slice::<PerSceneData>()[0].light_view_proj =
                        cascade_data.matrices[cascade_index];
                    let viewport = Viewport {
                        area: dashi::FRect2D {
                            x: (tile_x * resolution) as f32,
                            y: (tile_y * resolution) as f32,
                            w: resolution as f32,
                            h: resolution as f32,
                        },
                        scissor: Rect2D {
                            x: tile_x * resolution,
                            y: tile_y * resolution,
                            w: resolution,
                            h: resolution,
                        },
                        ..Default::default()
                    };
                    cmd = cmd
                        .bind_graphics_pipeline(pipeline.handle)
                        .update_viewport(&viewport)
                        .draw_indexed_indirect(&DrawIndexedIndirect {
                            indirect: subrenderer.draw_list,
                            bind_tables: pipeline.tables(),
                            dynamic_buffers: [None, None, Some(alloc), None],
                            draw_count: subrenderer.draw_count,
                            ..Default::default()
                        })
                        .unbind_graphics_pipeline();
                }
                cmd
            },
        );

        CascadedShadowResult {
            shadow_map,
            shadow_resolution: resolution,
            cascade_data,
        }
    }
}
