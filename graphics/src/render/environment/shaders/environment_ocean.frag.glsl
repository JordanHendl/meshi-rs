#version 450

#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_samplerless_texture_functions : enable
layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec3 v_view_dir;
layout(location = 3) in vec3 v_world_pos;
layout(location = 4) in float v_velocity;
layout(location = 5) in vec2 v_flow;
layout(location = 0) out vec4 out_color;

layout(scalar, set = 0, binding = 0) readonly buffer OceanParams {
    uvec4 cascade_fft_sizes;
    vec4 cascade_patch_sizes;
    vec4 cascade_blend_ranges;
    uint vertex_resolution;
    uint camera_index;
    uint base_tile_radius;
    uint max_tile_radius;
    uint far_tile_radius;
    float tile_height_step;
    uint endless;
    float time;
    vec2 wind_dir;
    float wind_speed;
    float gerstner_amplitude;
    float fresnel_bias;
    float fresnel_strength;
    float foam_strength;
    float foam_threshold;
    float foam_advection_strength;
    float foam_decay_rate;
    float foam_noise_scale;
    vec2 current;
    vec3 _padding1;
    vec4 absorption_coeff;
    vec4 shallow_color;
    vec4 deep_color;
    vec4 scattering_color;
    float scattering_strength;
    float turbidity_depth;
    float refraction_strength;
    float ssr_strength;
    float ssr_max_distance;
    float ssr_thickness;
    uint ssr_steps;
    float debug_view;
    vec3 _padding2;
} params;


layout(scalar, set = 1, binding = 10) readonly buffer OceanShadowParams {
    uint shadow_cascade_count;
    uint shadow_resolution;
    uint shadow_padding0;
    uint shadow_padding1;
    vec4 shadow_splits;
} shadow_params;

layout(scalar, set = 2, binding = 0) readonly buffer OceanShadowMatrices {
    mat4 shadow_matrices[4];
} shadow_matrices;

layout(scalar, set = 3, binding = 11) readonly buffer OceanCloudShadowParams {
    uint shadow_enabled;
    uint shadow_cascade_count;
    uint shadow_resolution;
    uint _padding0;
    vec4 shadow_splits;
    vec4 shadow_cascade_extents;
    uvec4 shadow_cascade_resolutions;
    uvec4 shadow_cascade_offsets;
} cloud_shadow_params;

layout(set = 1, binding = 12) readonly buffer OceanCloudShadowBuffer {
    float values[];
} cloud_shadow_buffer;

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

layout(set = 1, binding = 3) uniform textureCube ocean_env_map;
layout(set = 1, binding = 4) uniform sampler ocean_env_sampler;
layout(set = 1, binding = 5) uniform texture2D ocean_scene_color;
layout(set = 1, binding = 6) uniform texture2D ocean_scene_depth;
layout(set = 1, binding = 7) uniform sampler ocean_scene_sampler;
layout(set = 1, binding = 8) uniform texture2D ocean_shadow_map;
layout(set = 1, binding = 9) uniform sampler ocean_shadow_sampler;

const float LIGHT_TYPE_DIRECTIONAL = 0.0;
const float PI = 3.14159265359;

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    float a = hash21(i);
    float b = hash21(i + vec2(1.0, 0.0));
    float c = hash21(i + vec2(0.0, 1.0));
    float d = hash21(i + vec2(1.0, 1.0));
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y;
}

float fbm(vec2 p) {
    float value = 0.0;
    float amplitude = 0.5;
    for (int i = 0; i < 3; ++i) {
        value += amplitude * noise(p);
        p *= 2.02;
        amplitude *= 0.5;
    }
    return value;
}

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

float linearize_depth(float depth, float near_plane, float far_plane) {
    float z = depth * 2.0 - 1.0;
    return (2.0 * near_plane * far_plane) / max(far_plane + near_plane - z * (far_plane - near_plane), 1e-4);
}

vec3 sample_scene_color(vec2 uv) {
    return texture(sampler2D(ocean_scene_color, ocean_scene_sampler), uv).rgb;
}

float sample_scene_depth(vec2 uv, float near_plane, float far_plane) {
    float depth = texture(sampler2D(ocean_scene_depth, ocean_scene_sampler), uv).r;
    return linearize_depth(depth, near_plane, far_plane);
}

