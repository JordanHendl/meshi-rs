pub mod clouds;
pub mod ocean;
pub mod sky;
pub mod terrain;

use dashi::cmd::{Executable, PendingGraphics};
use dashi::{
    Buffer, CommandStream, Context, DynamicAllocator, Format, Handle, ImageView, SampleCount,
    Viewport,
};
use furikake::{BindlessState, types::Camera};
use glam::{Mat4, Vec3, Vec4};
use noren::{DB, RDBFile};

use crate::CloudSettings;
use crate::render::gpu_draw_builder::GPUDrawBuilder;
use clouds::CloudRenderer;
use ocean::OceanRenderer;
use sky::SkyRenderer;
use terrain::TerrainRenderer;

#[derive(Clone)]
pub struct EnvironmentRendererInfo {
    pub initial_viewport: Viewport,
    pub color_format: Format,
    pub sample_count: SampleCount,
    pub use_depth: bool,
    pub skybox: sky::SkyboxInfo,
    pub ocean: ocean::OceanInfo,
    pub terrain: terrain::TerrainInfo,
    pub cloud_depth_view: Option<ImageView>,
}

pub struct EnvironmentFrameSettings {
    pub delta_time: f32,
    pub time_seconds: Option<f32>,
    pub time_scale: f32,
    pub paused: bool,
    pub ocean: Option<ocean::OceanFrameSettings>,
    pub skybox: Option<sky::SkyboxFrameSettings>,
    pub sky: Option<sky::SkyFrameSettings>,
}

impl Default for EnvironmentFrameSettings {
    fn default() -> Self {
        Self {
            delta_time: 0.0,
            time_seconds: None,
            time_scale: 1.0,
            paused: false,
            ocean: None,
            skybox: None,
            sky: None,
        }
    }
}

pub struct EnvironmentRenderer {
    dynamic: DynamicAllocator,
    time: f32,
    time_scale: f32,
    paused: bool,
    last_delta_time: f32,
    sky: SkyRenderer,
    clouds: Option<CloudRenderer>,
    cloud_settings: CloudSettings,
    ocean: OceanRenderer,
    terrain: TerrainRenderer,
    color_format: Format,
    sample_count: SampleCount,
}

