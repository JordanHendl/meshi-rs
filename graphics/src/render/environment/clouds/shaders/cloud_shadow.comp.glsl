#version 450
#extension GL_EXT_samplerless_texture_functions : enable

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0) uniform CloudShadowParams {
    uint shadow_resolution;
    uvec3 base_noise_size;
    uvec3 detail_noise_size;
    uint weather_map_size;
    uint _padding;
    float cloud_base;
    float cloud_top;
    float density_scale;
    float _padding_1;
    vec2 wind;
    float time;
    float coverage_power;
    vec3 sun_direction;
    float shadow_strength;
    vec3 camera_position;
    float shadow_extent;
} params;

layout(set = 0, binding = 1) uniform texture2D cloud_weather_map;
layout(set = 0, binding = 2) uniform sampler cloud_weather_sampler;
layout(set = 0, binding = 3) uniform texture2D cloud_base_noise;
layout(set = 0, binding = 4) uniform sampler cloud_base_sampler;
layout(set = 0, binding = 5) uniform texture2D cloud_detail_noise;
layout(set = 0, binding = 6) uniform sampler cloud_detail_sampler;
layout(set = 0, binding = 7) buffer CloudShadowBuffer {
    float values[];
} cloud_shadow_buffer;

float hash11(float p) {
    p = fract(p * 0.1031);
    p *= p + 33.33;
    p *= p + p;
    return fract(p);
}

float sample_noise(texture2D tex, sampler samp, vec3 p, uvec3 dims) {
    vec3 fp = fract(p);
    float z = fp.z * float(dims.z);
    float z0 = floor(z);
    float z1 = mod(z0 + 1.0, float(dims.z));
    float fz = fract(z);
    float slice_w = float(dims.x) * float(dims.z);
    vec2 uv0 = vec2((fp.x * float(dims.x) + z0 * float(dims.x)) / slice_w, fp.y);
    vec2 uv1 = vec2((fp.x * float(dims.x) + z1 * float(dims.x)) / slice_w, fp.y);
    float n0 = texture(sampler2D(tex, samp), uv0).r;
    float n1 = texture(sampler2D(tex, samp), uv1).r;
    return mix(n0, n1, fz);
}

float sample_weather(vec2 uv) {
    return texture(sampler2D(cloud_weather_map, cloud_weather_sampler), uv).r;
}

void main() {
    uvec2 gid = gl_GlobalInvocationID.xy;
    if (gid.x >= params.shadow_resolution || gid.y >= params.shadow_resolution) {
        return;
    }

    vec2 uv = (vec2(gid) + 0.5) / float(params.shadow_resolution);
    vec2 centered = (uv * 2.0 - 1.0) * params.shadow_extent;
    vec3 origin = params.camera_position + vec3(centered.x, params.cloud_top, centered.y);
    vec3 dir = normalize(-params.sun_direction);

    float layer_depth = params.cloud_top - params.cloud_base;
    float step_count = 12.0;
    float step_size = layer_depth / step_count;
    float transmittance = 1.0;

    for (uint i = 0; i < uint(step_count); ++i) {
        float h = params.cloud_top - float(i) * step_size;
        vec3 sample_pos = vec3(origin.x, h, origin.z) + dir * (float(i) * step_size);
        float height_frac = clamp((h - params.cloud_base) / layer_depth, 0.0, 1.0);

        vec2 weather_uv = (sample_pos.xz * 0.0001) + params.wind * params.time * 0.0001;
        float coverage = pow(sample_weather(weather_uv), params.coverage_power);
        vec3 base_pos = sample_pos * 0.00025;
        float base_noise = sample_noise(cloud_base_noise, cloud_base_sampler, base_pos, params.base_noise_size);
        float detail_noise = sample_noise(cloud_detail_noise, cloud_detail_sampler, base_pos * 4.0, params.detail_noise_size);
        float density = max(base_noise * coverage - (1.0 - height_frac), 0.0);
        density = mix(density, density * detail_noise, 0.5);
        float sigma = density * params.density_scale;
        transmittance *= exp(-sigma * step_size * params.shadow_strength);
        if (transmittance < 0.01) {
            break;
        }
    }

    uint idx = gid.y * params.shadow_resolution + gid.x;
    cloud_shadow_buffer.values[idx] = clamp(transmittance, 0.0, 1.0);
}
