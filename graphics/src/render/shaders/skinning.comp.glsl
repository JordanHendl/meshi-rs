// skinning.comp.glsl
#version 450

layout(local_size_x = 1, local_size_y = 1, local_size_z = 1) in;

struct SkinningDispatch {
    uint animation_state_id;
    uint clip_handle;
    uint skeleton_handle;
    uint reset_time;
    float time_seconds;
    float playback_rate;
    float delta_time;
    uint looping;
};

struct AnimationClip {
    float duration;
    uint track_count;
    uint track_offset;
    uint _padding;
};

struct AnimationTrack {
    uint joint_index;
    uint keyframe_count;
    uint keyframe_offset;
    uint _padding;
};

struct AnimationKeyframe {
    float time;
    vec3 _padding;
    vec4 value;
};

struct SkeletonHeader {
    uint joint_count;
    uint joint_offset;
    uint bind_pose_offset;
    uint _padding;
};

struct JointTransform {
    int parent_index;
    uvec3 _padding;
    mat4 bind_pose;
    mat4 inverse_bind;
};

struct AnimationState {
    uint clip_handle;
    uint skeleton_handle;
    float start_time;
    float playback_rate;
    uint looping;
    uint _padding0;
    uint _padding1;
    uint _padding2;
};

layout(set = 0, binding = 0) buffer SkinningDispatches {
    SkinningDispatch dispatches[];
} skinning_dispatches;

layout(set = 0, binding = 1) buffer AnimationClips {
    AnimationClip clips[];
} meshi_bindless_animations;

layout(set = 0, binding = 2) buffer AnimationTracks {
    AnimationTrack tracks[];
} meshi_bindless_animation_tracks;

layout(set = 0, binding = 3) buffer AnimationKeyframes {
    AnimationKeyframe keyframes[];
} meshi_bindless_animation_keyframes;

layout(set = 0, binding = 4) buffer Skeletons {
    SkeletonHeader skeletons[];
} meshi_bindless_skeletons;

layout(set = 0, binding = 5) buffer Joints {
    JointTransform joints[];
} meshi_bindless_joints;

layout(set = 0, binding = 6) buffer SkinningStates {
    AnimationState states[];
} meshi_bindless_skinning;

mat3 quat_to_mat3(vec4 q) {
    q = normalize(q);
    float x = q.x;
    float y = q.y;
    float z = q.z;
    float w = q.w;

    float xx = x * x;
    float yy = y * y;
    float zz = z * z;
    float xy = x * y;
    float xz = x * z;
    float yz = y * z;
    float wx = w * x;
    float wy = w * y;
    float wz = w * z;

    return mat3(
        1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy),
        2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx),
        2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy)
    );
}

AnimationKeyframe read_keyframe(uint index) {
    return meshi_bindless_animation_keyframes.keyframes[index];
}

vec4 sample_track(AnimationTrack track, float time_seconds) {
    if (track.keyframe_count == 0u) {
        return vec4(0.0);
    }

    uint base = track.keyframe_offset;
    AnimationKeyframe prev = read_keyframe(base);
    if (track.keyframe_count == 1u || time_seconds <= prev.time) {
        return prev.value;
    }

    for (uint idx = 1u; idx < track.keyframe_count; idx++) {
        AnimationKeyframe next = read_keyframe(base + idx);
        if (time_seconds <= next.time) {
            float span = max(next.time - prev.time, 0.0001);
            float t = clamp((time_seconds - prev.time) / span, 0.0, 1.0);
            return mix(prev.value, next.value, t);
        }
        prev = next;
    }

    return prev.value;
}

vec4 sample_track_rotation(AnimationTrack track, float time_seconds) {
    vec4 value = sample_track(track, time_seconds);
    return normalize(value);
}

bool track_is_rotation(AnimationTrack track) {
    if (track.keyframe_count == 0u) {
        return false;
    }
    vec4 value = read_keyframe(track.keyframe_offset).value;
    return abs(value.w) > 0.0001;
}

mat4 compose_transform(vec3 translation, mat3 rotation, vec3 scale) {
    return mat4(
        vec4(rotation[0] * scale.x, 0.0),
        vec4(rotation[1] * scale.y, 0.0),
        vec4(rotation[2] * scale.z, 0.0),
        vec4(translation, 1.0)
    );
}

mat3 bind_rotation(mat4 bind_pose, vec3 bind_scale) {
    return mat3(
        bind_pose[0].xyz / bind_scale.x,
        bind_pose[1].xyz / bind_scale.y,
        bind_pose[2].xyz / bind_scale.z
    );
}

