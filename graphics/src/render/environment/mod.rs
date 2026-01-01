pub mod sky;

use dashi::{ClearValue, Context, DynamicAllocator, Format, ImageView, SampleCount, Viewport};
use furikake::{BindlessState, types::Camera};
use tare::graph::RenderGraph;

use sky::SkyRenderer;

#[derive(Clone, Copy)]
pub struct EnvironmentRendererInfo {
    pub color_format: Format,
    pub sample_count: SampleCount,
    pub use_depth: bool,
}

pub struct EnvironmentRenderer {
    dynamic: DynamicAllocator,
    time: f32,
    sky: SkyRenderer,
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
        let sky = SkyRenderer::new(ctx, state, info, &dynamic);
        Self {
            dynamic,
            time: 0.0,
            sky,
            color_format: info.color_format,
            sample_count: info.sample_count,
        }
    }

    pub fn reset(&mut self) {
        self.dynamic.reset();
    }

    pub fn render(
        &mut self,
        graph: &mut RenderGraph,
        viewport: Viewport,
        color: ImageView,
        depth: Option<ImageView>,
        camera: dashi::Handle<Camera>,
        delta_time: f32,
    ) {
        self.time += delta_time;

        let mut attachments: [Option<ImageView>; 8] = [None; 8];
        attachments[0] = Some(color);

        let clear_values: [Option<ClearValue>; 8] = [None; 8];

        self.sky.add_pass(
            graph,
            &mut self.dynamic,
            viewport,
            attachments,
            clear_values,
            depth,
            camera,
            self.time,
            delta_time,
        );
    }

    pub fn color_format(&self) -> Format {
        self.color_format
    }

    pub fn sample_count(&self) -> SampleCount {
        self.sample_count
    }
}
