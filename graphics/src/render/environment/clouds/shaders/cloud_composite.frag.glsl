#version 450
#extension GL_EXT_samplerless_texture_functions : enable
#extension GL_EXT_scalar_block_layout : disable

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 frag_color;

const uint DEBUG_VIEW_WEATHER = 1u;
const uint DEBUG_VIEW_SHADOW = 2u;
const uint DEBUG_VIEW_TRANS = 3u;
const uint DEBUG_VIEW_STEP = 4u;
const uint DEBUG_VIEW_WEIGHT = 5u;
const uint DEBUG_VIEW_CLOUD_SHADOW_CASCADE_0 = 11u;
const uint DEBUG_VIEW_CLOUD_SHADOW_CASCADE_1 = 12u;
const uint DEBUG_VIEW_CLOUD_SHADOW_CASCADE_2 = 13u;
const uint DEBUG_VIEW_CLOUD_SHADOW_CASCADE_3 = 14u;

layout(set = 0, binding = 0, std140) uniform CloudCompositeParams {
    uvec4 resolution_info;
    vec4 camera_params;
    uvec4 history_info;
    vec4 shadow_params;
    vec4 atmosphere_view;
    vec4 atmosphere_haze_color;
    uvec4 shadow_cascade_resolutions;
    uvec4 shadow_cascade_offsets;
} params;

layout(set = 1, binding = 0) buffer CloudColorA { vec4 values[]; } cloud_color_a;
layout(set = 1, binding = 1) buffer CloudColorB { vec4 values[]; } cloud_color_b;
layout(set = 1, binding = 2) buffer CloudTransA { float values[]; } cloud_trans_a;
layout(set = 1, binding = 3) buffer CloudTransB { float values[]; } cloud_trans_b;
layout(set = 1, binding = 4) buffer CloudDepthA { float values[]; } cloud_depth_a;
layout(set = 1, binding = 5) buffer CloudDepthB { float values[]; } cloud_depth_b;
layout(set = 1, binding = 6) buffer CloudSteps { float values[]; } cloud_steps;
layout(set = 1, binding = 7) buffer CloudWeightA { float values[]; } cloud_weight_a;
layout(set = 1, binding = 8) buffer CloudWeightB { float values[]; } cloud_weight_b;
layout(set = 1, binding = 9) buffer CloudShadow { float values[]; } cloud_shadow;

layout(set = 2, binding = 0) uniform texture2D cloud_weather_map;
layout(set = 2, binding = 1) uniform texture2D scene_depth;
layout(set = 2, binding = 2) uniform sampler cloud_sampler;

float linearize_depth(float depth) {
    float z = depth * 2.0 - 1.0;
    return (2.0 * params.camera_params.x * params.camera_params.y) /
           (params.camera_params.y + params.camera_params.x - z * (params.camera_params.y - params.camera_params.x));
}

uint clamp_index_low(ivec2 p) {
    int x = clamp(p.x, 0, int(params.resolution_info.z) - 1);
    int y = clamp(p.y, 0, int(params.resolution_info.w) - 1);
    return uint(y) * params.resolution_info.z + uint(x);
}

void lowres_bilerp_basis(vec2 uv, out ivec2 base, out vec2 f) {
    vec2 coord = uv * vec2(params.resolution_info.zw) - 0.5;
    base = ivec2(floor(coord));
    f = fract(coord);
}

vec4 sample_color_a(vec2 uv) {
    ivec2 base; vec2 f;
    lowres_bilerp_basis(uv, base, f);

    vec4 c00 = cloud_color_a.values[clamp_index_low(base + ivec2(0, 0))];
    vec4 c10 = cloud_color_a.values[clamp_index_low(base + ivec2(1, 0))];
    vec4 c01 = cloud_color_a.values[clamp_index_low(base + ivec2(0, 1))];
    vec4 c11 = cloud_color_a.values[clamp_index_low(base + ivec2(1, 1))];

    return mix(mix(c00, c10, f.x), mix(c01, c11, f.x), f.y);
}

