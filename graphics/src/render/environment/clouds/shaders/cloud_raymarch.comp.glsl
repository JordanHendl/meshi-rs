#version 450
#extension GL_EXT_samplerless_texture_functions : enable
#extension GL_EXT_scalar_block_layout : enable

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0, scalar) uniform CloudRaymarchParams {
    uvec2 output_resolution;
    uvec3 base_noise_size;
    uvec3 detail_noise_size;
    uint weather_map_size;
    uint frame_index;
    uint shadow_resolution;
    mat4 view_proj;
    mat4 inv_view_proj;
    vec3 camera_position;
    float camera_near;
    float camera_far;
    float cloud_base;
    float cloud_top;
    float density_scale;
    uint step_count;
    uint light_step_count;
    float phase_g;
    vec3 sun_radiance;
    float shadow_strength;
    vec2 wind;
    float time;
    float coverage_power;
    float detail_strength;
    float curl_strength;
    float jitter_strength;
    float epsilon;
    vec3 sun_direction;
    uint use_shadow_map;
    float shadow_extent;
} params;

layout(set = 0, binding = 1) uniform texture2D cloud_weather_map;
layout(set = 0, binding = 2) uniform sampler cloud_weather_sampler;
layout(set = 0, binding = 3) uniform texture2D cloud_base_noise;
layout(set = 0, binding = 4) uniform sampler cloud_base_sampler;
layout(set = 0, binding = 5) uniform texture2D cloud_detail_noise;
layout(set = 0, binding = 6) uniform sampler cloud_detail_sampler;
layout(set = 0, binding = 7) uniform texture2D cloud_blue_noise;
layout(set = 0, binding = 8) uniform sampler cloud_blue_sampler;
layout(set = 0, binding = 9) buffer CloudShadowBuffer {
    float values[];
} cloud_shadow_buffer;
layout(set = 0, binding = 10) buffer CloudColorBuffer {
    vec4 values[];
} cloud_color_buffer;
layout(set = 0, binding = 11) buffer CloudTransmittanceBuffer {
    float values[];
} cloud_transmittance_buffer;
layout(set = 0, binding = 12) buffer CloudDepthBuffer {
    float values[];
} cloud_depth_buffer;
layout(set = 0, binding = 13) buffer CloudStepsBuffer {
    float values[];
} cloud_steps_buffer;

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

vec3 curl_noise(texture2D tex, sampler samp, vec3 p, uvec3 dims) {
    float eps = 0.01;
    float n1 = sample_noise(tex, samp, vec3(p.x, p.y + eps, p.z), dims);
    float n2 = sample_noise(tex, samp, vec3(p.x, p.y - eps, p.z), dims);
    float n3 = sample_noise(tex, samp, vec3(p.x, p.y, p.z + eps), dims);
    float n4 = sample_noise(tex, samp, vec3(p.x, p.y, p.z - eps), dims);
    float n5 = sample_noise(tex, samp, vec3(p.x + eps, p.y, p.z), dims);
    float n6 = sample_noise(tex, samp, vec3(p.x - eps, p.y, p.z), dims);
    float curl_x = n1 - n2 - n3 + n4;
    float curl_y = n3 - n4 - n5 + n6;
    float curl_z = n5 - n6 - n1 + n2;
    return vec3(curl_x, curl_y, curl_z);
}

vec4 sample_weather(vec2 uv) {
    return texture(sampler2D(cloud_weather_map, cloud_weather_sampler), uv);
}

float phase_hg(float cos_theta, float g) {
    float g2 = g * g;
    float denom = pow(1.0 + g2 - 2.0 * g * cos_theta, 1.5);
    return (1.0 - g2) / max(4.0 * 3.14159265 * denom, 1e-3);
}

float sample_shadow(vec3 world_pos, float shadow_extent, uint shadow_res) {
    vec2 uv = (world_pos.xz / shadow_extent) * 0.5 + 0.5;
    uv = clamp(uv, vec2(0.0), vec2(1.0));
    uvec2 coord = uvec2(uv * float(shadow_res));
    coord = min(coord, uvec2(shadow_res - 1));
    uint idx = coord.y * shadow_res + coord.x;
    return cloud_shadow_buffer.values[idx];
}

float light_march(vec3 origin, vec3 dir, float max_dist, uint steps, float sigma, float shadow_strength) {
    float step_size = max_dist / float(steps);
    float trans = 1.0;
    vec3 pos = origin;
    for (uint i = 0; i < steps; ++i) {
        pos += dir * step_size;
        float density = sigma;
        trans *= exp(-density * step_size * shadow_strength);
        if (trans < 0.05) {
            break;
        }
    }
    return trans;
}

