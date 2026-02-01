use crate::ShadowCascadeSettings;
use crate::render::SpotShadowLight;
use crate::render::deferred::shadow::{ShadowPass, ShadowPassInfo};
use crate::render::environment::{EnvironmentRenderer, terrain::TERRAIN_DRAW_BIN};
use crate::render::gpu_draw_builder::GPUDrawBuilder;
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
pub enum ShadowPipelineMode {
    Deferred,
    Forward,
}

impl ShadowPipelineMode {
    fn label(self) -> &'static str {
        match self {
            ShadowPipelineMode::Deferred => "DEFERRED",
            ShadowPipelineMode::Forward => "FORWARD",
        }
    }
}

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

pub struct SpotShadowResult {
    pub shadow_map: Option<TransientImage>,
    pub shadow_bindless_id: u32,
    pub shadow_resolution: u32,
    pub shadow_matrix: Mat4,
}

pub struct ShadowResult {
    pub cascaded: CascadedShadowResult,
    pub spot: SpotShadowResult,
}

pub struct CascadedShadows {
    mode: ShadowPipelineMode,
    main_pass: ShadowPass,
    terrain_pass: ShadowPass,
    cascade_buffer: StagedBuffer,
}

pub struct SpotShadows {
    mode: ShadowPipelineMode,
    main_pass: ShadowPass,
    terrain_pass: ShadowPass,
    enabled: bool,
    resolution: u32,
    spot_light: Option<SpotShadowLight>,
}

pub struct ShadowSystem {
    cascaded: CascadedShadows,
    spot: SpotShadows,
}