vec4 sample_color_b(vec2 uv) {
    ivec2 base; vec2 f;
    lowres_bilerp_basis(uv, base, f);

    vec4 c00 = cloud_color_b.values[clamp_index_low(base + ivec2(0, 0))];
    vec4 c10 = cloud_color_b.values[clamp_index_low(base + ivec2(1, 0))];
    vec4 c01 = cloud_color_b.values[clamp_index_low(base + ivec2(0, 1))];
    vec4 c11 = cloud_color_b.values[clamp_index_low(base + ivec2(1, 1))];

    return mix(mix(c00, c10, f.x), mix(c01, c11, f.x), f.y);
}

float sample_trans_a(vec2 uv) {
    ivec2 base; vec2 f;
    lowres_bilerp_basis(uv, base, f);

    float v00 = cloud_trans_a.values[clamp_index_low(base + ivec2(0, 0))];
    float v10 = cloud_trans_a.values[clamp_index_low(base + ivec2(1, 0))];
    float v01 = cloud_trans_a.values[clamp_index_low(base + ivec2(0, 1))];
    float v11 = cloud_trans_a.values[clamp_index_low(base + ivec2(1, 1))];

    return mix(mix(v00, v10, f.x), mix(v01, v11, f.x), f.y);
}

float sample_trans_b(vec2 uv) {
    ivec2 base; vec2 f;
    lowres_bilerp_basis(uv, base, f);

    float v00 = cloud_trans_b.values[clamp_index_low(base + ivec2(0, 0))];
    float v10 = cloud_trans_b.values[clamp_index_low(base + ivec2(1, 0))];
    float v01 = cloud_trans_b.values[clamp_index_low(base + ivec2(0, 1))];
    float v11 = cloud_trans_b.values[clamp_index_low(base + ivec2(1, 1))];

    return mix(mix(v00, v10, f.x), mix(v01, v11, f.x), f.y);
}

float sample_depth_a(vec2 uv) {
    ivec2 base; vec2 f;
    lowres_bilerp_basis(uv, base, f);

    float v00 = cloud_depth_a.values[clamp_index_low(base + ivec2(0, 0))];
    float v10 = cloud_depth_a.values[clamp_index_low(base + ivec2(1, 0))];
    float v01 = cloud_depth_a.values[clamp_index_low(base + ivec2(0, 1))];
    float v11 = cloud_depth_a.values[clamp_index_low(base + ivec2(1, 1))];

    return mix(mix(v00, v10, f.x), mix(v01, v11, f.x), f.y);
}

float sample_depth_b(vec2 uv) {
    ivec2 base; vec2 f;
    lowres_bilerp_basis(uv, base, f);

    float v00 = cloud_depth_b.values[clamp_index_low(base + ivec2(0, 0))];
    float v10 = cloud_depth_b.values[clamp_index_low(base + ivec2(1, 0))];
    float v01 = cloud_depth_b.values[clamp_index_low(base + ivec2(0, 1))];
    float v11 = cloud_depth_b.values[clamp_index_low(base + ivec2(1, 1))];

    return mix(mix(v00, v10, f.x), mix(v01, v11, f.x), f.y);
}

float sample_steps(vec2 uv) {
    ivec2 base; vec2 f;
    lowres_bilerp_basis(uv, base, f);

    float v00 = cloud_steps.values[clamp_index_low(base + ivec2(0, 0))];
    float v10 = cloud_steps.values[clamp_index_low(base + ivec2(1, 0))];
    float v01 = cloud_steps.values[clamp_index_low(base + ivec2(0, 1))];
    float v11 = cloud_steps.values[clamp_index_low(base + ivec2(1, 1))];

    return mix(mix(v00, v10, f.x), mix(v01, v11, f.x), f.y);
}

float sample_weight_a(vec2 uv) {
    ivec2 base; vec2 f;
    lowres_bilerp_basis(uv, base, f);

    float v00 = cloud_weight_a.values[clamp_index_low(base + ivec2(0, 0))];
    float v10 = cloud_weight_a.values[clamp_index_low(base + ivec2(1, 0))];
    float v01 = cloud_weight_a.values[clamp_index_low(base + ivec2(0, 1))];
    float v11 = cloud_weight_a.values[clamp_index_low(base + ivec2(1, 1))];

    return mix(mix(v00, v10, f.x), mix(v01, v11, f.x), f.y);
}

