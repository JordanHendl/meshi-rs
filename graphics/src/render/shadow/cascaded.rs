use crate::ShadowCascadeSettings;
use crate::render::{SpotShadowLight, SubrendererDrawInfo};
use crate::render::utils::gpu_draw_builder::GPUDrawBuilder;
use dashi::cmd::Executable;
use dashi::gpu::cmd::{Scope, SyncPoint};
use dashi::{
    AspectMask, ClearValue, CommandStream, Context, DynamicAllocator, Format, Handle, ImageInfo,
    Rect2D, ShaderResource, Viewport,
};
use furikake::BindlessState;
use furikake::reservations::ReservedBinding;
use glam::{Mat4, Vec2, Vec3};
use meshi_ffi_structs::LightInfo;
use tare::graph::*;
use tare::transient::TransientImage;
use tare::utils::StagedBuffer;

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
    cascade_buffer: StagedBuffer,
}

impl CascadedShadows {
    pub fn new(
        ctx: &mut Context,
        info: &ShadowSystemInfo,
        mode: ShadowPipelineMode,
    ) -> Self {

        Self {
            mode,
        }
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
        let fov_y = camera.fov_y_radians.max(0.001);
        let tan_fov = (fov_y * 0.5).tan();
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

    fn compute_splits(cascades: ShadowCascadeSettings, near: f32, far: f32) -> [f32; 4] {
        let mut splits = [far; 4];
        let count = cascades.cascade_count.clamp(1, 4) as usize;
        if count == 0 {
            return splits;
        }
        let safe_near = near.max(0.01);
        let safe_far = far.max(safe_near + 0.01);
        let range = safe_far - safe_near;
        for cascade_index in 0..count {
            let split = cascades.cascade_splits[cascade_index].clamp(0.0, 1.0);
            splits[cascade_index] = safe_near + range * split;
        }
        splits
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
    ) -> &mut ShadowCascadeData {
//        let cascades = self.main_pass.cascades();
        let cascade_count = cascades.cascade_count.clamp(1, 4);
        let splits = Self::compute_splits(cascades, camera.near, camera.far);

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
            let mut center = Vec3::ZERO;
            for corner in corners.iter() {
                center += *corner;
            }
            center /= corners.len() as f32;

            let up = if light_dir.abs().dot(Vec3::Y) > 0.9 {
                Vec3::X
            } else {
                Vec3::Y
            };
            let mut min = Vec3::splat(f32::MAX);
            let mut max = Vec3::splat(f32::MIN);
            let depth = 1.0;
            let eye = center - light_dir * depth;
            let light_view = Mat4::look_at_rh(eye, center, up);

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

            let mut min_z = min.z;
            let mut max_z = max.z;
            if min_z > max_z {
                std::mem::swap(&mut min_z, &mut max_z);
            }

            let mut near = (-max_z).max(0.1);
            let mut far = (-min_z).max(near + 0.001);
            if !near.is_finite() || !far.is_finite() {
                near = 0.1;
                far = 1.0;
            }
            if (far - near).abs() < 0.001 {
                far = near + 0.001;
            }

            let light_proj = Mat4::orthographic_rh(min.x, max.x, min.y, max.y, near, far);
            matrices[cascade_index] = Self::clip_space_fixup() * light_proj * light_view;
        }

        let bump = crate::render::global_bump().get();
        bump.alloc(ShadowCascadeData {
            count: cascade_count,
            splits,
            matrices,
        })
    }

