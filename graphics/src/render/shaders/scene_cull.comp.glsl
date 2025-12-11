#version 450

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

struct SceneObject {
    mat4 local_transform;
    mat4 world_transform;
    uint scene_mask;
    uint parent_slot;
    uint dirty;
    uint active;
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
};

layout(set = 0, binding = 1) buffer SceneBins {
    SceneBin bins[];
};

layout(set = 0, binding = 2) buffer CulledBins {
    CulledObject culled[];
};

layout(set = 0, binding = 3) buffer BinCounts {
    uint counts[];
};

layout(set = 0, binding = 4) uniform SceneParams {
    uint num_bins;
    uint max_objects;
    uint camera_slot;
    uint _padding1;
} params;

layout(set = 1, binding = 0) buffer Cameras {
    Camera cameras[];
};

vec3 rotate_vec3(vec3 v, vec4 q) {
    vec3 t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (idx >= params.max_objects) {
        return;
    }

    SceneObject obj = objects[idx];
    if (obj.active == 0) {
        return;
    }

    if (params.camera_slot == 0xffffffffu) {
        return;
    }

    Camera cam = cameras[params.camera_slot];
    vec3 world_position = obj.world_transform[3].xyz;
    vec3 to_object = world_position - cam.position;
    vec3 forward = rotate_vec3(vec3(0.0, 0.0, -1.0), cam.rotation);

    if (dot(forward, to_object) <= 0.0) {
        return;
    }

    for (uint bin = 0; bin < params.num_bins; ++bin) {
        if ((obj.scene_mask & bins[bin].mask) == 0) {
            continue;
        }

        uint write_index = atomicAdd(counts[bin], 1);
        if (write_index >= params.max_objects) {
            continue;
        }

        uint target = bin * params.max_objects + write_index;
        culled[target].total_transform = obj.world_transform;
        culled[target].bin_id = bins[bin].id;
    }
}