vec2 compute_screen_uv(Camera cam, vec3 world_pos, out vec3 view_pos, out vec3 view_normal, vec3 normal) {
    mat4 view = inverse(cam.world_from_camera);
    mat4 proj = cam.projection;
    view_pos = (view * vec4(world_pos, 1.0)).xyz;
    view_normal = normalize((view * vec4(normal, 0.0)).xyz);
    vec4 clip = proj * vec4(view_pos, 1.0);
    vec2 ndc = clip.xy / max(clip.w, 1e-4);
    return ndc * 0.5 + 0.5;
}

vec3 compute_ssr(Camera cam, vec3 view_pos, vec3 view_normal, vec3 view_dir, out float hit) {
    hit = 0.0;
    float max_distance = max(params.ssr_max_distance, 0.0);
    if (params.ssr_strength <= 0.001 || max_distance <= 0.0) {
        return vec3(0.0);
    }
    mat4 proj = cam.projection;
    vec3 reflect_dir = normalize(reflect(-view_dir, view_normal));
    int max_steps = int(clamp(float(params.ssr_steps), 4.0, 64.0));
    float step_size = max_distance / float(max_steps);
    vec3 ray_pos = view_pos + view_normal * 0.15;
    vec3 color = vec3(0.0);
    for (int i = 0; i < 64; ++i) {
        if (i >= max_steps) {
            break;
        }
        ray_pos += reflect_dir * step_size;
        vec4 clip = proj * vec4(ray_pos, 1.0);
        if (clip.w <= 0.0) {
            break;
        }
        vec2 uv = (clip.xy / clip.w) * 0.5 + 0.5;
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            break;
        }
        float scene_depth = sample_scene_depth(uv, cam.near, cam.far);
        float ray_depth = -ray_pos.z;
        if (abs(ray_depth - scene_depth) <= max(params.ssr_thickness, 0.1)) {
            color = sample_scene_color(uv);
            hit = 1.0;
            break;
        }
    }
    return color;
}

uint select_shadow_cascade(float view_depth) {
    uint count = max(shadow_params.shadow_cascade_count, 1u);
    uint index = count - 1u;
    for (uint i = 0u; i < count; ++i) {
        if (view_depth <= shadow_params.shadow_splits[i]) {
            index = i;
            break;
        }
    }
    return index;
}

uint select_cloud_shadow_cascade(float view_depth) {
    uint count = max(cloud_shadow_params.shadow_cascade_count, 1u);
    uint index = count - 1u;
    for (uint i = 0u; i < count; ++i) {
        if (view_depth <= cloud_shadow_params.shadow_splits[i]) {
            index = i;
            break;
        }
    }
    return index;
}

float sample_shadow(vec3 world_pos, float view_depth, float bias) {
    uint cascade_count = max(shadow_params.shadow_cascade_count, 1u);
    uint shadow_res = max(shadow_params.shadow_resolution, 1u);
    if (shadow_res == 0u) {
        return 1.0;
    }

    uint cascade_index = select_shadow_cascade(view_depth);
    vec4 shadow_pos = shadow_matrices.shadow_matrices[cascade_index] * vec4(world_pos, 1.0);
    shadow_pos.xyz /= max(shadow_pos.w, 0.0001);
    vec2 uv = shadow_pos.xy * 0.5 + 0.5;
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return 1.0;
    }

    uint grid_x = (cascade_count > 1u) ? 2u : 1u;
    uint grid_y = (cascade_count > 2u) ? 2u : 1u;
    vec2 atlas_size = vec2(float(shadow_res * grid_x), float(shadow_res * grid_y));
    vec2 texel = vec2(1.0) / atlas_size;
    vec2 uv_adjusted = uv;
    uint tile_x = cascade_index % grid_x;
    uint tile_y = cascade_index / grid_x;
    uv_adjusted.x = uv.x / float(grid_x) + float(tile_x) / float(grid_x);
    uv_adjusted.y = uv.y / float(grid_y) + float(tile_y) / float(grid_y);

    float shadow = 0.0;
    float depth = shadow_pos.z;
    for (int x = -1; x <= 1; ++x) {
        for (int y = -1; y <= 1; ++y) {
            vec2 offset = vec2(x, y) * texel;
            vec2 coord = uv_adjusted + offset;
            ivec2 texel_coord = ivec2(coord * atlas_size);
            texel_coord = clamp(texel_coord, ivec2(0), ivec2(int(atlas_size.x) - 1, int(atlas_size.y) - 1));
            float map_depth = texelFetch(ocean_shadow_map, texel_coord, 0).x;
            shadow += (depth - bias) <= map_depth ? 1.0 : 0.0;
        }
    }
    return shadow / 9.0;
}

