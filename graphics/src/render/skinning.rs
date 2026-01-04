use bento::builder::CSOBuilder;
use dashi::{
    BufferInfo, BufferUsage, CommandStream, CommandQueueInfo2, Context, MemoryVisibility,
    ShaderResource, UsageBits,
    driver::command::Dispatch,
    execution::CommandRing,
};
use furikake::BindlessAnimationRegistry;
use furikake::{
    BindlessState,
    reservations::ReservedBinding,
    types::{AnimationState as FurikakeAnimationState, SkeletonHeader},
};
use crate::SkinnedModelInfo;
use noren::meta::DeviceModel;
use resource_pool::Handle;
use tare::utils::StagedBuffer;

#[derive(Clone, Copy, Default)]
pub struct SkinningInfo {
    pub skeleton: Handle<SkeletonHeader>,
    pub animation_state: Handle<FurikakeAnimationState>,
}

#[derive(Clone)]
pub struct SkinnedModelData {
    pub info: SkinnedModelInfo,
    pub animation_state: Handle<FurikakeAnimationState>,
    animation_dirty: bool,
}

impl SkinnedModelData {
    pub fn new(info: SkinnedModelInfo, bindless: &mut BindlessState) -> Self {
        let animation_state = bindless.register_animation_state();
        Self {
            info,
            animation_state,
            animation_dirty: true,
        }
    }

    pub fn model(&self) -> &DeviceModel {
        &self.info.model
    }

    pub fn skinning_info(&self) -> SkinningInfo {
        let skeleton = self
            .info
            .model
            .rig
            .as_ref()
            .map(|rig| rig.skeleton)
            .unwrap_or_default();

        SkinningInfo {
            skeleton,
            animation_state: self.animation_state,
        }
    }

    pub fn mark_animation_dirty(&mut self) {
        self.animation_dirty = true;
    }

    pub fn clear_animation_dirty(&mut self) {
        self.animation_dirty = false;
    }
}

pub struct SkinningDispatcher {
    queue: CommandRing,
    pipeline: Option<bento::builder::CSO>,
    dispatches: StagedBuffer,
}

pub const MAX_SKINNING_DISPATCHES: usize = 1024;

impl SkinningDispatcher {
    pub fn new(ctx: &mut Context, state: &BindlessState) -> Self {
        let dispatches = StagedBuffer::new(
            ctx,
            BufferInfo {
                debug_name: "[MESHI] Skinning Dispatches",
                byte_size: (std::mem::size_of::<SkinningDispatch>() as u32
                    * MAX_SKINNING_DISPATCHES as u32)
                    .max(256),
                visibility: MemoryVisibility::CpuAndGpu,
                usage: BufferUsage::STORAGE,
                initial_data: None,
            },
        );

        let animation_binding = state.binding("meshi_bindless_animations");
        let skinning_binding = state.binding("meshi_bindless_skinning");
        let pipeline = if let (Ok(animation_binding), Ok(skinning_binding)) =
            (animation_binding, skinning_binding)
        {
            let ReservedBinding::TableBinding {
                resources: animation_resources,
                ..
            } = animation_binding.binding();
            let ReservedBinding::TableBinding {
                resources: skinning_resources,
                ..
            } = skinning_binding.binding();

            CSOBuilder::new()
                .shader(Some(include_str!("shaders/skinning.comp.glsl").as_bytes()))
                .add_variable(
                    "skinning_dispatches",
                    ShaderResource::StorageBuffer(dispatches.device().into()),
                )
                .add_variable("meshi_bindless_animations", animation_resources[0].resource.clone())
                .add_variable("meshi_bindless_skinning", skinning_resources[0].resource.clone())
                .build(ctx)
                .ok()
        } else {
            None
        };

        let queue = ctx
            .make_command_ring(&CommandQueueInfo2 {
                debug_name: "[SKINNING]",
                parent: None,
                queue_type: dashi::QueueType::Compute,
            })
            .expect("create skinning compute queue");

        Self {
            queue,
            pipeline,
            dispatches,
        }
    }

    pub fn update(
        &mut self,
        dispatches: &[SkinningDispatch],
    ) {
        let Some(pipeline) = self.pipeline.as_ref() else {
            return;
        };

        let buffer = self.dispatches.as_slice_mut::<SkinningDispatch>();
        if buffer.is_empty() {
            return;
        }

        let dispatch_count = dispatches.len().min(buffer.len());
        buffer[..dispatch_count].copy_from_slice(&dispatches[..dispatch_count]);

        if dispatch_count == 0 {
            return;
        }

        self.queue
            .record(|c| {
                CommandStream::new()
                    .begin()
                    .combine(self.dispatches.sync_up())
                    .prepare_buffer(self.dispatches.device().handle, UsageBits::COMPUTE_SHADER)
                    .dispatch(&Dispatch {
                        x: dispatch_count as u32,
                        y: 1,
                        z: 1,
                        pipeline: pipeline.handle,
                        bind_tables: pipeline.tables(),
                        dynamic_buffers: Default::default(),
                    })
                    .unbind_pipeline()
                    .end()
                    .append(c)
                    .expect("record skinning compute");
            })
            .expect("record skinning commands");

        self.queue.submit(&Default::default()).expect("submit skinning");
        self.queue.wait_all().expect("wait skinning");
    }
}

pub fn unregister_skinned_model(bindless: &mut BindlessState, skinned: &SkinnedModelData) {
    bindless.unregister_animation_state(skinned.animation_state);
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkinningDispatch {
    animation_state_id: u32,
    clip_handle: u32,
    skeleton_handle: u32,
    reset_time: u32,
    time_seconds: f32,
    playback_rate: f32,
    delta_time: f32,
    looping: u32,
}

impl SkinningDispatch {
    pub fn from_model(model: &SkinnedModelData, delta_time: f32) -> Self {
        let clip_handle = model
            .info
            .model
            .rig
            .as_ref()
            .and_then(|rig| rig.animation);
        let skeleton_handle = model
            .info
            .model
            .rig
            .as_ref()
            .map(|rig| rig.skeleton)
            .unwrap_or_default();

        Self {
            animation_state_id: if model.animation_state.valid() {
                model.animation_state.slot as u32
            } else {
                u16::MAX as u32
            },
            clip_handle: clip_handle
                .filter(|handle| handle.valid())
                .map(|handle| handle.slot as u32)
                .unwrap_or(u16::MAX as u32),
            skeleton_handle: if skeleton_handle.valid() {
                skeleton_handle.slot as u32
            } else {
                u16::MAX as u32
            },
            reset_time: model.animation_dirty as u32,
            time_seconds: model.info.animation.time_seconds,
            playback_rate: model.info.animation.speed,
            delta_time,
            looping: model.info.animation.looping as u32,
        }
    }
}
