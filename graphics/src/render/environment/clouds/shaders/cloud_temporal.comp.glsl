#version 450

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

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

layout(set = 0, binding = 0) uniform CloudTemporalParams {
    uvec2 output_resolution;
    mat4 prev_view_proj;
    uint camera_index;
    uvec3 _padding;
    float blend_factor;
    float clamp_strength;
    float depth_sigma;
    float _padding_1;
} params;

layout(set = 0, binding = 1) buffer CloudCurrentColor { vec4 values[]; } cloud_current_color;
layout(set = 0, binding = 2) buffer CloudCurrentTrans { float values[]; } cloud_current_trans;
layout(set = 0, binding = 3) buffer CloudCurrentDepth { float values[]; } cloud_current_depth;
layout(set = 0, binding = 4) buffer CloudHistoryColor { vec4 values[]; } cloud_history_color;
layout(set = 0, binding = 5) buffer CloudHistoryTrans { float values[]; } cloud_history_trans;
layout(set = 0, binding = 6) buffer CloudHistoryDepth { float values[]; } cloud_history_depth;
layout(set = 0, binding = 7) buffer CloudHistoryWeight { float values[]; } cloud_history_weight;
layout(set = 0, binding = 8) buffer CloudOutputColor { vec4 values[]; } cloud_output_color;
layout(set = 0, binding = 9) buffer CloudOutputTrans { float values[]; } cloud_output_trans;
layout(set = 0, binding = 10) buffer CloudOutputDepth { float values[]; } cloud_output_depth;
layout(set = 0, binding = 11) buffer CloudOutputWeight { float values[]; } cloud_output_weight;
layout(set = 1, binding = 1) readonly buffer SceneCameras {
    Camera cameras[];
} meshi_bindless_cameras;

// Replace both sample_buffer functions with these:

uint clamp_index(ivec2 coord, uvec2 res) {
    ivec2 c = clamp(coord, ivec2(0), ivec2(res) - ivec2(1));
    return uint(c.y) * res.x + uint(c.x);
}

vec4 sample_current_color(ivec2 coord) {
    return cloud_current_color.values[clamp_index(coord, params.output_resolution)];
}

vec4 sample_history_color(ivec2 coord) {
    return cloud_history_color.values[clamp_index(coord, params.output_resolution)];
}

float sample_history_trans(ivec2 coord) {
    return cloud_history_trans.values[clamp_index(coord, params.output_resolution)];
}

float sample_history_depth(ivec2 coord) {
    return cloud_history_depth.values[clamp_index(coord, params.output_resolution)];
}

float sample_history_weight(ivec2 coord) {
    return cloud_history_weight.values[clamp_index(coord, params.output_resolution)];
}

vec4 clamp_color(vec4 history, vec4 current, vec4 min_c, vec4 max_c, float strength) {
    vec4 clamped = clamp(history, min_c, max_c);
    return mix(history, clamped, strength);
}

void main() {
    uvec2 gid = gl_GlobalInvocationID.xy;
    if (gid.x >= params.output_resolution.x || gid.y >= params.output_resolution.y) {
        return;
    }

    uint idx = gid.y * params.output_resolution.x + gid.x;
    vec4 current_color = cloud_current_color.values[idx];
    float current_trans = cloud_current_trans.values[idx];
    float current_depth = cloud_current_depth.values[idx];

    Camera camera = meshi_bindless_cameras.cameras[params.camera_index];
    vec3 camera_position = camera.world_from_camera[3].xyz;
    vec3 camera_forward = normalize(-camera.world_from_camera[2].xyz);
    mat4 view = inverse(camera.world_from_camera);
    mat4 view_proj = camera.projection * view;
    mat4 inv_view_proj = inverse(view_proj);

    vec2 uv = (vec2(gid) + 0.5) / vec2(params.output_resolution);
    vec2 ndc = uv * 2.0 - 1.0;
    vec4 clip = vec4(ndc, 1.0, 1.0);
    vec4 world = inv_view_proj * clip;
    world.xyz /= world.w;
    vec3 ray_dir = normalize(world.xyz - camera_position);
    float forward_dot = max(dot(ray_dir, camera_forward), 1e-4);
    float ray_distance = current_depth / forward_dot;
    vec3 world_pos = camera_position + ray_dir * ray_distance;
    vec4 prev_clip = params.prev_view_proj * vec4(world_pos, 1.0);
    vec2 prev_uv = prev_clip.xy / prev_clip.w * 0.5 + 0.5;

    ivec2 prev_coord = ivec2(prev_uv * vec2(params.output_resolution));
    bool valid = all(greaterThanEqual(prev_uv, vec2(0.0))) && all(lessThanEqual(prev_uv, vec2(1.0)));

    vec4 history_color  = valid ? sample_history_color(prev_coord)  : current_color;
    float history_trans = valid ? sample_history_trans(prev_coord)  : current_trans;
    float history_depth = valid ? sample_history_depth(prev_coord)  : current_depth;
    float history_weight= valid ? sample_history_weight(prev_coord) : 0.0;

    vec4 min_c = current_color;
    vec4 max_c = current_color;
    for (int y = -1; y <= 1; ++y) {
        for (int x = -1; x <= 1; ++x) {
            vec4 s = sample_current_color(ivec2(gid) + ivec2(x, y));
            min_c = min(min_c, s);
            max_c = max(max_c, s);
        }
    }

    history_color = clamp_color(history_color, current_color, min_c, max_c, params.clamp_strength);

    float depth_delta = abs(history_depth - current_depth);
    float depth_factor = exp(-depth_delta / max(params.depth_sigma, 1e-3));

    float blend = params.blend_factor * depth_factor;
    vec4 out_color = mix(current_color, history_color, blend);
    float out_trans = mix(current_trans, history_trans, blend);
    float out_depth = mix(current_depth, history_depth, blend);
    float out_weight = mix(1.0, history_weight, blend);

    cloud_output_color.values[idx] = out_color;
    cloud_output_trans.values[idx] = out_trans;
    cloud_output_depth.values[idx] = out_depth;
    cloud_output_weight.values[idx] = out_weight;
}