float sample_cloud_shadow(vec3 world_pos, float view_depth) {
    if (cloud_shadow_params.shadow_enabled == 0u) {
        return 1.0;
    }
    uint cascade_index = select_cloud_shadow_cascade(view_depth);
    float extent = cloud_shadow_params.shadow_cascade_extents[cascade_index];
    uint cascade_resolution = cloud_shadow_params.shadow_cascade_resolutions[cascade_index];
    if (cascade_resolution == 0u) {
        cascade_resolution = cloud_shadow_params.shadow_resolution;
    }
    cascade_resolution = max(cascade_resolution, 1u);
    vec2 uv = (world_pos.xz / extent) * 0.5 + 0.5;
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return 1.0;
    }
    uvec2 coord = uvec2(uv * float(cascade_resolution));
    coord = min(coord, uvec2(cascade_resolution - 1));
    uint cascade_offset = cloud_shadow_params.shadow_cascade_offsets[cascade_index];
    uint idx = cascade_offset + coord.y * cascade_resolution + coord.x;
    return cloud_shadow_buffer.values[idx];
}

vec3 apply_light(vec3 base_color, vec3 normal, vec3 view_dir, vec3 world_pos, float view_depth, float roughness, vec3 f0) {
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
        light_dir = dir_len > 0.0 ? -dir / dir_len : vec3(0.0, 1.0, 0.0);
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
    float specular_boost = mix(1.0, 1.35, 1.0 - roughness);
    specular *= specular_boost;
    vec3 k_s = f;
    vec3 k_d = (vec3(1.0) - k_s);
    vec3 diffuse = k_d * base_color / PI;

    float shadow_factor = 1.0;
    if (light_type == LIGHT_TYPE_DIRECTIONAL && shadow_params.shadow_resolution > 0u) {
        uint light_flags = floatBitsToUint(light.extra.x);
        bool casts_shadows = (light_flags & 1u) != 0u;
        if (casts_shadows) {
            float bias = max(0.0005, 0.002 * (1.0 - ndotl));
            shadow_factor = sample_shadow(world_pos, view_depth, bias);
        }
    }
    if (light_type == LIGHT_TYPE_DIRECTIONAL) {
        shadow_factor *= sample_cloud_shadow(world_pos, view_depth);
    }

    vec3 radiance = light_color * attenuation;
    gain += (diffuse + specular) * radiance * ndotl * shadow_factor;
    }

    return gain;
}