impl CascadedShadows {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        draw_builder: &GPUDrawBuilder,
        terrain_draw_builder: &GPUDrawBuilder,
        dynamic: &DynamicAllocator,
        cascade_buffer: StagedBuffer,
        info: ShadowPassInfo,
        mode: ShadowPipelineMode,
    ) -> Self {
        let main_pass = ShadowPass::new(ctx, state, draw_builder, dynamic, info);
        let terrain_pass = ShadowPass::new(ctx, state, terrain_draw_builder, dynamic, info);

        Self {
            mode,
            main_pass,
            terrain_pass,
            cascade_buffer,
        }
    }

    pub fn resolution(&self) -> u32 {
        self.main_pass.resolution()
    }

    pub fn resolution_mut(&mut self) -> &mut u32 {
        self.main_pass.resolution_mut()
    }

    pub fn cascades(&self) -> ShadowCascadeSettings {
        self.main_pass.cascades()
    }

    pub fn cascades_mut(&mut self) -> &mut ShadowCascadeSettings {
        self.main_pass.cascades_mut()
    }

    pub fn cascade_buffer(&self) -> &StagedBuffer {
        &self.cascade_buffer
    }

    pub fn depth_clear_value(&self) -> ClearValue {
        self.main_pass.depth_clear_value()
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
        let cascades = self.main_pass.cascades();
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
        graph: &mut RenderGraph,
        state: &BindlessState,
        dynamic: &mut DynamicAllocator,
        draw_builder: &mut GPUDrawBuilder,
        environment: &mut EnvironmentRenderer,
        camera: &furikake::types::Camera,
        view_idx: u32,
    ) -> CascadedShadowResult {
        let bump = crate::render::global_bump().get();
        let s = self as *mut Self;
        let shadow_resolution = bump.alloc(self.main_pass.resolution());
        let cascade_data = unsafe {
            (*s).compute_shadow_cascade_data(camera, environment.primary_light_direction())
        };
        let cascade_count = cascade_data.count.max(1);
        let grid_x = bump.alloc(if cascade_count > 1 { 2 } else { 1 });
        let grid_y = bump.alloc(if cascade_count > 2 { 2 } else { 1 });
        {
            let cascade_info = &mut self.cascade_buffer.as_slice_mut::<ShadowCascadeInfo>()[0];
            cascade_info.splits = cascade_data.splits;
            cascade_info.matrices = cascade_data.matrices;
        }

        let shadow_atlas_width = *shadow_resolution * *grid_x;
        let shadow_atlas_height = *shadow_resolution * *grid_y;
        let shadow_viewport = bump.alloc(Viewport {
            area: dashi::FRect2D {
                x: 0.0,
                y: 0.0,
                w: shadow_atlas_width as f32,
                h: shadow_atlas_height as f32,
            },
            scissor: Rect2D {
                x: 0,
                y: 0,
                w: shadow_atlas_width,
                h: shadow_atlas_height,
            },
            ..Default::default()
        });

        let shadow_map = graph.make_image(&ImageInfo {
            debug_name: &format!("[MESHI {}] Shadow Map {view_idx}", self.mode.label()),
            dim: [shadow_atlas_width, shadow_atlas_height, 1],
            layers: 1,
            format: Format::D24S8,
            mip_levels: 1,
            samples: self.main_pass.sample_count(),
            initial_data: None,
            ..Default::default()
        });
        let mut shadow_map = shadow_map;
        shadow_map.view.aspect = AspectMask::Depth;

        graph.add_compute_pass(|cmd| {
            let cmd = cmd
                .combine(draw_builder.build_draws(super::BIN_SHADOW, view_idx))
                .combine(environment.build_terrain_draws(TERRAIN_DRAW_BIN, view_idx))
                .sync(SyncPoint::ComputeToGraphics, Scope::AllCommonReads);

            cmd.end()
        });

        let shadow_clear: [Option<ClearValue>; 8] = [None; 8];
        graph.add_subpass(
            &SubpassInfo {
                name: Some("[MESHI] SHADOW PASS".to_string()),
                viewport: *shadow_viewport,
                color_attachments: [None; 8],
                depth_attachment: Some(shadow_map.view),
                clear_values: shadow_clear,
                depth_clear: Some(self.main_pass.depth_clear_value()),
            },
            |mut cmd| {
                let indices = state
                    .binding("meshi_bindless_indices")
                    .expect("Bindless indices not available")
                    .binding();

                let indices_handle = match indices {
                    ReservedBinding::TableBinding {
                        binding: _,
                        resources,
                    } => match resources[0].resource {
                        ShaderResource::StorageBuffer(view) => Some(view.handle),
                        _ => None,
                    },
                    _ => None,
                };

                let Some(indices_handle) = indices_handle else {
                    return cmd;
                };

                let (terrain_draw_list, terrain_draw_count) = environment
                    .terrain_draw_builder()
                    .map(|builder| (builder.draw_list(), builder.draw_count()))
                    .unwrap_or((Handle::default(), 0));

                let cascade_count = cascade_data.count.max(1);
                let main_per_draw = draw_builder.per_draw_data();
                self.main_pass.set_per_draw_data(main_per_draw);
                for cascade_index in 0..cascade_count as usize {
                    let tile_x = (cascade_index as u32) % *grid_x;
                    let tile_y = (cascade_index as u32) / *grid_x;
                    let cascade_viewport = Viewport {
                        area: dashi::FRect2D {
                            x: (tile_x * *shadow_resolution) as f32,
                            y: (tile_y * *shadow_resolution) as f32,
                            w: *shadow_resolution as f32,
                            h: *shadow_resolution as f32,
                        },
                        scissor: Rect2D {
                            x: tile_x * *shadow_resolution,
                            y: tile_y * *shadow_resolution,
                            w: *shadow_resolution,
                            h: *shadow_resolution,
                        },
                        ..Default::default()
                    };
                    cmd = cmd.combine(self.main_pass.record(
                        &cascade_viewport,
                        dynamic,
                        cascade_data.matrices[cascade_index],
                        indices_handle,
                        draw_builder.draw_list(),
                        draw_builder.draw_count(),
                    ));
                    if terrain_draw_count > 0 {
                        cmd = cmd.combine(self.terrain_pass.record(
                            &cascade_viewport,
                            dynamic,
                            cascade_data.matrices[cascade_index],
                            indices_handle,
                            terrain_draw_list,
                            terrain_draw_count,
                        ));
                    }
                }

                cmd
            },
        );

        CascadedShadowResult {
            shadow_map,
            shadow_resolution: *shadow_resolution,
            cascade_data: cascade_data.clone(),
        }
    }
}

