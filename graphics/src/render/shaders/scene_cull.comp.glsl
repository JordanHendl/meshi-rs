//scene_cull.comp.glsl
#version 450

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

struct SceneObject {
    mat4 local_transform;
    mat4 world_transform;
    uint scene_mask;
    uint parent_slot;
    uint dirty;
    uint is_active;
    uint parent;
    uint child_count;
    uint children[16];
};

struct SceneBin {
    uint id;
    uint mask;
};

struct CulledObject {
    mat4 total_transform;
    uint bin_id;
};

struct Camera {
    vec3 position;
    float _padding0;
    vec4 rotation;
};

layout(set = 0, binding = 0) buffer SceneObjects {
    SceneObject objects[];
} objects;

layout(set = 0, binding = 1) buffer SceneBins {
    SceneBin bins[];
} bins;

layout(set = 0, binding = 2) buffer CulledBins {
    CulledObject culled[];
} culled;

layout(set = 0, binding = 3) buffer BinCounts {
    uint counts[];
} counts;

layout(set = 0, binding = 4) uniform SceneParams {
    uint num_bins;
    uint max_objects;
    uint num_views;
} params;

layout(set = 0, binding = 5) uniform SceneCameras {
    uint count;
    uint slots[8];
} camera;

layout(set = 1, binding = 0) buffer Cameras {
    Camera cameras[];
} cameras;

vec3 rotate_vec3(vec3 v, vec4 q) {
    vec3 t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (idx >= params.max_objects) {
        return;
    }

    SceneObject obj = objects.objects[idx];
    if (obj.is_active == 0) {
        return;
    }

    if (camera.count == 0u) {
        return;
    }

    uint view_count = min(camera.count, params.num_views);
    vec3 world_position = obj.world_transform[3].xyz;

    for (uint view = 0; view < view_count; ++view) {
        uint slot = camera.slots[view];
        if (slot == 0xffffffffu) {
            continue;
        }

        Camera cam = cameras.cameras[slot];
        vec3 to_object = world_position - cam.position;
        vec3 forward = rotate_vec3(vec3(0.0, 0.0, -1.0), cam.rotation);

        if (dot(forward, to_object) <= 0.0) {
            continue;
        }

        for (uint bin = 0; bin < params.num_bins; ++bin) {
            if ((obj.scene_mask & bins.bins[bin].mask) == 0) {
                continue;
            }

            uint bin_offset = view * params.num_bins + bin;
            uint write_index = atomicAdd(counts.counts[bin_offset], 1);
            if (write_index >= params.max_objects) {
                continue;
            }

            uint target = bin_offset * params.max_objects + write_index;
            culled.culled[target].total_transform = obj.world_transform;
            culled.culled[target].bin_id = bins.bins[bin].id;
        }
    }
}