    pub fn process(
        &mut self,
        info: &SubrendererDrawInfo,
    ) -> CascadedShadowResult {
//        let bump = crate::render::global_bump().get();
//        let s = self as *mut Self;
//        let shadow_resolution = bump.alloc(self.main_pass.resolution());
//        let cascade_data = unsafe {
//            (*s).compute_shadow_cascade_data(camera, environment.primary_light_direction())
//        };
//        let cascade_count = cascade_data.count.max(1);
//        let grid_x = bump.alloc(if cascade_count > 1 { 2 } else { 1 });
//        let grid_y = bump.alloc(if cascade_count > 2 { 2 } else { 1 });
//        {
//            let cascade_info = &mut self.cascade_buffer.as_slice_mut::<ShadowCascadeInfo>()[0];
//            cascade_info.splits = cascade_data.splits;
//            cascade_info.matrices = cascade_data.matrices;
//        }
//
//        let shadow_atlas_width = *shadow_resolution * *grid_x;
//        let shadow_atlas_height = *shadow_resolution * *grid_y;
//        let shadow_viewport = bump.alloc(Viewport {
//            area: dashi::FRect2D {
//                x: 0.0,
//                y: 0.0,
//                w: shadow_atlas_width as f32,
//                h: shadow_atlas_height as f32,
//            },
//            scissor: Rect2D {
//                x: 0,
//                y: 0,
//                w: shadow_atlas_width,
//                h: shadow_atlas_height,
//            },
//            ..Default::default()
//        });
//
//        let shadow_map = graph.make_image(&ImageInfo {
//            debug_name: &format!("[MESHI {}] Shadow Map {view_idx}", self.mode.label()),
//            dim: [shadow_atlas_width, shadow_atlas_height, 1],
//            layers: 1,
//            format: Format::D24S8,
//            mip_levels: 1,
//            samples: self.main_pass.sample_count(),
//            initial_data: None,
//            ..Default::default()
//        });
//        let mut shadow_map = shadow_map;
//        shadow_map.view.aspect = AspectMask::Depth;
//
//        graph.add_compute_pass(|cmd| {
//            let cmd = cmd
//                .combine(draw_builder.build_draws(super::BIN_SHADOW, view_idx))
//                .combine(environment.build_terrain_draws(TERRAIN_DRAW_BIN, view_idx))
//                .sync(SyncPoint::ComputeToGraphics, Scope::AllCommonReads);
//
//            cmd.end()
//        });
//
//        let shadow_clear: [Option<ClearValue>; 8] = [None; 8];
//        graph.add_subpass(
//            &SubpassInfo {
//                name: Some("[MESHI] SHADOW PASS".to_string()),
//                viewport: *shadow_viewport,
//                color_attachments: [None; 8],
//                depth_attachment: Some(shadow_map.view),
//                clear_values: shadow_clear,
//                depth_clear: Some(self.main_pass.depth_clear_value()),
//            },
//            |mut cmd| {
//                let indices = state
//                    .binding("meshi_bindless_indices")
//                    .expect("Bindless indices not available")
//                    .binding();
//
//                let indices_handle = match indices {
//                    ReservedBinding::TableBinding {
//                        binding: _,
//                        resources,
//                    } => match resources[0].resource {
//                        ShaderResource::StorageBuffer(view) => Some(view.handle),
//                        _ => None,
//                    },
//                    _ => None,
//                };
//
//                let Some(indices_handle) = indices_handle else {
//                    return cmd;
//                };
//
//                let (terrain_draw_list, terrain_draw_count) = environment
//                    .terrain_draw_builder()
//                    .map(|builder| (builder.draw_list(), builder.draw_count()))
//                    .unwrap_or((Handle::default(), 0));
//
//                let cascade_count = cascade_data.count.max(1);
//                let main_per_draw = draw_builder.per_draw_data();
//                self.main_pass.set_per_draw_data(main_per_draw);
//                for cascade_index in 0..cascade_count as usize {
//                    let tile_x = (cascade_index as u32) % *grid_x;
//                    let tile_y = (cascade_index as u32) / *grid_x;
//                    let cascade_viewport = Viewport {
//                        area: dashi::FRect2D {
//                            x: (tile_x * *shadow_resolution) as f32,
//                            y: (tile_y * *shadow_resolution) as f32,
//                            w: *shadow_resolution as f32,
//                            h: *shadow_resolution as f32,
//                        },
//                        scissor: Rect2D {
//                            x: tile_x * *shadow_resolution,
//                            y: tile_y * *shadow_resolution,
//                            w: *shadow_resolution,
//                            h: *shadow_resolution,
//                        },
//                        ..Default::default()
//                    };
//                    cmd = cmd.combine(self.main_pass.record(
//                        &cascade_viewport,
//                        dynamic,
//                        cascade_data.matrices[cascade_index],
//                        indices_handle,
//                        draw_builder.draw_list(),
//                        draw_builder.draw_count(),
//                    ));
//                    if terrain_draw_count > 0 {
//                        cmd = cmd.combine(self.terrain_pass.record(
//                            &cascade_viewport,
//                            dynamic,
//                            cascade_data.matrices[cascade_index],
//                            indices_handle,
//                            terrain_draw_list,
//                            terrain_draw_count,
//                        ));
//                    }
//                }
//
//                cmd
//            },
//        );
//
//        CascadedShadowResult {
//            shadow_map,
//            shadow_resolution: *shadow_resolution,
//            cascade_data: cascade_data.clone(),
//        }
    }
}

