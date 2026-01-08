use crate::{AnimationState, SkinnedModelInfo};
use bento::builder::CSOBuilder;
use dashi::{
    BufferInfo, BufferUsage, CommandQueueInfo2, CommandStream, Context, MemoryVisibility,
    ShaderResource, UsageBits, driver::command::Dispatch, execution::CommandRing,
};
use furikake::BindlessAnimationRegistry;
use furikake::PSOBuilderFurikakeExt;
use furikake::{
    BindlessState,
    reservations::{
        bindless_joints::ReservedBindlessJoints, bindless_skeletons::ReservedBindlessSkeletons,
        per_obj_joints::{PerObjectJointAllocation, ReservedPerObjJoints},
    },
    types::{AnimationState as FurikakeAnimationState, JointTransform, SkeletonHeader},
};
use noren::meta::DeviceModel;
use resource_pool::{Handle, resource_list::ResourceList};
use tare::utils::StagedBuffer;
use tracing::error;

#[derive(Clone, Copy, Default)]
pub struct SkinningInfo {
    pub skeleton: Handle<SkeletonHeader>,
    pub animation_state: Handle<FurikakeAnimationState>,
    pub joints: Handle<JointTransform>,
}

#[derive(Clone)]
pub struct SkinnedModelData {
    pub info: SkinnedModelInfo,
    pub animation_state: Handle<FurikakeAnimationState>,
    pub instance_skeleton: Handle<SkeletonHeader>,
    pub instance_joints: Handle<JointTransform>,
    pub instance_joint_count: u32,
    animation_clips: Vec<Handle<furikake::types::AnimationClip>>,
    animation_dirty: bool,
    per_obj_joints: Option<PerObjectJointAllocation>,
}

impl SkinnedModelData {
    pub fn new(info: SkinnedModelInfo, bindless: &mut BindlessState) -> Self {
        let rig = info.model.rig.as_ref();
        if rig.is_none() {
            error!("Registered skinned model without a rig; animation handles will be missing.");
        }

        let animation_clips = rig
            .map(|rig| {
                let mut entries: Vec<_> = rig.animations.iter().collect();
                entries.sort_by(|(left, _), (right, _)| left.cmp(right));
                entries.into_iter().map(|(_, handle)| *handle).collect()
            })
            .unwrap_or_default();

        let animation_state = if rig.is_some() {
            bindless.register_animation_state()
        } else {
            Handle::default()
        };
        let per_obj_joints = rig.and_then(|rig| reserve_per_obj_joints(bindless, rig.skeleton));
        if rig.is_some() && per_obj_joints.is_none() {
            error!(
                "Failed to reserve per-object joints for skinned model; skinning dispatch will be skipped."
            );
        }

        let (instance_skeleton, instance_joints, instance_joint_count) = if rig.is_some() {
            clone_instance_skeleton(&info, bindless)
        } else {
            (Handle::default(), Handle::default(), 0)
        };
        Self {
            info,
            animation_state,
            instance_skeleton,
            instance_joints,
            instance_joint_count,
            animation_clips,
            animation_dirty: true,
            per_obj_joints,
        }
    }

    pub fn model(&self) -> &DeviceModel {
        &self.info.model
    }

    pub fn skinning_info(&self) -> SkinningInfo {
        let fallback_skeleton = self
            .info
            .model
            .rig
            .as_ref()
            .map(|rig| rig.skeleton)
            .unwrap_or_default();
        let skeleton = if self.instance_skeleton.valid() {
            self.instance_skeleton
        } else {
            fallback_skeleton
        };

        let joints = self.per_obj_joints_handle();

        SkinningInfo {
            skeleton,
            animation_state: self.animation_state,
            joints,
        }
    }

    pub fn mark_animation_dirty(&mut self) {
        self.animation_dirty = true;
    }

    pub fn clear_animation_dirty(&mut self) {
        self.animation_dirty = false;
    }

    pub fn dispatch_skeleton(&self) -> Handle<SkeletonHeader> {
        self.skinning_info().skeleton
    }

    pub fn per_obj_joints_handle(&self) -> Handle<JointTransform> {
        self.per_obj_joints
            .map(|allocation| Handle::new(allocation.offset as u16, 0))
            .unwrap_or_default()
    }
}

pub struct SkinningDispatcher {
    queue: CommandRing,
    pipeline: Option<bento::builder::CSO>,
    dispatches: StagedBuffer,
    skinned: ResourceList<SkinnedModelData>,
}

