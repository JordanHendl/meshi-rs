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
    float tile_height_step;
    float time;
    vec2 wind_dir;
    float wind_speed;
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

vec4 apply_light(vec3 base_color, vec3 normal, vec3 view_dir, vec3 world_pos) {
    if (meshi_bindless_lights.lights.length() == 0) {
        return vec4(base_color * 0.1, 0.5);
    }
    
    vec3 gain = vec3(0.0);
    float spec = 0.0;
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
    vec3 h = normalize(light_dir + v);
    float diffuse = max(dot(n, light_dir), 0.0);
    float specular = pow(max(dot(n, h), 0.0), 48.0);
    spec += specular;
    vec3 diffuse_term = diffuse * light_color * base_color;
    vec3 specular_term = specular * light_color;
      vec3 ambient_term = base_color * 0.08;

      gain += diffuse_term + specular_term + ambient_term * attenuation;
    }

    return vec4(gain, max(0.8, spec));
}

void main() {
    float shade = 0.4 + 0.6 * v_uv.y;
    vec3 base_color = vec3(0.0, 0.3, 0.6) * shade;

    vec3 n = normalize(v_normal);
    vec3 v = normalize(v_view_dir);
    float ndotv = clamp(dot(n, v), 0.0, 1.0);
    float fresnel = 0.02 + (1.0 - 0.02) * pow(1.0 - ndotv, 5.0);
    vec3 reflection_dir = reflect(-v, n);
    vec3 env_color = texture(samplerCube(ocean_env_map, ocean_env_sampler), reflection_dir).rgb;

    float slope = 1.0 - clamp(abs(n.y), 0.0, 1.0);
    float velocity = abs(v_velocity);
    float curvature = length(fwidth(n));
    float breaking = velocity * 0.35 + slope * 1.2;
    float foam_mask = smoothstep(0.55, 0.95, breaking + curvature * 0.4);
    vec3 foam_color = vec3(0.9, 0.95, 1.0) * foam_mask;

    vec4 color = apply_light(base_color, n, v, v_world_pos);
    color.rgb = mix(color.rgb, env_color, clamp(fresnel * 0.85, 0.0, 1.0));
    color.rgb += foam_color;

    out_color = vec4(color);
}
