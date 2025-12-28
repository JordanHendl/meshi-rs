//scene_cull.comp.glsl
#version 450

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

struct SceneObject {
    mat4 local_transform;
    mat4 world_transform;
    uint scene_mask;
    uint transformation;
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
    uint object_id;
    uint bin_id;
    uint transformation;
};

struct Camera {
    mat4 world_from_camera;
    mat4 projection;
    vec2 viewport;
    float near;
    float far;
    float fov_y_radians;
    uint projection_kind;
    float _padding;
};

vec3 camera_position(const Camera cam) {
    return cam.world_from_camera[3].xyz;
}

vec4 camera_rotation_quat(const Camera cam)
{
    // Extract rotation matrix (world space)
    mat3 m = mat3(cam.world_from_camera);

    // Standard matrix → quaternion conversion
    float trace = m[0][0] + m[1][1] + m[2][2];
    vec4 q;

    if (trace > 0.0) {
        float s = sqrt(trace + 1.0) * 2.0;
        q.w = 0.25 * s;
        q.x = (m[2][1] - m[1][2]) / s;
        q.y = (m[0][2] - m[2][0]) / s;
        q.z = (m[1][0] - m[0][1]) / s;
    }
    else if (m[0][0] > m[1][1] && m[0][0] > m[2][2]) {
        float s = sqrt(1.0 + m[0][0] - m[1][1] - m[2][2]) * 2.0;
        q.w = (m[2][1] - m[1][2]) / s;
        q.x = 0.25 * s;
        q.y = (m[0][1] + m[1][0]) / s;
        q.z = (m[0][2] + m[2][0]) / s;
    }
    else if (m[1][1] > m[2][2]) {
        float s = sqrt(1.0 + m[1][1] - m[0][0] - m[2][2]) * 2.0;
        q.w = (m[0][2] - m[2][0]) / s;
        q.x = (m[0][1] + m[1][0]) / s;
        q.y = 0.25 * s;
        q.z = (m[1][2] + m[2][1]) / s;
    }
    else {
        float s = sqrt(1.0 + m[2][2] - m[0][0] - m[1][1]) * 2.0;
        q.w = (m[1][0] - m[0][1]) / s;
        q.x = (m[0][2] + m[2][0]) / s;
        q.y = (m[1][2] + m[2][1]) / s;
        q.z = 0.25 * s;
    }

    return normalize(q);
}

vec3 camera_rotation(const Camera cam) {
    // Extract camera basis vectors (world space)
    vec3 right   =  cam.world_from_camera[0].xyz;
    vec3 up      =  cam.world_from_camera[1].xyz;
    vec3 forward = -cam.world_from_camera[2].xyz;

    // Yaw (around Y)
    float yaw = atan(forward.x, forward.z);

    // Pitch (around X)
    float pitch = asin(clamp(forward.y, -1.0, 1.0));

    // Roll (around Z)
    float roll = atan(right.y, up.y);

    return vec3(pitch, yaw, roll);
}

vec3 camera_forward(const Camera cam) {
    return -cam.world_from_camera[2].xyz;
}

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
        vec3 to_object = world_position - camera_position(cam);
        vec3 forward = rotate_vec3(vec3(0.0, 0.0, -1.0), camera_rotation_quat(cam));

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
            culled.culled[target].object_id = idx;
            culled.culled[target].transformation = obj.transformation;
        }
    }
}