mat4 sample_local_transform(
    uint joint_index,
    AnimationClip clip,
    float time_seconds,
    uint bind_pose_offset
) {
    JointTransform bind_joint = meshi_bindless_joints.joints[bind_pose_offset + joint_index];
    vec3 bind_translation = bind_joint.bind_pose[3].xyz;
    vec3 bind_scale = vec3(
        length(bind_joint.bind_pose[0].xyz),
        length(bind_joint.bind_pose[1].xyz),
        length(bind_joint.bind_pose[2].xyz)
    );
    mat3 bind_rot = bind_rotation(bind_joint.bind_pose, bind_scale);

    vec3 translation = vec3(0.0);
    vec3 scale = vec3(1.0);
    vec4 rotation = vec4(0.0, 0.0, 0.0, 1.0);
    bool has_translation = false;
    bool has_rotation = false;
    bool has_scale = false;
    uint non_rotation_tracks = 0u;

    for (uint idx = 0u; idx < clip.track_count; idx++) {
        AnimationTrack track = meshi_bindless_animation_tracks.tracks[clip.track_offset + idx];
        if (track.joint_index != joint_index) {
            continue;
        }

        if (track_is_rotation(track)) {
            rotation = sample_track_rotation(track, time_seconds);
            has_rotation = true;
        } else {
            vec3 value = sample_track(track, time_seconds).xyz;
            if (non_rotation_tracks == 0u) {
                translation = value;
                has_translation = true;
            } else {
                scale = value;
                has_scale = true;
            }
            non_rotation_tracks++;
        }
    }

    vec3 final_translation = has_translation ? translation : bind_translation;
    vec3 final_scale = has_scale ? scale : bind_scale;
    mat3 final_rotation = has_rotation ? quat_to_mat3(rotation) : bind_rot;

    return compose_transform(final_translation, final_rotation, final_scale);
}

mat4 compute_global_transform(
    uint joint_index,
    AnimationClip clip,
    float time_seconds,
    SkeletonHeader skeleton
) {
    mat4 world = sample_local_transform(
        joint_index,
        clip,
        time_seconds,
        skeleton.bind_pose_offset
    );

    int parent = meshi_bindless_joints.joints[skeleton.bind_pose_offset + joint_index].parent_index;
    uint remaining = skeleton.joint_count;
    while (parent >= 0 && remaining > 0u) {
        uint parent_index = uint(parent);
        mat4 parent_local = sample_local_transform(
            parent_index,
            clip,
            time_seconds,
            skeleton.bind_pose_offset
        );
        world = parent_local * world;
        parent = meshi_bindless_joints.joints[skeleton.bind_pose_offset + parent_index].parent_index;
        remaining--;
    }

    return world;
}

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (idx >= skinning_dispatches.dispatches.length()) {
        return;
    }

    SkinningDispatch dispatch = skinning_dispatches.dispatches[idx];
    if (dispatch.animation_state_id == 0xFFFFu) {
        return;
    }

    AnimationState state = meshi_bindless_skinning.states[dispatch.animation_state_id];
    if (dispatch.reset_time != 0u) {
        state.start_time = dispatch.time_seconds;
    }

    float time = state.start_time + dispatch.delta_time * dispatch.playback_rate;
    float duration = 0.0;
    if (dispatch.clip_handle != 0xFFFFu) {
        duration = meshi_bindless_animations.clips[dispatch.clip_handle].duration;
    }

    if (duration > 0.0) {
        if (dispatch.looping != 0u) {
            time = mod(time, duration);
            if (time < 0.0) {
                time += duration;
            }
        } else {
            time = min(time, duration);
        }
    }

    state.start_time = time;
    state.playback_rate = dispatch.playback_rate;
    state.looping = dispatch.looping;
    state.clip_handle = dispatch.clip_handle;
    state.skeleton_handle = dispatch.skeleton_handle;

    meshi_bindless_skinning.states[dispatch.animation_state_id] = state;

    if (dispatch.skeleton_handle == 0xFFFFu) {
        return;
    }

    SkeletonHeader skeleton = meshi_bindless_skeletons.skeletons[dispatch.skeleton_handle];
    if (skeleton.joint_count == 0u) {
        return;
    }

    if (dispatch.clip_handle == 0xFFFFu) {
        for (uint joint_idx = 0u; joint_idx < skeleton.joint_count; joint_idx++) {
            JointTransform bind_joint =
                meshi_bindless_joints.joints[skeleton.bind_pose_offset + joint_idx];
            JointTransform out_joint =
                meshi_bindless_joints.joints[skeleton.joint_offset + joint_idx];
            out_joint.bind_pose = bind_joint.bind_pose;
            out_joint.inverse_bind = bind_joint.inverse_bind;
            out_joint.parent_index = bind_joint.parent_index;
            meshi_bindless_joints.joints[skeleton.joint_offset + joint_idx] = out_joint;
        }
        return;
    }

    AnimationClip clip = meshi_bindless_animations.clips[dispatch.clip_handle];
    for (uint joint_idx = 0u; joint_idx < skeleton.joint_count; joint_idx++) {
        mat4 world = compute_global_transform(joint_idx, clip, time, skeleton);
        JointTransform out_joint =
            meshi_bindless_joints.joints[skeleton.joint_offset + joint_idx];
        JointTransform bind_joint =
            meshi_bindless_joints.joints[skeleton.bind_pose_offset + joint_idx];
        out_joint.bind_pose = world;
        out_joint.inverse_bind = bind_joint.inverse_bind;
        out_joint.parent_index = bind_joint.parent_index;
        meshi_bindless_joints.joints[skeleton.joint_offset + joint_idx] = out_joint;
    }
}
