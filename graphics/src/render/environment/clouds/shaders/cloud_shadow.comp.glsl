#version 450
#extension GL_EXT_samplerless_texture_functions : enable
#extension GL_EXT_scalar_block_layout : enable

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

layout(set = 0, binding = 0, scalar) uniform CloudShadowParams {
    uint shadow_resolution;
    uint cascade_count;
    uvec2 _padding_0;
    uint cascade_resolutions[4];
    uint cascade_offsets[4];
    uvec3 base_noise_size;
    uvec3 detail_noise_size;
    uint weather_map_size;
    uint camera_index;
    float cloud_base_a;
    float cloud_top_a;
    float density_scale_a;
    float noise_scale_a;
    vec2 wind_a;
    float cloud_base_b;
    float cloud_top_b;
    float density_scale_b;
    float noise_scale_b;
    vec2 wind_b;
    uvec3 weather_channels_a;
    uvec3 weather_channels_b;
    float time;
    float coverage_power;
    vec3 sun_direction;
    float shadow_strength;
    vec4 cascade_extents;
    vec4 cascade_splits;
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
layout(set = 1, binding = 1) readonly buffer SceneCameras {
    Camera cameras[];
} meshi_bindless_cameras;

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

float weather_channel(vec4 weather, uint channel) {
    if (channel == 0u) {
        return weather.r;
    }
    if (channel == 1u) {
        return weather.g;
    }
    if (channel == 2u) {
        return weather.b;
    }
    return weather.a;
}

void main() {
    uint cascade_index = gl_GlobalInvocationID.z;
    uvec2 gid = gl_GlobalInvocationID.xy;
    if (cascade_index >= params.cascade_count) {
        return;
    }
    uint cascade_resolution = params.cascade_resolutions[cascade_index];
    if (cascade_resolution == 0u) {
        cascade_resolution = params.shadow_resolution;
    }
    if (gid.x >= cascade_resolution || gid.y >= cascade_resolution) {
        return;
    }

    Camera camera = meshi_bindless_cameras.cameras[params.camera_index];
    vec3 camera_position = camera.world_from_camera[3].xyz;
    vec2 uv = (vec2(gid) + 0.5) / float(cascade_resolution);
    float cascade_extent = params.cascade_extents[cascade_index];
    vec2 centered = (uv * 2.0 - 1.0) * cascade_extent;
    float max_top = max(params.cloud_top_a, params.cloud_top_b);
    float min_base = min(params.cloud_base_a, params.cloud_base_b);
    vec3 origin = camera_position + vec3(centered.x, max_top, centered.y);
    vec3 dir = normalize(-params.sun_direction);

    float layer_depth = max_top - min_base;
    if (layer_depth <= 0.0) {
        uint cascade_offset = params.cascade_offsets[cascade_index];
        uint idx = cascade_offset + gid.y * cascade_resolution + gid.x;
        cloud_shadow_buffer.values[idx] = 1.0;
        return;
    }
    float step_count = 12.0;
    float step_size = layer_depth / step_count;
    float transmittance = 1.0;

    for (uint i = 0; i < uint(step_count); ++i) {
        float h = max_top - float(i) * step_size;
        vec3 sample_pos = vec3(origin.x, h, origin.z) + dir * (float(i) * step_size);
        float sigma = 0.0;

        if (h >= params.cloud_base_a && h <= params.cloud_top_a && params.density_scale_a > 0.0) {
            float height_frac = clamp((h - params.cloud_base_a) / max(params.cloud_top_a - params.cloud_base_a, 1.0), 0.0, 1.0);
            float weather_scale = 0.0001 * params.noise_scale_a;
            vec2 weather_uv = (sample_pos.xz * weather_scale) + params.wind_a * params.time * weather_scale;
            vec4 weather = texture(sampler2D(cloud_weather_map, cloud_weather_sampler), fract(weather_uv));
            float coverage = pow(weather_channel(weather, params.weather_channels_a.x), params.coverage_power);
            float thickness = weather_channel(weather, params.weather_channels_a.z);
            vec3 base_pos = sample_pos * (0.00025 * params.noise_scale_a);
            float base_noise = sample_noise(cloud_base_noise, cloud_base_sampler, base_pos, params.base_noise_size);
            float detail_noise = sample_noise(cloud_detail_noise, cloud_detail_sampler, base_pos * 4.0, params.detail_noise_size);
            float density = max(base_noise * coverage - (1.0 - thickness) * (1.0 - height_frac), 0.0);
            density = mix(density, density * detail_noise, 0.5);
            sigma += density * params.density_scale_a;
        }

        if (h >= params.cloud_base_b && h <= params.cloud_top_b && params.density_scale_b > 0.0) {
            float height_frac = clamp((h - params.cloud_base_b) / max(params.cloud_top_b - params.cloud_base_b, 1.0), 0.0, 1.0);
            float weather_scale = 0.0001 * params.noise_scale_b;
            vec2 weather_uv = (sample_pos.xz * weather_scale) + params.wind_b * params.time * weather_scale;
            vec4 weather = texture(sampler2D(cloud_weather_map, cloud_weather_sampler), fract(weather_uv));
            float coverage = pow(weather_channel(weather, params.weather_channels_b.x), params.coverage_power);
            float thickness = weather_channel(weather, params.weather_channels_b.z);
            vec3 base_pos = sample_pos * (0.00025 * params.noise_scale_b);
            float base_noise = sample_noise(cloud_base_noise, cloud_base_sampler, base_pos, params.base_noise_size);
            float detail_noise = sample_noise(cloud_detail_noise, cloud_detail_sampler, base_pos * 4.0, params.detail_noise_size);
            float density = max(base_noise * coverage - (1.0 - thickness) * (1.0 - height_frac), 0.0);
            density = mix(density, density * detail_noise, 0.5);
            sigma += density * params.density_scale_b;
        }

        transmittance *= exp(-sigma * step_size * params.shadow_strength);
        if (transmittance < 0.01) {
            break;
        }
    }

    uint cascade_offset = params.cascade_offsets[cascade_index];
    uint idx = cascade_offset + gid.y * cascade_resolution + gid.x;
    cloud_shadow_buffer.values[idx] = clamp(transmittance, 0.0, 1.0);
}
