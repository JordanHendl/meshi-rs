use crate::ShadowCascadeSettings;
use crate::render::SpotShadowLight;
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

pub struct SpotShadowResult {
    pub shadow_map: Option<TransientImage>,
    pub shadow_bindless_id: u32,
    pub shadow_resolution: u32,
    pub shadow_matrix: Mat4,
}

pub struct SpotShadows {
    mode: ShadowPipelineMode,
    enabled: bool,
    resolution: u32,
    spot_light: Option<SpotShadowLight>,
}

impl SpotShadows {
    pub fn new(
        ctx: &mut Context,
        info: ShadowSystemInfo,
        mode: ShadowPipelineMode,
    ) -> Self {
        Self {
            mode,
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
