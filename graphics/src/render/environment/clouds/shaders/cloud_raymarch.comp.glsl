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

layout(set = 0, binding = 0, scalar) uniform CloudRaymarchParams {
    uvec2 output_resolution;
    uvec3 base_noise_size;
    uvec3 detail_noise_size;
    uint weather_map_size;
    uint frame_index;
    uint shadow_resolution;
    uint shadow_cascade_count;
    float shadow_cascade_splits[4];
    float shadow_cascade_extents[4];
    uint shadow_cascade_resolutions[4];
    uint shadow_cascade_offsets[4];
    float shadow_cascade_strengths[4];
    uint camera_index;
    uvec3 weather_channels_a;
    uint debug_view;
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
    uvec3 weather_channels_b;
    uint step_count;
    uint light_step_count;
    float phase_g;
    float multi_scatter_strength;
    uint multi_scatter_respects_shadow;
    vec3 sun_radiance;
    float shadow_strength;
    float time;
    float coverage_power;
    float detail_strength;
    float curl_strength;
    float jitter_strength;
    float epsilon;
    vec3 sun_direction;
    uint use_shadow_map;
    float shadow_extent;
    float atmosphere_view_strength;
    float atmosphere_view_extinction;
    float atmosphere_light_transmittance;
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
layout(set = 0, binding = 14) uniform textureCube cloud_environment_map;
layout(set = 0, binding = 15) uniform sampler cloud_environment_sampler;
layout(set = 1, binding = 1) readonly buffer SceneCameras {
    Camera cameras[];
} meshi_bindless_cameras;
struct Light {
    vec4 position_type;
    vec4 direction_range;
    vec4 color_intensity;
    vec4 spot_area;
    vec4 extra;
};

layout(set = 1, binding = 2) readonly buffer SceneLights {
    Light lights[];
} meshi_bindless_lights;

const float LIGHT_TYPE_DIRECTIONAL = 0.0;
const float LIGHT_TYPE_POINT = 1.0;
const float LIGHT_TYPE_SPOT = 2.0;
const float LIGHT_TYPE_RECT = 3.0;

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
    vec2 wrapped = fract(uv);
    return texture(sampler2D(cloud_weather_map, cloud_weather_sampler), wrapped);
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

float phase_hg(float cos_theta, float g) {
    float g2 = g * g;
    float denom = pow(1.0 + g2 - 2.0 * g * cos_theta, 1.5);
    return (1.0 - g2) / max(4.0 * 3.14159265 * denom, 1e-3);
}

uint select_shadow_cascade(float view_depth) {
    uint count = max(params.shadow_cascade_count, 1u);
    for (uint i = 0u; i < count; ++i) {
        if (view_depth <= params.shadow_cascade_splits[i]) {
            return i;
        }
    }
    return count - 1u;
}

float sample_shadow(vec3 world_pos, float view_depth) {
    uint cascade_index = select_shadow_cascade(view_depth);
    float shadow_extent = params.shadow_cascade_extents[cascade_index];
    uint cascade_resolution = params.shadow_cascade_resolutions[cascade_index];
    if (cascade_resolution == 0u) {
        cascade_resolution = params.shadow_resolution;
    }
    vec2 uv = (world_pos.xz / shadow_extent) * 0.5 + 0.5;
    uv = clamp(uv, vec2(0.0), vec2(1.0));
    uvec2 coord = uvec2(uv * float(cascade_resolution));
    coord = min(coord, uvec2(cascade_resolution - 1));
    uint cascade_offset = params.shadow_cascade_offsets[cascade_index];
    uint idx = cascade_offset + coord.y * cascade_resolution + coord.x;
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

float atmosphere_light_distance(vec3 sample_pos, vec3 light_dir, float max_dist, float light_type) {
    if (light_type == LIGHT_TYPE_DIRECTIONAL) {
        float denom = max(abs(light_dir.y), 0.05);
        float height = max(sample_pos.y, 0.0);
        return height / denom;
    }
    return max_dist;
}

float atmosphere_light_transmittance(vec3 sample_pos, vec3 light_dir, float max_dist, float light_type) {
    float distance = atmosphere_light_distance(sample_pos, light_dir, max_dist, light_type);
    float transmittance = exp(-params.atmosphere_view_extinction * distance);
    return mix(1.0, transmittance, params.atmosphere_light_transmittance);
}

vec3 sample_environment(vec3 direction) {
    return texture(samplerCube(cloud_environment_map, cloud_environment_sampler), direction).rgb;
}

vec3 light_direction(Light light, vec3 sample_pos, out float attenuation, out float max_dist, out float spot_factor) {
    float light_type = light.position_type.w;
    spot_factor = 1.0;
    if (light_type == LIGHT_TYPE_DIRECTIONAL) {
        vec3 dir = light.direction_range.xyz;
        float dir_len = length(dir);
        attenuation = 1.0;
        max_dist = 5000.0;
        return dir_len > 0.0 ? normalize(-dir) : vec3(0.0, 1.0, 0.0);
    }

    vec3 to_light = light.position_type.xyz - sample_pos;
    float distance = length(to_light);
    vec3 light_dir = distance > 0.0 ? to_light / distance : vec3(0.0, 1.0, 0.0);
    float range = max(light.direction_range.w, 0.001);
    float falloff = clamp(1.0 - (distance / range), 0.0, 1.0);
    attenuation = falloff * falloff;
    max_dist = distance;

    if (light_type == LIGHT_TYPE_SPOT) {
        vec3 spot_dir = normalize(light.direction_range.xyz);
        float inner = light.spot_area.x;
        float outer = light.spot_area.y;
        float spot_cos = dot(normalize(-light_dir), spot_dir);
        spot_factor = smoothstep(outer, inner, spot_cos);
    }

    return light_dir;
}

struct LayerResult {
    vec3 color;
    float trans;
    float depth;
    float weight;
    float steps;
};

LayerResult march_layer(
    vec3 camera_position,
    vec3 camera_forward,
    mat4 view,
    vec3 ray_dir,
    uvec2 gid,
    float cloud_base,
    float cloud_top,
    float density_scale,
    float noise_scale,
    vec2 wind,
    uvec3 weather_channels
) {
    LayerResult result;
    result.color = vec3(0.0);
    result.trans = 1.0;
    result.depth = 0.0;
    result.weight = 0.0;
    result.steps = 0.0;
    if (cloud_top <= cloud_base || density_scale <= 0.0) {
        return result;
    }

    float t0 = (cloud_base - camera_position.y) / ray_dir.y;
    float t1 = (cloud_top - camera_position.y) / ray_dir.y;
    if (t0 > t1) {
        float temp = t0;
        t0 = t1;
        t1 = temp;
    }
    if (t1 <= 0.0) {
        return result;
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
        vec3 sample_pos = camera_position + ray_dir * t;
        float height_frac = clamp((sample_pos.y - cloud_base) / (cloud_top - cloud_base), 0.0, 1.0);

        float weather_scale = 0.0001 * noise_scale;
        vec2 weather_uv = sample_pos.xz * weather_scale + wind * params.time * weather_scale;
        vec4 weather = sample_weather(weather_uv);
        float coverage = pow(weather_channel(weather, weather_channels.x), params.coverage_power);
        float type = weather_channel(weather, weather_channels.y);
        float thickness = weather_channel(weather, weather_channels.z);

        vec3 base_pos = sample_pos * (0.00025 * noise_scale) + vec3(wind * params.time * 0.01, 0.0);
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
            float sigma_t = density * density_scale;
            float step_trans = exp(-sigma_t * step_size);
            vec3 scatter_single = vec3(0.0);
            vec3 scatter_multi = vec3(0.0);
            int light_count = meshi_bindless_lights.lights.length();
            float multi_strength = params.multi_scatter_strength;
            bool show_single = params.debug_view == 9u;
            bool show_multi = params.debug_view == 10u;
            if (show_single) {
                multi_strength = 0.0;
            }
            if (light_count == 0) {
                vec3 sun_dir = normalize(params.sun_direction);
                vec3 env_tint = sample_environment(sun_dir);
                float phase = phase_hg(dot(ray_dir, sun_dir), params.phase_g);
                float light_trans = light_march(sample_pos, sun_dir, 5000.0, params.light_step_count, sigma_t, params.shadow_strength);
                float atmosphere_trans = atmosphere_light_transmittance(sample_pos, sun_dir, 5000.0, LIGHT_TYPE_DIRECTIONAL);
                vec3 base = params.sun_radiance * env_tint * phase * light_trans * atmosphere_trans;
                float shadow_gate = (params.multi_scatter_respects_shadow == 1u) ? light_trans : 1.0;
                float multi_gain = 1.0 + multi_strength * (1.0 - step_trans) * shadow_gate;
                scatter_single += base;
                scatter_multi += base * (multi_gain - 1.0);
            } else {
                for (int light_index = 0; light_index < light_count; ++light_index) {
                    Light light = meshi_bindless_lights.lights[light_index];
                    float light_type = light.position_type.w;
                    if (light_type < 0.0) {
                        continue;
                    }
                    float attenuation;
                    float max_dist;
                    float spot_factor;
                    vec3 light_dir = light_direction(light, sample_pos, attenuation, max_dist, spot_factor);
                    float light_trans = 1.0;
                    vec3 sun_dir = normalize(params.sun_direction);
                    if (params.use_shadow_map == 1u && light_type == LIGHT_TYPE_DIRECTIONAL && dot(light_dir, sun_dir) > 0.95) {
                        float view_depth = -(view * vec4(sample_pos, 1.0)).z;
                        uint cascade_index = select_shadow_cascade(view_depth);
                        float cascade_strength = params.shadow_cascade_strengths[cascade_index];
                        light_trans = sample_shadow(sample_pos, view_depth) * cascade_strength;
                    } else {
                        light_trans = light_march(sample_pos, light_dir, max_dist, params.light_step_count, sigma_t, params.shadow_strength);
                    }
                    float phase = phase_hg(dot(ray_dir, light_dir), params.phase_g);
                    vec3 light_color = light.color_intensity.rgb * light.color_intensity.w;
                    if (light_type == LIGHT_TYPE_DIRECTIONAL) {
                        light_color *= sample_environment(light_dir);
                    }
                    float atmosphere_trans = atmosphere_light_transmittance(sample_pos, light_dir, max_dist, light_type);
                    vec3 base = light_color * phase * light_trans * attenuation * spot_factor * atmosphere_trans;
                    float shadow_gate = (params.multi_scatter_respects_shadow == 1u) ? light_trans : 1.0;
                    float multi_gain = 1.0 + multi_strength * (1.0 - step_trans) * shadow_gate;
                    scatter_single += base;
                    scatter_multi += base * (multi_gain - 1.0);
                }
            }
            vec3 scatter = scatter_single + scatter_multi;
            if (show_multi) {
                scatter = scatter_multi;
            }
            color += transmittance * scatter * (1.0 - step_trans);
            transmittance *= step_trans;
            float forward_dot = max(dot(ray_dir, camera_forward), 0.0);
            float view_depth = t * forward_dot;
            depth_accum += view_depth * (1.0 - step_trans);
            weight_accum += (1.0 - step_trans);
        }

        steps_used += 1.0;
        t += step_size;
        if (transmittance < params.epsilon) {
            break;
        }
    }

    result.color = color;
    result.trans = clamp(transmittance, 0.0, 1.0);
    result.depth = weight_accum > 0.0 ? depth_accum / weight_accum : 0.0;
    result.weight = weight_accum;
    result.steps = steps_used / step_count;
    return result;
}

void main() {
    uvec2 gid = gl_GlobalInvocationID.xy;
    if (gid.x >= params.output_resolution.x || gid.y >= params.output_resolution.y) {
        return;
    }

    Camera camera = meshi_bindless_cameras.cameras[params.camera_index];
    vec3 camera_position = camera.world_from_camera[3].xyz;
    vec3 camera_forward = normalize(-camera.world_from_camera[2].xyz);
    mat4 view = inverse(camera.world_from_camera);
    mat4 view_proj = camera.projection * view;
    mat4 inv_view_proj = inverse(view_proj);

    uint idx = gid.y * params.output_resolution.x + gid.x;

    vec2 uv = (vec2(gid) + 0.5) / vec2(params.output_resolution);
    uv.y = 1.0 - uv.y;
    vec2 ndc = uv * 2.0 - 1.0;
    vec4 clip = vec4(ndc, 1.0, 1.0);
    vec4 world = inv_view_proj * clip;
    world.xyz /= world.w;
    vec3 ray_dir = normalize(world.xyz - camera_position);

    LayerResult layer_a = march_layer(
        camera_position,
        camera_forward,
        view,
        ray_dir,
        gid,
        params.cloud_base_a,
        params.cloud_top_a,
        params.density_scale_a,
        params.noise_scale_a,
        params.wind_a,
        params.weather_channels_a
    );

    LayerResult layer_b = march_layer(
        camera_position,
        camera_forward,
        view,
        ray_dir,
        gid,
        params.cloud_base_b,
        params.cloud_top_b,
        params.density_scale_b,
        params.noise_scale_b,
        params.wind_b,
        params.weather_channels_b
    );

    if (params.debug_view == 7u) {
        layer_b.color = vec3(0.0);
        layer_b.trans = 1.0;
        layer_b.depth = 0.0;
        layer_b.weight = 0.0;
    }
    if (params.debug_view == 8u) {
        layer_a.color = vec3(0.0);
        layer_a.trans = 1.0;
        layer_a.depth = 0.0;
        layer_a.weight = 0.0;
    }

    vec3 color = vec3(0.0);
    float transmittance = 1.0;
    float depth = 0.0;
    float steps_used = 0.0;
    bool has_a = layer_a.weight > 0.0;
    bool has_b = layer_b.weight > 0.0;

    if (has_a && has_b) {
        bool a_first = layer_a.depth <= layer_b.depth;
        if (a_first) {
            color = layer_a.color + layer_a.trans * layer_b.color;
            transmittance = layer_a.trans * layer_b.trans;
            depth = layer_a.depth;
        } else {
            color = layer_b.color + layer_b.trans * layer_a.color;
            transmittance = layer_b.trans * layer_a.trans;
            depth = layer_b.depth;
        }
        steps_used = (layer_a.steps + layer_b.steps) * 0.5;
    } else if (has_a) {
        color = layer_a.color;
        transmittance = layer_a.trans;
        depth = layer_a.depth;
        steps_used = layer_a.steps;
    } else if (has_b) {
        color = layer_b.color;
        transmittance = layer_b.trans;
        depth = layer_b.depth;
        steps_used = layer_b.steps;
    }

    cloud_color_buffer.values[idx] = vec4(color, 1.0);
    cloud_transmittance_buffer.values[idx] = clamp(transmittance, 0.0, 1.0);
    cloud_depth_buffer.values[idx] = depth;
    cloud_steps_buffer.values[idx] = steps_used;
}