void main() {
    Camera cam = meshi_bindless_cameras.cameras[params.camera_index];
    float shade = 0.4 + 0.6 * v_uv.y;

    vec3 n = normalize(v_normal);
    vec3 v = normalize(v_view_dir);
    float ndotv = clamp(dot(n, v), 0.0, 1.0);
    float fresnel_bias = clamp(params.fresnel_bias, 0.0, 1.0);
    float fresnel_strength = max(params.fresnel_strength, 0.0);
    int debug_view = int(params.debug_view + 0.5);

    float slope = 1.0 - clamp(abs(n.y), 0.0, 1.0);
    float foam_threshold = clamp(params.foam_threshold, 0.0, 1.0);
    float foam_upper = min(1.0, foam_threshold + 0.2);
    float cresting = smoothstep(foam_threshold, foam_upper, slope);
    float foam_scale = max(params.foam_noise_scale, 0.001);
    float velocity_scale = 1.0 + abs(v_velocity) * 0.05;
    float advection = params.foam_advection_strength * velocity_scale * params.time;
    vec2 foam_uv = v_world_pos.xz * foam_scale + v_flow * advection;
    float foam_tex = fbm(foam_uv * 2.0);
    float foam_age = fract(params.time * max(params.foam_decay_rate, 0.0) + foam_tex);
    float foam_decay = smoothstep(1.0, 0.0, foam_age);
    float foam_mask = cresting * foam_tex * foam_decay;
    float foam_strength = max(params.foam_strength, 0.0);
    foam_mask *= foam_strength;
    vec3 foam_color = vec3(0.9, 0.95, 1.0) * foam_mask;

    if (debug_view == 1) {
        out_color = vec4(n * 0.5 + 0.5, 1.0);
        return;
    }
    if (debug_view == 2) {
        float height_scale = 1.0;
        float height = v_world_pos.y / height_scale;
        float height_norm = clamp(height * 0.5 + 0.5, 0.0, 1.0);
        vec3 height_color = mix(vec3(0.02, 0.2, 0.55), vec3(0.9, 0.4, 0.15), height_norm);
        out_color = vec4(height_color, 1.0);
        return;
    }
    if (debug_view == 3) {
        out_color = vec4(vec3(foam_mask), 1.0);
        return;
    }
    if (debug_view == 4) {
        float velocity = clamp(abs(v_velocity) / 1.0, 0.0, 1.0);
        out_color = vec4(vec3(velocity), 1.0);
        return;
    }

    float roughness = clamp(0.02 + slope * 0.35 + foam_mask * 0.2, 0.02, 0.8);
    vec3 f0 = vec3(fresnel_bias);
    vec3 fresnel = fresnel_schlick(ndotv, f0);

    vec3 view_pos;
    vec3 view_normal;
    vec2 screen_uv = compute_screen_uv(cam, v_world_pos, view_pos, view_normal, n);
    screen_uv = clamp(screen_uv, vec2(0.0), vec2(1.0));
    float view_depth = -view_pos.z;
    float scene_depth = sample_scene_depth(screen_uv, cam.near, cam.far);
    float thickness = max(scene_depth - view_depth, 0.0);
    float turbidity_depth = max(params.turbidity_depth, 0.1);
    float depth_factor = clamp(thickness / turbidity_depth, 0.0, 1.0);
    vec3 turbidity_color = mix(params.deep_color.xyz, params.shallow_color.xyz, depth_factor);
    vec3 base_color = turbidity_color * shade;

    float refraction_scale = params.refraction_strength * (0.35 + 0.65 * (1.0 - ndotv));
    vec2 refract_offset = view_normal.xy * refraction_scale;
    vec2 refract_uv = clamp(screen_uv + refract_offset, vec2(0.0), vec2(1.0));
    vec3 scene_color = sample_scene_color(refract_uv);
    vec3 absorption = exp(-params.absorption_coeff.xyz * thickness);
    vec3 refracted = scene_color * absorption + turbidity_color * (1.0 - absorption);
    vec3 scatter = params.scattering_color.xyz * (1.0 - exp(-params.scattering_strength * thickness));
    refracted += scatter;

    vec3 view_dir = normalize(-view_pos);
    float ssr_hit = 0.0;
    vec3 ssr_color = compute_ssr(cam, view_pos, view_normal, view_dir, ssr_hit);

    vec3 reflection_dir = normalize(reflect(-v, n));
    vec3 env_color = texture(samplerCube(ocean_env_map, ocean_env_sampler), reflection_dir).rgb;
    vec3 reflection_color = mix(env_color, ssr_color, ssr_hit * clamp(params.ssr_strength, 0.0, 1.0));
    vec3 specular_ibl = reflection_color * mix(fresnel, vec3(1.0), roughness * 0.2);
    vec3 diffuse_ibl = base_color * (1.0 - fresnel) * 0.08;

    vec3 surface = apply_light(base_color, n, v, v_world_pos, view_depth, roughness, f0);
    surface += diffuse_ibl * fresnel_strength;
    surface += specular_ibl * fresnel_strength;
    float reflect_factor = clamp(fresnel_strength * fresnel.r, 0.0, 1.0);
    vec3 color = mix(refracted, surface, reflect_factor);
    color += foam_color;
    float depth_opacity = clamp(thickness / turbidity_depth, 0.0, 1.0);
    float transparency = mix(0.2, 0.85, reflect_factor) + depth_opacity * 0.2 + foam_mask * 0.15;
    float alpha = clamp(transparency, 0.1, 0.98);

    out_color = vec4(color, alpha);
}
