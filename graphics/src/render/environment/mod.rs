pub mod clouds;
pub mod ocean;
pub mod sky;
pub mod terrain;

use dashi::cmd::{Executable, PendingGraphics};
use dashi::{CommandStream, Context, DynamicAllocator, Format, SampleCount, Viewport};
use furikake::{BindlessState, types::Camera};

use ocean::OceanRenderer;
use sky::SkyRenderer;
use terrain::TerrainRenderer;

#[derive(Clone)]
pub struct EnvironmentRendererInfo {
    pub color_format: Format,
    pub sample_count: SampleCount,
    pub use_depth: bool,
    pub skybox: sky::SkyboxInfo,
    pub ocean: ocean::OceanInfo,
    pub terrain: terrain::TerrainInfo,
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
        let ocean = OceanRenderer::new(ctx, state, &info, &dynamic);
        let terrain = TerrainRenderer::new(ctx, state, &info, &dynamic);

        Self {
            color_format: info.color_format,
            sample_count: info.sample_count.clone(),
            time: 0.0,
            time_scale: 1.0,
            paused: false,
            last_delta_time: 0.0,
            sky,
            ocean,
            terrain,
            dynamic,
        }
    }

    pub fn reset(&mut self) {
        self.dynamic.reset();
    }

    pub fn update(&mut self, settings: EnvironmentFrameSettings) {
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

    pub fn update_terrain(&mut self, settings: terrain::TerrainFrameSettings) {
        self.terrain.update(settings);
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
    ) -> CommandStream<PendingGraphics> {
        CommandStream::<PendingGraphics>::subdraw()
            .combine(self.sky.record_draws(
                viewport,
                &mut self.dynamic,
                camera,
                self.time,
                self.last_delta_time,
            ))
            .combine(
                self.ocean
                    .record_draws(viewport, &mut self.dynamic, camera, self.time),
            )
        // .combine(self.terrain.record_draws(viewport, &mut self.dynamic))
    }

    pub fn record_compute(&mut self, ctx: &mut Context) -> CommandStream<Executable> {
        CommandStream::new()
            .begin()
            .combine(self.sky.record_compute(ctx, self.time, self.last_delta_time))
            .combine(self.ocean.record_compute(&mut self.dynamic, self.time))
            //            .combine(self.terrain.record_compute(&mut self.dynamic))
            .end()
    }

    pub fn color_format(&self) -> Format {
        self.color_format
    }

    pub fn sample_count(&self) -> SampleCount {
        self.sample_count
    }
}
