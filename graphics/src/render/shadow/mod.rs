mod cascaded;
mod spot;

use crate::{
    ShadowCascadeSettings,
    render::{SpotShadowLight, SubrendererInitInfo, SubrendererProcessInfo},
};
use dashi::{
    BufferInfo, BufferUsage, ClearValue, CommandStream, Context, MemoryVisibility, SampleCount,
    cmd::Executable,
};
use tare::utils::StagedBuffer;

pub use cascaded::{CascadedShadowResult, ShadowCascadeInfo};
pub use spot::SpotShadowResult;

#[derive(Clone, Copy, Debug)]
pub enum ShadowPipelineMode {
    Deferred,
    Forward,
}

impl ShadowPipelineMode {
    pub fn label(self) -> &'static str {
        match self {
            ShadowPipelineMode::Deferred => "DEFERRED",
            ShadowPipelineMode::Forward => "FORWARD",
        }
    }
}

pub struct ShadowResult {
    pub cascaded: CascadedShadowResult,
    pub spot: SpotShadowResult,
}

#[derive(Clone, Copy)]
pub struct ShadowSystemInfo {
    pub sample_count: SampleCount,
    pub cascades: ShadowCascadeSettings,
    pub cascaded_resolution: u32,
    pub spot_resolution: u32,
    pub depth_clear: ClearValue,
}

#[derive(Clone, Copy)]
pub struct ShadowProcessInfo {
    pub subrenderer: SubrendererProcessInfo,
    pub view_idx: u32,
    pub shadow_bin: u32,
    pub primary_light_direction: glam::Vec3,
    pub spot_light: Option<SpotShadowLight>,
}

pub struct ShadowSystem {
    cascaded: cascaded::CascadedShadows,
    spot: spot::SpotShadows,
}

impl ShadowSystem {
    pub fn new(
        init: SubrendererInitInfo,
        info: ShadowSystemInfo,
        mode: ShadowPipelineMode,
    ) -> Self {
        let SubrendererInitInfo {
            ctx,
            state,
            per_draw_buffer,
            per_scene_dynamic,
        } = init;

        let cascade_buffer = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[MESHI] Shadow Cascade Info",
                byte_size: (std::mem::size_of::<ShadowCascadeInfo>() as u32).max(256),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        );
        let cascaded = cascaded::CascadedShadows::new(
            ctx,
            state,
            per_draw_buffer,
            per_scene_dynamic.clone(),
            info,
            mode,
            cascade_buffer,
        );
        let spot =
            spot::SpotShadows::new(ctx, state, per_draw_buffer, per_scene_dynamic, info, mode);
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

    pub fn cascade_buffer_handle(&self) -> dashi::BufferView {
        self.cascaded.cascade_buffer().device()
    }

    pub fn process(&mut self, info: ShadowProcessInfo) -> ShadowResult {
        let cascaded = self.cascaded.process(info);
        let spot = self.spot.process(info);
        ShadowResult { cascaded, spot }
    }
}