float sample_weight_b(vec2 uv) {
    ivec2 base; vec2 f;
    lowres_bilerp_basis(uv, base, f);

    float v00 = cloud_weight_b.values[clamp_index_low(base + ivec2(0, 0))];
    float v10 = cloud_weight_b.values[clamp_index_low(base + ivec2(1, 0))];
    float v01 = cloud_weight_b.values[clamp_index_low(base + ivec2(0, 1))];
    float v11 = cloud_weight_b.values[clamp_index_low(base + ivec2(1, 1))];

    return mix(mix(v00, v10, f.x), mix(v01, v11, f.x), f.y);
}

void main() {
    vec4 color   = (params.history_info.y == 0u) ? sample_color_a(v_uv)  : sample_color_b(v_uv);
    float trans  = (params.history_info.y == 0u) ? sample_trans_a(v_uv)  : sample_trans_b(v_uv);
    float depth  = (params.history_info.y == 0u) ? sample_depth_a(v_uv)  : sample_depth_b(v_uv);
    float steps  = sample_steps(v_uv);
    float weight = (params.history_info.y == 0u) ? sample_weight_a(v_uv) : sample_weight_b(v_uv);

    float scene_depth_v = texture(sampler2D(scene_depth, cloud_sampler), v_uv).r;
    float scene_linear = linearize_depth(scene_depth_v);
    if (depth > 0.0 && scene_linear + params.camera_params.z < depth) {
        trans = 1.0;
        color = vec4(0.0);
    }

    if (params.history_info.x == DEBUG_VIEW_WEATHER) {
        vec4 weather = texture(sampler2D(cloud_weather_map, cloud_sampler), fract(v_uv));
        frag_color = vec4(weather.rgb, 1.0);
        return;
    }
    if (params.history_info.x == DEBUG_VIEW_SHADOW
        || (params.history_info.x >= DEBUG_VIEW_CLOUD_SHADOW_CASCADE_0
            && params.history_info.x <= DEBUG_VIEW_CLOUD_SHADOW_CASCADE_3)) {
        uint cascade_index = 0u;
        if (params.history_info.x >= DEBUG_VIEW_CLOUD_SHADOW_CASCADE_0) {
            cascade_index = params.history_info.x - DEBUG_VIEW_CLOUD_SHADOW_CASCADE_0;
        }
        cascade_index = min(cascade_index, max(params.history_info.z, 1u) - 1u);
        uint shadow_res = params.shadow_cascade_resolutions[cascade_index];
        if (shadow_res == 0u) {
            shadow_res = uint(params.shadow_params.x);
        }
        shadow_res = max(shadow_res, 1u);
        uvec2 coord = uvec2(v_uv * float(shadow_res));
        coord = min(coord, uvec2(shadow_res - 1));
        uint idx = params.shadow_cascade_offsets[cascade_index] + coord.y * shadow_res + coord.x;
        float shadow = cloud_shadow.values[idx];
        frag_color = vec4(vec3(shadow), 1.0);
        return;
    }
    if (params.history_info.x == DEBUG_VIEW_TRANS) {
        frag_color = vec4(vec3(trans), 1.0);
        return;
    }
    if (params.history_info.x == DEBUG_VIEW_STEP) {
        frag_color = vec4(steps, 0.0, 1.0 - steps, 1.0);
        return;
    }
    if (params.history_info.x == DEBUG_VIEW_WEIGHT) {
        float weight_v = clamp(weight * params.camera_params.w, 0.0, 1.0);
        frag_color = vec4(weight_v, 1.0 - weight_v, 0.0, 1.0);
        return;
    }

    float view_trans = 1.0;
    if (depth > 0.0) {
        view_trans = exp(-params.atmosphere_view.y * depth);
    }
    view_trans = mix(1.0, view_trans, params.atmosphere_view.x);
    vec3 haze = params.atmosphere_haze_color.rgb * params.atmosphere_view.z;
    color.rgb = color.rgb * view_trans + haze * (1.0 - view_trans);
    float alpha = (1.0 - trans) * view_trans;
    frag_color = vec4(color.rgb * alpha, alpha);
}
