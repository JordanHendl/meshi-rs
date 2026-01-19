pub mod ocean;
pub mod sky;
pub mod terrain;
pub mod clouds;

use dashi::cmd::PendingGraphics;
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
    pub time_scale: f32,
    pub ocean: Option<ocean::OceanFrameSettings>,
    pub skybox: Option<sky::SkyboxFrameSettings>,
}

impl Default for EnvironmentFrameSettings {
    fn default() -> Self {
        Self {
            delta_time: 0.0,
            time_scale: 1.0,
            ocean: None,
            skybox: None,
        }
    }
}

pub struct EnvironmentRenderer {
    dynamic: DynamicAllocator,
    time: f32,
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
        let scaled_delta = settings.delta_time * settings.time_scale;
        self.time += scaled_delta;
        self.last_delta_time = scaled_delta;

        if let Some(ocean) = settings.ocean {
            self.ocean.update(ocean);
        }

        if let Some(skybox) = settings.skybox {
            self.sky.update_skybox(skybox);
        }
    }

    pub fn update_ocean(&mut self, settings: ocean::OceanFrameSettings) {
        self.ocean.update(settings);
    }

    pub fn update_skybox(&mut self, settings: sky::SkyboxFrameSettings) {
        self.sky.update_skybox(settings);
    }

    pub fn update_terrain(&mut self, settings: terrain::TerrainFrameSettings) {
        self.terrain.update(settings);
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
            .combine(self.ocean.record_draws(
                viewport,
                &mut self.dynamic,
                camera,
                self.time,
            ))
           // .combine(self.terrain.record_draws(viewport, &mut self.dynamic))
    }

    pub fn color_format(&self) -> Format {
        self.color_format
    }

    pub fn sample_count(&self) -> SampleCount {
        self.sample_count
    }
}
