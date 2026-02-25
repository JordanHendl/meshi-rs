use crate::ShadowCascadeSettings;
use crate::render::utils::gpu_draw_builder::GPUDrawBuilder;
use crate::render::{SpotShadowLight, SubrendererDrawInfo};
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

pub struct ShadowResult {
    pub cascaded: CascadedShadowResult,
    pub spot: SpotShadowResult,
}

pub struct ShadowSystemInfo {}

pub struct ShadowSystem {
    cascaded: CascadedShadows,
    spot: SpotShadows,
}

impl ShadowSystem {
    pub fn new(
        ctx: &mut Context,
        state: &mut BindlessState,
        info: &ShadowSystemInfo,
        mode: ShadowPipelineMode,
    ) -> Self {
        let cascaded = CascadedShadows::new(ctx, info, mode);
        let spot = SpotShadows::new(ctx, info, mode);
        Self { cascaded, spot }
    }

    pub fn pre_compute(&mut self) -> CommandStream<Executable> {
        CommandStream::new()
            .begin()
            //            .combine(self.cascaded.cascade_buffer().sync_up())
            .end()
    }

    pub fn post_compute(&mut self) -> CommandStream<Executable> {
        CommandStream::new().begin().end()
    }

    pub fn resolution(&self) -> u32 {
        self.cascaded.resolution()
    }

    pub fn process(&mut self, info: &SubrendererDrawInfo) -> ShadowResult {
        let cascaded = self.cascaded.process(info);
        let spot = self.spot.process(info);
        ShadowResult { cascaded, spot }
    }
}