void main() {
    uvec2 gid = gl_GlobalInvocationID.xy;
    if (gid.x >= params.output_resolution.x || gid.y >= params.output_resolution.y) {
        return;
    }

    uint idx = gid.y * params.output_resolution.x + gid.x;

    vec2 uv = (vec2(gid) + 0.5) / vec2(params.output_resolution);
    vec2 ndc = uv * 2.0 - 1.0;
    vec4 clip = vec4(ndc, 1.0, 1.0);
    vec4 world = params.inv_view_proj * clip;
    world.xyz /= world.w;
    vec3 ray_dir = normalize(world.xyz - params.camera_position);

    float t0 = (params.cloud_base - params.camera_position.y) / ray_dir.y;
    float t1 = (params.cloud_top - params.camera_position.y) / ray_dir.y;
    if (t0 > t1) {
        float temp = t0;
        t0 = t1;
        t1 = temp;
    }
    if (t1 <= 0.0) {
        cloud_color_buffer.values[idx] = vec4(0.0);
        cloud_transmittance_buffer.values[idx] = 1.0;
        cloud_depth_buffer.values[idx] = 0.0;
        cloud_steps_buffer.values[idx] = 0.0;
        return;
    }

    float start = max(t0, 0.0);
    float end = t1;
    float step_count = float(max(params.step_count, 1));
    float step_size = (end - start) / step_count;
    vec2 blue_uv = (vec2(gid) + vec2(params.frame_index % 64u)) / 128.0;
    vec2 jitter = texture(sampler2D(cloud_blue_noise, cloud_blue_sampler), blue_uv).rg;
    float jitter_offset = (jitter.x + jitter.y) * 0.5 * params.jitter_strength;
    float t = start + jitter_offset * step_size;

    float transmittance = 1.0;
    vec3 color = vec3(0.0);
    float depth_accum = 0.0;
    float weight_accum = 0.0;
    float steps_used = 0.0;

    for (uint i = 0; i < params.step_count; ++i) {
        vec3 sample_pos = params.camera_position + ray_dir * t;
        float height_frac = clamp((sample_pos.y - params.cloud_base) / (params.cloud_top - params.cloud_base), 0.0, 1.0);

        vec2 weather_uv = sample_pos.xz * 0.0001 + params.wind * params.time * 0.0001;
        vec4 weather = sample_weather(weather_uv);
        float coverage = pow(weather.r, params.coverage_power);
        float type = weather.g;
        float thickness = weather.b;

        vec3 base_pos = sample_pos * 0.00025 + vec3(params.wind * params.time * 0.01, 0.0);
        if (params.curl_strength > 0.0) {
            vec3 curl = curl_noise(cloud_base_noise, cloud_base_sampler, base_pos, params.base_noise_size);
            base_pos += curl * params.curl_strength;
        }
        float base_noise = sample_noise(cloud_base_noise, cloud_base_sampler, base_pos, params.base_noise_size);
        float detail_noise = sample_noise(cloud_detail_noise, cloud_detail_sampler, base_pos * 4.0, params.detail_noise_size);
        float density = max(base_noise * coverage - (1.0 - thickness) * (1.0 - height_frac), 0.0);
        density *= mix(1.0, detail_noise, params.detail_strength);
        density *= mix(1.0, type, 0.5);

        if (density > 0.001) {
            float sigma_t = density * params.density_scale;
            float step_trans = exp(-sigma_t * step_size);
            float light_trans = 1.0;
            if (params.use_shadow_map == 1u) {
                light_trans = sample_shadow(sample_pos, params.shadow_extent, params.shadow_resolution) * params.shadow_strength;
            } else {
                light_trans = light_march(sample_pos, params.sun_direction, 5000.0, params.light_step_count, sigma_t, params.shadow_strength);
            }
            float phase = phase_hg(dot(ray_dir, params.sun_direction), params.phase_g);
            vec3 scatter = params.sun_radiance * phase * light_trans;
            color += transmittance * scatter * (1.0 - step_trans);
            transmittance *= step_trans;
            depth_accum += t * (1.0 - step_trans);
            weight_accum += (1.0 - step_trans);
        }

        steps_used += 1.0;
        t += step_size;
        if (transmittance < params.epsilon) {
            break;
        }
    }

    float depth = weight_accum > 0.0 ? depth_accum / weight_accum : 0.0;

    cloud_color_buffer.values[idx] = vec4(color, 1.0);
    cloud_transmittance_buffer.values[idx] = clamp(transmittance, 0.0, 1.0);
    cloud_depth_buffer.values[idx] = depth;
    cloud_steps_buffer.values[idx] = steps_used / step_count;
}
