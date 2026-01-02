pub mod sky;

use dashi::{ClearValue, Context, DynamicAllocator, Format, ImageInfo, ImageView, SampleCount, Viewport};
use furikake::{BindlessState, types::Camera};
use tare::graph::{RenderGraph, SubpassInfo};

use sky::SkyRenderer;

use super::ViewOutput;

#[derive(Clone)]
pub struct EnvironmentRendererInfo {
    pub color_format: Format,
    pub sample_count: SampleCount,
    pub use_depth: bool,
    pub skybox: sky::SkyboxInfo,
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

        Self {
            color_format: info.color_format,
            sample_count: info.sample_count.clone(),
            time: 0.0,
            sky: SkyRenderer::new(ctx, state, info, &dynamic),
            dynamic,
        }
    }

    pub fn reset(&mut self) {
        self.dynamic.reset();
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
