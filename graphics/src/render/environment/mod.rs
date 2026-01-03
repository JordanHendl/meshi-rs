pub mod ocean;
pub mod sky;
pub mod terrain;

use dashi::{ClearValue, Context, DynamicAllocator, Format, ImageView, SampleCount, Viewport};
use furikake::{BindlessState, types::Camera};
use tare::graph::{RenderGraph, SubpassInfo};

use ocean::OceanRenderer;
use sky::SkyRenderer;
use terrain::TerrainRenderer;

use super::ViewOutput;

#[derive(Clone)]
pub struct EnvironmentRendererInfo {
    pub color_format: Format,
    pub sample_count: SampleCount,
    pub use_depth: bool,
    pub skybox: sky::SkyboxInfo,
    pub ocean: ocean::OceanInfo,
    pub terrain: terrain::TerrainInfo,
}

pub struct EnvironmentRenderer {
    dynamic: DynamicAllocator,
    time: f32,
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
            sky,
            ocean,
            terrain,
            dynamic,
        }
    }

    pub fn reset(&mut self) {
        self.dynamic.reset();
    }

    pub fn update_ocean(&mut self, settings: ocean::OceanFrameSettings) {
        self.ocean.update(settings);
    }

    pub fn update_terrain(&mut self, settings: terrain::TerrainFrameSettings) {
        self.terrain.update(settings);
    }

    pub fn render(
        &mut self,
        graph: &mut RenderGraph,
        viewport: &Viewport,
        fb: ImageView,
        depth: Option<ImageView>,
        camera: dashi::Handle<Camera>,
        delta_time: f32,
    ) -> ViewOutput {
        self.time += delta_time;

        let mut attachments: [Option<ImageView>; 8] = [None; 8];
        attachments[0] = Some(fb);

        let clear_values: [Option<ClearValue>; 8] = [None; 8];

        let info = SubpassInfo {
            viewport: viewport.clone(),
            color_attachments: attachments,
            depth_attachment: depth,
            clear_values,
            depth_clear: None,
        };

        self.sky.add_pass(
            graph,
            viewport,
            &mut self.dynamic,
            info.clone(),
            camera,
            self.time,
            delta_time,
        );

        self.ocean.add_pass(
            graph,
            viewport,
            &mut self.dynamic,
            info.clone(),
            self.time,
        );

        self.terrain.add_pass(graph, viewport, &mut self.dynamic, info);

        ViewOutput {
            camera,
            image: fb,
            semaphore: Default::default(),
        }
    }

    pub fn color_format(&self) -> Format {
        self.color_format
    }

    pub fn sample_count(&self) -> SampleCount {
        self.sample_count
    }
}