impl EnvironmentRenderer {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        info: EnvironmentRendererInfo,
    ) -> Self {
        let dynamic = ctx
            .make_dynamic_allocator(&Default::default())
            .expect("Failed to make environment dynamic allocator");

        let sky = SkyRenderer::new(ctx, state, &info, &dynamic);
        let clouds = info.cloud_depth_view.map(|depth_view| {
            CloudRenderer::new(
                ctx,
                state,
                &info.initial_viewport,
                depth_view,
                info.sample_count,
                sky.environment_cubemap_view(),
            )
        });
        let ocean = OceanRenderer::new(ctx, state, &info, &dynamic, sky.environment_cubemap_view());
        let terrain = TerrainRenderer::new(ctx, state, &info, &dynamic);

        Self {
            color_format: info.color_format,
            sample_count: info.sample_count.clone(),
            time: 0.0,
            time_scale: 1.0,
            paused: false,
            last_delta_time: 0.0,
            sky,
            clouds,
            cloud_settings: CloudSettings::default(),
            ocean,
            terrain,
            dynamic,
        }
    }

    pub fn reset(&mut self) {
        self.dynamic.reset();
    }

    pub fn pre_compute(&mut self, ctx: &mut Context) -> CommandStream<Executable> {
        CommandStream::new()
            .begin()
            .combine(self.sky.pre_compute(ctx))
            .combine(self.ocean.pre_compute())
            .combine(self.terrain.pre_compute())
            .end()
    }

    pub fn post_compute(&mut self) -> CommandStream<Executable> {
        CommandStream::new()
            .begin()
            .combine(self.sky.post_compute())
            .combine(self.ocean.post_compute())
            .combine(self.terrain.post_compute())
            .end()
    }

    pub fn update(&mut self, settings: EnvironmentFrameSettings) {
        let bump = crate::render::global_bump().get();
        let _frame_marker = bump.alloc(0u8);
        let previous_time = self.time;
        self.time_scale = settings.time_scale;
        self.paused = settings.paused;

        if let Some(time_seconds) = settings.time_seconds {
            self.time = time_seconds;
            self.last_delta_time = self.time - previous_time;
        } else if self.paused {
            self.last_delta_time = 0.0;
        } else {
            let scaled_delta = settings.delta_time * self.time_scale;
            self.time += scaled_delta;
            self.last_delta_time = scaled_delta;
        }

        if let Some(ocean) = settings.ocean {
            self.ocean.update(ocean);
        }

        if let Some(skybox) = settings.skybox {
            self.sky.update_skybox(skybox);
        }

        if let Some(sky) = settings.sky {
            self.sky.update_sky(sky);
        }
    }

    pub fn update_ocean(&mut self, settings: ocean::OceanFrameSettings) {
        self.ocean.update(settings);
    }

    pub fn update_skybox(&mut self, settings: sky::SkyboxFrameSettings) {
        self.sky.update_skybox(settings);
    }

    pub fn update_sky(&mut self, settings: sky::SkyFrameSettings) {
        self.sky.update_sky(settings);
    }

    pub fn sun_direction(&self) -> Vec3 {
        self.sky.sun_direction()
    }

    pub fn primary_light_direction(&self) -> Vec3 {
        self.sky.primary_light_direction()
    }

    pub fn prepare_sky_cubemap(
        &mut self,
        ctx: &mut Context,
        state: &mut BindlessState,
        viewport: &Viewport,
        camera: dashi::Handle<Camera>,
    ) -> Option<sky::SkyCubemapPass> {
        self.sky.prepare_cubemap_pass(ctx, state, viewport, camera)
    }

    pub fn render_sky_cubemap_face(
        &mut self,
        viewport: &Viewport,
        face_index: usize,
    ) -> CommandStream<PendingGraphics> {
        self.sky
            .record_cubemap_face(viewport, &mut self.dynamic, face_index)
    }

    pub fn update_terrain(
        &mut self,
        camera: Handle<Camera>,
        state: &mut BindlessState,
    ) {
        self.terrain.update(camera, state);
    }

    pub fn initialize_terrain_deferred(
        &mut self,
        ctx: &mut Context,
        state: &mut BindlessState,
        sample_count: SampleCount,
        cull_results: Handle<Buffer>,
        bin_counts: Handle<Buffer>,
        num_bins: u32,
        dynamic: &DynamicAllocator,
    ) {
        self.terrain.initialize_deferred(
            ctx,
            state,
            sample_count,
            cull_results,
            bin_counts,
            num_bins,
            dynamic,
        );
    }

    pub fn initialize_database(&mut self, db: &mut DB) {
        self.terrain.initialize_database(db);
    }

    pub fn register_debug(&mut self) {
        self.sky.register_debug();
        if let Some(clouds) = self.clouds.as_mut() {
            clouds.register_debug();
        }
        self.ocean.register_debug();
    }

    pub fn set_terrain_rdb(&mut self, rdb: &mut RDBFile, project_key: &str) {
        self.terrain.set_rdb(rdb, project_key);
    }

    pub fn set_terrain_project_key(&mut self, project_key: &str) {
        self.terrain.set_project_key(project_key);
    }

    pub fn build_terrain_draws(&mut self, bin: u32, view: u32) -> CommandStream<Executable> {
        self.terrain.build_deferred_draws(bin, view)
    }

    pub fn terrain_draw_builder(&self) -> Option<&GPUDrawBuilder> {
        self.terrain.draw_builder()
    }

    pub fn terrain_draw_info(&self) -> Option<terrain::TerrainDrawInfo> {
        self.terrain.draw_info()
    }

    pub fn record_terrain_draws(
        &mut self,
        viewport: &Viewport,
        dynamic: &mut DynamicAllocator,
        camera: Handle<Camera>,
        indices_handle: Handle<Buffer>,
    ) -> CommandStream<PendingGraphics> {
        self.terrain
            .record_deferred_draws(viewport, dynamic, camera, indices_handle)
    }

    pub fn set_time(&mut self, time_seconds: f32) {
        self.time = time_seconds;
    }

    pub fn set_time_scale(&mut self, time_scale: f32) {
        self.time_scale = time_scale;
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn time_seconds(&self) -> f32 {
        self.time
    }

    pub fn time_scale(&self) -> f32 {
        self.time_scale
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn render(
        &mut self,
        viewport: &Viewport,
        camera: dashi::Handle<Camera>,
        scene_color: Option<dashi::ImageView>,
        scene_depth: Option<dashi::ImageView>,
        shadow_map: Option<dashi::ImageView>,
        shadow_cascade_count: u32,
        shadow_resolution: u32,
        shadow_splits: Vec4,
        shadow_matrices: [Mat4; 4],
    ) -> CommandStream<PendingGraphics> {
        self.ocean
            .set_environment_map(self.sky.environment_cubemap_view());
        self.ocean.set_scene_textures(scene_color, scene_depth);
        self.ocean.set_shadow_map(
            shadow_map,
            shadow_cascade_count,
            shadow_resolution,
            shadow_splits,
            shadow_matrices,
        );
        let cloud_shadow_info = self
            .clouds
            .as_ref()
            .and_then(|clouds| clouds.shadow_map_info());
        if let Some(info) = cloud_shadow_info {
            self.ocean.set_cloud_shadow_map(
                Some(info.shadow_buffer),
                info.shadow_cascade_count,
                info.shadow_resolution,
                Vec4::from(info.shadow_cascade_splits),
                info.shadow_cascade_extents,
                info.shadow_cascade_resolutions,
                info.shadow_cascade_offsets,
            );
        } else {
            self.ocean
                .set_cloud_shadow_map(None, 0, 0, Vec4::ZERO, [0.0; 4], [0; 4], [0; 4]);
        }
        CommandStream::<PendingGraphics>::subdraw()
            .combine(self.sky.record_draws(
                viewport,
                &mut self.dynamic,
                camera,
                self.time,
                self.last_delta_time,
            ))
            .combine(
                self.clouds
                    .as_mut()
                    .map(|clouds| clouds.record_composite(viewport))
                    .unwrap_or_else(CommandStream::<PendingGraphics>::subdraw),
            )
            .combine(
                self.ocean
                    .record_draws(viewport, &mut self.dynamic, camera, self.time),
            )
        // .combine(self.terrain.record_draws(viewport, &mut self.dynamic))
    }

    pub fn record_compute(&mut self, ctx: &mut Context) -> CommandStream<Executable> {
        CommandStream::new()
            .begin()
            .combine(
                self.sky
                    .record_compute(ctx, self.time, self.last_delta_time),
            )
            .combine(self.ocean.record_compute(&mut self.dynamic, self.time))
            .combine(self.terrain.record_compute(&mut self.dynamic))
            .end()
    }

    pub fn record_clouds_update(
        &mut self,
        ctx: &mut Context,
        state: &mut BindlessState,
        viewport: &Viewport,
        camera: Handle<Camera>,
        delta_time: f32,
    ) -> CommandStream<Executable> {
        if let Some(clouds) = self.clouds.as_mut() {
            CommandStream::new()
                .begin()
                .combine(clouds.pre_compute())
                .combine(clouds.update(ctx, state, viewport, camera, delta_time))
                .combine(clouds.post_compute())
                .end()
        } else {
            CommandStream::new().begin().end()
        }
    }

    pub fn cloud_settings(&self) -> CloudSettings {
        self.clouds
            .as_ref()
            .map(CloudRenderer::settings)
            .unwrap_or(self.cloud_settings)
    }

    pub fn set_cloud_settings(&mut self, settings: CloudSettings) {
        if let Some(clouds) = self.clouds.as_mut() {
            clouds.set_settings(settings);
        } else {
            self.cloud_settings = settings;
        }
    }

    pub fn set_cloud_weather_map(&mut self, view: Option<ImageView>) {
        if let Some(clouds) = self.clouds.as_mut() {
            clouds.set_authored_weather_map(view);
        }
    }

    pub fn cloud_timing_overlay_text(&self) -> String {
        self.clouds
            .as_ref()
            .map(CloudRenderer::timing_overlay_text)
            .unwrap_or_default()
    }

    pub fn environment_cubemap_view(&self) -> dashi::ImageView {
        self.sky.environment_cubemap_view()
    }

    pub fn color_format(&self) -> Format {
        self.color_format
    }

    pub fn sample_count(&self) -> SampleCount {
        self.sample_count
    }
}