impl SpotShadows {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        draw_builder: &GPUDrawBuilder,
        terrain_draw_builder: &GPUDrawBuilder,
        dynamic: &DynamicAllocator,
        info: ShadowPassInfo,
        mode: ShadowPipelineMode,
    ) -> Self {
        let resolution = info.resolution;
        let main_pass = ShadowPass::new(ctx, state, draw_builder, dynamic, info);
        let terrain_pass = ShadowPass::new(ctx, state, terrain_draw_builder, dynamic, info);
        Self {
            mode,
            main_pass,
            terrain_pass,
            enabled: true,
            resolution,
            spot_light: None,
        }
    }

    pub fn enabled_mut(&mut self) -> &mut bool {
        &mut self.enabled
    }

    pub fn resolution_mut(&mut self) -> &mut u32 {
        &mut self.resolution
    }

    pub fn set_light(&mut self, light: Option<SpotShadowLight>) {
        self.spot_light = light;
    }

    pub fn light_handle(&self) -> Option<Handle<furikake::types::Light>> {
        self.spot_light.map(|light| light.handle)
    }

    pub fn update_light_state(&self, state: &mut BindlessState) {
        let Some(spot_light) = self.spot_light else {
            return;
        };
        let enabled_value = if self.enabled { 1.0 } else { 0.0 };
        let handle = spot_light.handle;
        let _ = state.reserved_mut(
            "meshi_bindless_lights",
            |lights: &mut furikake::reservations::bindless_lights::ReservedBindlessLights| {
                if handle.valid() {
                    lights.light_mut(handle).extra.y = enabled_value;
                }
            },
        );
    }
    fn clip_space_fixup() -> Mat4 {
        Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 1.0,
        ])
    }

    fn spot_shadow_matrix(light: &LightInfo) -> Mat4 {
        let position = Vec3::new(light.pos_x, light.pos_y, light.pos_z);
        let mut direction = Vec3::new(light.dir_x, light.dir_y, light.dir_z);
        if direction.length_squared() > 0.0 {
            direction = direction.normalize();
        } else {
            direction = Vec3::NEG_Z;
        }
        let up = if direction.abs().dot(Vec3::Y) > 0.9 {
            Vec3::X
        } else {
            Vec3::Y
        };
        let view = Mat4::look_at_rh(position, position + direction, up);
        let outer = light.spot_outer_angle_rad.max(0.01);
        let fov = (outer * 2.0).clamp(0.01, std::f32::consts::PI - 0.01);
        let near = 0.1;
        let far = if light.range > near {
            light.range
        } else {
            1000.0
        };
        let proj = Mat4::perspective_rh(fov, 1.0, near, far);
        Self::clip_space_fixup() * proj * view
    }

    pub fn process(
        &mut self,
        graph: &mut RenderGraph,
        state: &BindlessState,
        dynamic: &mut DynamicAllocator,
        draw_builder: &GPUDrawBuilder,
        environment: &mut EnvironmentRenderer,
        view_idx: u32,
    ) -> SpotShadowResult {
        let bump = crate::render::global_bump().get();

        let mut shadow_bindless_id = 0u32;
        let mut shadow_resolution = 0u32;
        let mut shadow_matrix = Mat4::IDENTITY;
        let mut shadow_map = None;

        if self.enabled {
            if let Some(spot_light) = self.spot_light {
                shadow_resolution = self.resolution.max(1);
                shadow_matrix = Self::spot_shadow_matrix(&spot_light.info);
                let spot_shadow_image = graph.make_image(&ImageInfo {
                    debug_name: &format!(
                        "[MESHI {}] Spot Shadow Map {view_idx}",
                        self.mode.label()
                    ),
                    dim: [shadow_resolution, shadow_resolution, 1],
                    layers: 1,
                    format: Format::D24S8,
                    mip_levels: 1,
                    samples: self.main_pass.sample_count(),
                    initial_data: None,
                    ..Default::default()
                });
                let mut spot_shadow_image = spot_shadow_image;
                spot_shadow_image.view.aspect = AspectMask::Depth;
                shadow_bindless_id = spot_shadow_image.bindless_id.unwrap_or(0) as u32;
                shadow_map = Some(spot_shadow_image);
            }
        }

        if let Some(spot_shadow_map) = shadow_map.as_ref() {
            let spot_shadow_viewport = bump.alloc(Viewport {
                area: dashi::FRect2D {
                    x: 0.0,
                    y: 0.0,
                    w: shadow_resolution as f32,
                    h: shadow_resolution as f32,
                },
                scissor: Rect2D {
                    x: 0,
                    y: 0,
                    w: shadow_resolution,
                    h: shadow_resolution,
                },
                ..Default::default()
            });

            let shadow_clear: [Option<ClearValue>; 8] = [None; 8];
            graph.add_subpass(
                &SubpassInfo {
                    name: Some("[MESHI] SPOT SHADOW PASS".to_string()),
                    viewport: *spot_shadow_viewport,
                    color_attachments: [None; 8],
                    depth_attachment: Some(spot_shadow_map.view),
                    clear_values: shadow_clear,
                    depth_clear: Some(self.main_pass.depth_clear_value()),
                },
                |mut cmd| {
                    let indices = state
                        .binding("meshi_bindless_indices")
                        .expect("Bindless indices not available")
                        .binding();

                    let indices_handle = match indices {
                        ReservedBinding::TableBinding {
                            binding: _,
                            resources,
                        } => match resources[0].resource {
                            ShaderResource::StorageBuffer(view) => Some(view.handle),
                            _ => None,
                        },
                        _ => None,
                    };

                    let Some(indices_handle) = indices_handle else {
                        return cmd;
                    };

                    let (terrain_draw_list, terrain_draw_count) = environment
                        .terrain_draw_builder()
                        .map(|builder| (builder.draw_list(), builder.draw_count()))
                        .unwrap_or((Handle::default(), 0));

                    cmd = cmd.combine(self.main_pass.record(
                        spot_shadow_viewport,
                        dynamic,
                        shadow_matrix,
                        indices_handle,
                        draw_builder.draw_list(),
                        draw_builder.draw_count(),
                    ));
                    if terrain_draw_count > 0 {
                        cmd = cmd.combine(self.terrain_pass.record(
                            spot_shadow_viewport,
                            dynamic,
                            shadow_matrix,
                            indices_handle,
                            terrain_draw_list,
                            terrain_draw_count,
                        ));
                    }

                    cmd
                },
            );
        }

        SpotShadowResult {
            shadow_map,
            shadow_bindless_id,
            shadow_resolution,
            shadow_matrix,
        }
    }
}

