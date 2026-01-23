#version 450
#extension GL_EXT_samplerless_texture_functions : enable

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 frag_color;

layout(set = 0, binding = 0) uniform CloudCompositeParams {
    uvec2 output_resolution;
    uvec2 low_resolution;
    float camera_near;
    float camera_far;
    float depth_sigma;
    uint debug_view;
    float history_weight_scale;
    float shadow_resolution;
    uint history_index;
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
    return (2.0 * params.camera_near * params.camera_far) /
           (params.camera_far + params.camera_near - z * (params.camera_far - params.camera_near));
}

uint clamp_index_low(ivec2 p) {
    int x = clamp(p.x, 0, int(params.low_resolution.x) - 1);
    int y = clamp(p.y, 0, int(params.low_resolution.y) - 1);
    return uint(y) * params.low_resolution.x + uint(x);
}

void lowres_bilerp_basis(vec2 uv, out ivec2 base, out vec2 f) {
    vec2 coord = uv * vec2(params.low_resolution) - 0.5;
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
    vec4 color   = (params.history_index == 0u) ? sample_color_a(v_uv)  : sample_color_b(v_uv);
    float trans  = (params.history_index == 0u) ? sample_trans_a(v_uv)  : sample_trans_b(v_uv);
    float depth  = (params.history_index == 0u) ? sample_depth_a(v_uv)  : sample_depth_b(v_uv);
    float steps  = sample_steps(v_uv);
    float weight = (params.history_index == 0u) ? sample_weight_a(v_uv) : sample_weight_b(v_uv);

    float scene_depth_v = texture(sampler2D(scene_depth, cloud_sampler), v_uv).r;
    float scene_linear = linearize_depth(scene_depth_v);
    if (depth > 0.0 && scene_linear + params.depth_sigma < depth) {
        trans = 1.0;
        color = vec4(0.0);
    }

    if (params.debug_view == 1u) {
        vec4 weather = texture(sampler2D(cloud_weather_map, cloud_sampler), fract(v_uv));
        frag_color = vec4(weather.rgb, 1.0);
        return;
    }
    if (params.debug_view == 2u) {
        vec2 uv = v_uv;
        uint shadow_res = uint(params.shadow_resolution);
        uvec2 coord = uvec2(uv * float(shadow_res));
        coord = min(coord, uvec2(shadow_res - 1));
        uint idx = coord.y * shadow_res + coord.x;
        float shadow = cloud_shadow.values[idx];
        frag_color = vec4(vec3(shadow), 1.0);
        return;
    }
    if (params.debug_view == 3u) {
        frag_color = vec4(vec3(trans), 1.0);
        return;
    }
    if (params.debug_view == 4u) {
        frag_color = vec4(steps, 0.0, 1.0 - steps, 1.0);
        return;
    }
    if (params.debug_view == 5u) {
        float weight_v = clamp(weight * params.history_weight_scale, 0.0, 1.0);
        frag_color = vec4(weight_v, 1.0 - weight_v, 0.0, 1.0);
        return;
    }

    float alpha = 1.0 - trans;
    frag_color = vec4(color.rgb * alpha, alpha);
}