pub const MAX_SKINNING_DISPATCHES: usize = 1024;
pub type SkinningHandle = Handle<SkinnedModelData>;

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

        let pipeline = CSOBuilder::new()
            .shader(Some(include_str!("shaders/skinning.comp.glsl").as_bytes()))
            .add_variable(
                "skinning_dispatches",
                ShaderResource::StorageBuffer(dispatches.device().into()),
            )
            .add_reserved_table_variables(state).unwrap()
            .build(ctx)
            .ok();

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
            skinned: Default::default(),
        }
    }

    pub fn register(
        &mut self,
        info: SkinnedModelInfo,
        bindless: &mut BindlessState,
    ) -> (SkinningHandle, SkinningInfo) {
        let skinned = SkinnedModelData::new(info, bindless);
        let skinning_info = skinned.skinning_info();
        let handle = self.skinned.push(skinned);
        (handle, skinning_info)
    }

    pub fn unregister(&mut self, handle: SkinningHandle, bindless: &mut BindlessState) {
        if !handle.valid() {
            return;
        }

        if !self.skinned.entries.iter().any(|h| h.slot == handle.slot) {
            return;
        }

        let skinned = self.skinned.get_ref(handle).clone();
        unregister_skinned_model(bindless, &skinned);
        self.skinned.release(handle);
    }

    pub fn set_animation_state(&mut self, handle: SkinningHandle, state: AnimationState) {
        if !handle.valid() {
            return;
        }

        if !self.skinned.entries.iter().any(|h| h.slot == handle.slot) {
            return;
        }

        let skinned = self.skinned.get_ref_mut(handle);
        skinned.info.animation = state;
        skinned.mark_animation_dirty();
    }

    pub fn skinning_info(&self, handle: SkinningHandle) -> SkinningInfo {
        if !handle.valid() {
            return SkinningInfo::default();
        }

        if !self.skinned.entries.iter().any(|h| h.slot == handle.slot) {
            return SkinningInfo::default();
        }

        self.skinned.get_ref(handle).skinning_info()
    }

    pub fn update(&mut self, delta_time: f32) {
        let Some(pipeline) = self.pipeline.as_ref() else {
            return;
        };

        let buffer = self.dispatches.as_slice_mut::<SkinningDispatch>();
        if buffer.is_empty() {
            return;
        }

        let entries = self.skinned.entries.clone();
        let mut dispatch_count = 0;
        for entry in entries {
            if dispatch_count >= buffer.len() || dispatch_count >= MAX_SKINNING_DISPATCHES {
                break;
            }
            let skinned = self.skinned.get_ref_mut(entry);
            if skinned.info.model.rig.is_some() && !skinned.per_obj_joints_handle().valid() {
                continue;
            }
            buffer[dispatch_count] = SkinningDispatch::from_model(skinned, delta_time);
            skinned.clear_animation_dirty();
            dispatch_count += 1;
        }

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

        self.queue
            .submit(&Default::default())
            .expect("submit skinning");
        self.queue.wait_all().expect("wait skinning");
    }
}

