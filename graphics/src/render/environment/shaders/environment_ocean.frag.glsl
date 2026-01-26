#version 450

#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_samplerless_texture_functions : enable
layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec3 v_view_dir;
layout(location = 3) in vec3 v_world_pos;
layout(location = 4) in float v_velocity;
layout(location = 0) out vec4 out_color;

layout(set = 1, binding = 0) readonly buffer OceanParams {
    uvec4 cascade_fft_sizes;
    vec4 cascade_patch_sizes;
    vec4 cascade_blend_ranges;
    uint vertex_resolution;
    uint camera_index;
    uint base_tile_radius;
    uint max_tile_radius;
    uint far_tile_radius;
    float tile_height_step;
    float time;
    vec2 wind_dir;
    float wind_speed;
    float wave_amplitude;
    float gerstner_amplitude;
    float fresnel_bias;
    float fresnel_strength;
    float foam_strength;
    float foam_threshold;
    float _padding1;
} params;

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

layout(set = 1, binding = 3) uniform textureCube ocean_env_map;
layout(set = 1, binding = 4) uniform sampler ocean_env_sampler;

const float LIGHT_TYPE_DIRECTIONAL = 0.0;
const float PI = 3.14159265359;

vec3 fresnel_schlick(float cos_theta, vec3 f0) {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

float distribution_ggx(vec3 n, vec3 h, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float ndoth = max(dot(n, h), 0.0);
    float ndoth2 = ndoth * ndoth;
    float denom = ndoth2 * (a2 - 1.0) + 1.0;
    return a2 / max(PI * denom * denom, 1e-5);
}

float geometry_schlick_ggx(float ndotv, float roughness) {
    float r = roughness + 1.0;
    float k = (r * r) / 8.0;
    float denom = ndotv * (1.0 - k) + k;
    return ndotv / max(denom, 1e-5);
}

float geometry_smith(vec3 n, vec3 v, vec3 l, float roughness) {
    float ndotv = max(dot(n, v), 0.0);
    float ndotl = max(dot(n, l), 0.0);
    float ggx_v = geometry_schlick_ggx(ndotv, roughness);
    float ggx_l = geometry_schlick_ggx(ndotl, roughness);
    return ggx_v * ggx_l;
}

vec3 apply_light(vec3 base_color, vec3 normal, vec3 view_dir, vec3 world_pos, float roughness, vec3 f0) {
    if (meshi_bindless_lights.lights.length() == 0) {
        return base_color * 0.1;
    }

    vec3 gain = vec3(0.0);
    for(int i = 0; i < meshi_bindless_lights.lights.length(); ++i) {
    Light light = meshi_bindless_lights.lights[i];
    float light_type = light.position_type.w;
    vec3 light_color = light.color_intensity.rgb * light.color_intensity.w;
    vec3 light_dir;
    float attenuation = 1.0;
    if (light_type < 0.0) {
      continue;
    } else if (light_type == LIGHT_TYPE_DIRECTIONAL) {
        vec3 dir = light.direction_range.xyz;
        float dir_len = length(dir);
        light_dir = dir_len > 0.0 ? dir / dir_len : vec3(0.0, 1.0, 0.0);
    } else {
        vec3 to_light = light.position_type.xyz - world_pos;
        float distance = length(to_light);
        light_dir = distance > 0.0 ? to_light / distance : vec3(0.0, 1.0, 0.0);
        float range = max(light.direction_range.w, 0.001);
        float falloff = clamp(1.0 - (distance / range), 0.0, 1.0);
        attenuation = falloff * falloff;
    }

    vec3 n = normalize(normal);
    vec3 v = normalize(view_dir);
    vec3 l = normalize(light_dir);
    vec3 h = normalize(v + l);
    float ndotl = max(dot(n, l), 0.0);
    float ndotv = max(dot(n, v), 0.0);
    vec3 f = fresnel_schlick(max(dot(h, v), 0.0), f0);
    float d = distribution_ggx(n, h, roughness);
    float g = geometry_smith(n, v, l, roughness);
    vec3 numerator = d * g * f;
    float denom = max(4.0 * ndotv * ndotl, 1e-4);
    vec3 specular = numerator / denom;
    vec3 k_s = f;
    vec3 k_d = (vec3(1.0) - k_s);
    vec3 diffuse = k_d * base_color / PI;

    vec3 radiance = light_color * attenuation;
    gain += (diffuse + specular) * radiance * ndotl;
    }

    return gain;
}

void main() {
    float shade = 0.4 + 0.6 * v_uv.y;
    vec3 base_color = vec3(0.0, 0.3, 0.6) * shade;

    vec3 n = normalize(v_normal);
    vec3 v = normalize(v_view_dir);
    float ndotv = clamp(dot(n, v), 0.0, 1.0);
    float fresnel_bias = clamp(params.fresnel_bias, 0.0, 1.0);
    float fresnel_strength = max(params.fresnel_strength, 0.0);

    float slope = 1.0 - clamp(abs(n.y), 0.0, 1.0);
    float velocity = abs(v_velocity);
    float curvature = length(fwidth(n));
    float breaking = velocity * 0.35 + slope * 1.2;
    float foam_threshold = clamp(params.foam_threshold, 0.0, 1.0);
    float foam_upper = min(1.0, foam_threshold + 0.4);
    float foam_mask = smoothstep(foam_threshold, foam_upper, breaking + curvature * 0.4);
    float foam_strength = max(params.foam_strength, 0.0);
    foam_mask *= foam_strength;
    vec3 foam_color = vec3(0.9, 0.95, 1.0) * foam_mask;

    float roughness = clamp(0.02 + slope * 0.35 + foam_mask * 0.2, 0.02, 0.8);
    vec3 f0 = vec3(fresnel_bias);
    vec3 fresnel = fresnel_schlick(ndotv, f0);

    vec3 reflection_dir = normalize(reflect(-v, n));
    vec3 env_color = texture(samplerCube(ocean_env_map, ocean_env_sampler), reflection_dir).rgb;
    vec3 specular_ibl = env_color * mix(fresnel, vec3(1.0), roughness * 0.2);
    vec3 diffuse_ibl = base_color * (1.0 - fresnel) * 0.08;

    vec3 color = apply_light(base_color, n, v, v_world_pos, roughness, f0);
    color += (diffuse_ibl + specular_ibl) * fresnel_strength;
    color += foam_color;
    float transparency = mix(0.25, 0.85, fresnel_strength * ndotv) + foam_mask * 0.15;
    float alpha = clamp(transparency, 0.1, 0.98);

    out_color = vec4(color, alpha);
}
