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

layout(set = 0, binding = 2) buffer SkinningStates {
    AnimationState states[];
} meshi_bindless_skinning;

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
}