impl ShadowSystem {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        draw_builder: &GPUDrawBuilder,
        terrain_draw_builder: &GPUDrawBuilder,
        dynamic: &DynamicAllocator,
        cascade_buffer: StagedBuffer,
        info: ShadowPassInfo,
        mode: ShadowPipelineMode,
    ) -> Self {
        let cascaded = CascadedShadows::new(
            ctx,
            state,
            draw_builder,
            terrain_draw_builder,
            dynamic,
            cascade_buffer,
            info,
            mode,
        );
        let spot = SpotShadows::new(
            ctx,
            state,
            draw_builder,
            terrain_draw_builder,
            dynamic,
            info,
            mode,
        );
        Self { cascaded, spot }
    }

    pub fn pre_compute(&mut self) -> CommandStream<Executable> {
        CommandStream::new()
            .begin()
            .combine(self.cascaded.cascade_buffer().sync_up())
            .end()
    }

    pub fn post_compute(&mut self) -> CommandStream<Executable> {
        CommandStream::new().begin().end()
    }

    pub fn cascades(&self) -> ShadowCascadeSettings {
        self.cascaded.cascades()
    }

    pub fn cascades_mut(&mut self) -> &mut ShadowCascadeSettings {
        self.cascaded.cascades_mut()
    }

    pub fn resolution(&self) -> u32 {
        self.cascaded.resolution()
    }

    pub fn resolution_mut(&mut self) -> &mut u32 {
        self.cascaded.resolution_mut()
    }

    pub fn spot_enabled_mut(&mut self) -> &mut bool {
        self.spot.enabled_mut()
    }

    pub fn spot_resolution_mut(&mut self) -> &mut u32 {
        self.spot.resolution_mut()
    }

    pub fn set_spot_light(&mut self, light: Option<SpotShadowLight>) {
        self.spot.set_light(light);
    }

    pub fn spot_light_handle(&self) -> Option<Handle<furikake::types::Light>> {
        self.spot.light_handle()
    }

    pub fn update_spot_light_state(&self, state: &mut BindlessState) {
        self.spot.update_light_state(state);
    }

    pub fn cascade_buffer(&self) -> &StagedBuffer {
        self.cascaded.cascade_buffer()
    }

    pub fn depth_clear_value(&self) -> ClearValue {
        self.cascaded.depth_clear_value()
    }

    pub fn process(
        &mut self,
        graph: &mut RenderGraph,
        state: &BindlessState,
        dynamic: &mut DynamicAllocator,
        draw_builder: &mut GPUDrawBuilder,
        environment: &mut EnvironmentRenderer,
        camera: &furikake::types::Camera,
        view_idx: u32,
    ) -> ShadowResult {
        let bump = crate::render::global_bump().get();
        let _frame_marker = bump.alloc(0u8);
        let cascaded = self.cascaded.process(
            graph,
            state,
            dynamic,
            draw_builder,
            environment,
            camera,
            view_idx,
        );
        let spot = self
            .spot
            .process(graph, state, dynamic, draw_builder, environment, view_idx);

        ShadowResult { cascaded, spot }
    }
}