pub fn unregister_skinned_model(bindless: &mut BindlessState, skinned: &SkinnedModelData) {
    bindless.unregister_animation_state(skinned.animation_state);
    if let Some(allocation) = skinned.per_obj_joints {
        let _ = bindless.reserved_mut::<ReservedPerObjJoints, _>(
            "meshi_per_obj_joints",
            |joints| {
                joints.free(allocation);
            },
        );
    }
    if skinned.instance_skeleton.valid() {
        let header = bindless
            .reserved::<ReservedBindlessSkeletons>("meshi_bindless_skeletons")
            .ok()
            .map(|skeletons| *skeletons.skeleton(skinned.instance_skeleton));
        if let Some(header) = header {
            let joint_count = header.joint_count as usize;
            let joint_offset = header.joint_offset;
            let bind_pose_offset = header.bind_pose_offset;
            let _ = bindless.reserved_mut::<ReservedBindlessJoints, _>(
                "meshi_bindless_joints",
                |joints| {
                    for idx in 0..joint_count {
                        let joint_slot = (joint_offset + idx as u32) as u16;
                        let bind_slot = (bind_pose_offset + idx as u32) as u16;
                        joints.remove_joint(Handle::new(joint_slot, 0));
                        joints.remove_joint(Handle::new(bind_slot, 0));
                    }
                },
            );
        }
        bindless.unregister_skeleton(skinned.instance_skeleton);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkinningDispatch {
    animation_state_id: u32,
    clip_handle: u32,
    skeleton_handle: u32,
    per_obj_joints_offset: u32,
    reset_time: u32,
    time_seconds: f32,
    playback_rate: f32,
    delta_time: f32,
    looping: u32,
}

impl SkinningDispatch {
    pub fn from_model(model: &SkinnedModelData, delta_time: f32) -> Self {
        let clip_handle = model
            .animation_clips
            .get(model.info.animation.clip_index as usize)
            .copied();
        let skeleton_handle = model.dispatch_skeleton();
        let per_obj_joints_handle = model.per_obj_joints_handle();

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
            per_obj_joints_offset: if per_obj_joints_handle.valid() {
                per_obj_joints_handle.slot as u32
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

fn clone_instance_skeleton(
    info: &SkinnedModelInfo,
    bindless: &mut BindlessState,
) -> (Handle<SkeletonHeader>, Handle<JointTransform>, u32) {
    let Some(rig) = info.model.rig.as_ref() else {
        return (Handle::default(), Handle::default(), 0);
    };
    if !rig.skeleton.valid() {
        return (Handle::default(), Handle::default(), 0);
    }

    let Ok(skeletons) = bindless.reserved::<ReservedBindlessSkeletons>("meshi_bindless_skeletons")
    else {
        return (Handle::default(), Handle::default(), 0);
    };
    let Ok(joints) = bindless.reserved::<ReservedBindlessJoints>("meshi_bindless_joints") else {
        return (Handle::default(), Handle::default(), 0);
    };

    let source = *skeletons.skeleton(rig.skeleton);
    if source.joint_count == 0 {
        return (Handle::default(), Handle::default(), 0);
    }

    let joint_count = source.joint_count as usize;
    let mut joint_data = Vec::with_capacity(joint_count);
    for idx in 0..joint_count {
        let joint_slot = (source.bind_pose_offset + idx as u32) as u16;
        joint_data.push(*joints.joint(Handle::new(joint_slot, 0)));
    }

    let mut joint_handles: Vec<Handle<JointTransform>> = Vec::with_capacity(joint_count * 2);
    let _ = bindless.reserved_mut::<ReservedBindlessJoints, _>("meshi_bindless_joints", |buffer| {
        for _ in 0..(joint_count * 2) {
            joint_handles.push(buffer.add_joint());
        }
        joint_handles.sort_by_key(|handle| handle.slot);
        for (idx, joint) in joint_data.iter().enumerate() {
            let animated = joint_handles[idx];
            let bind_pose = joint_handles[idx + joint_count];
            *buffer.joint_mut(animated) = *joint;
            *buffer.joint_mut(bind_pose) = *joint;
        }
    });

    if joint_handles.len() < joint_count * 2 {
        return (Handle::default(), Handle::default(), 0);
    }

    let mut instance_skeleton = Handle::default();
    let instance_joints = joint_handles[0];
    let _ = bindless.reserved_mut::<ReservedBindlessSkeletons, _>(
        "meshi_bindless_skeletons",
        |buffer| {
            instance_skeleton = buffer.add_skeleton();
            if instance_skeleton.valid() {
                *buffer.skeleton_mut(instance_skeleton) = SkeletonHeader {
                    joint_count: joint_count as u32,
                    joint_offset: joint_handles[0].slot as u32,
                    bind_pose_offset: joint_handles[joint_count].slot as u32,
                    ..Default::default()
                };
            }
        },
    );

    if instance_skeleton.valid() {
        (instance_skeleton, instance_joints, joint_count as u32)
    } else {
        (Handle::default(), Handle::default(), 0)
    }
}

fn reserve_per_obj_joints(
    bindless: &mut BindlessState,
    skeleton: Handle<SkeletonHeader>,
) -> Option<PerObjectJointAllocation> {
    if !skeleton.valid() {
        return None;
    }

    let skeletons =
        bindless.reserved::<ReservedBindlessSkeletons>("meshi_bindless_skeletons").ok()?;
    let header = *skeletons.skeleton(skeleton);
    if header.joint_count == 0 {
        return None;
    }
    
    let mut h = Option::None;
    bindless
        .reserved_mut::<ReservedPerObjJoints, _>("meshi_per_obj_joints", |joints| {
            h = joints.reserve(header.joint_count)
        }).unwrap();

    h
}
