#version 450

#extension GL_EXT_nonuniform_qualifier : enable
layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec3 v_view_dir;
layout(location = 3) in vec3 v_world_pos;
layout(location = 0) out vec4 out_color;

layout(set = 1, binding = 0) readonly buffer OceanParams {
    uint fft_size;
    uint vertex_resolution;
    uint camera_index;
    uint _padding0;
    float patch_size;
    float time;
    vec2 wind_dir;
    float wind_speed;
    vec2 _padding1;
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

vec3 apply_light(vec3 base_color, vec3 normal, vec3 view_dir, vec3 world_pos) {
    if (meshi_bindless_lights.lights.length() == 0) {
        return base_color * 0.4;
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
    vec3 diffuse_term = diffuse * light_color * base_color;
    vec3 specular_term = specular * light_color;
      vec3 ambient_term = base_color * 0.08;

      gain += diffuse_term + specular_term + ambient_term * attenuation;
    }

    return gain;
}

void main() {
    float shade = 0.4 + 0.6 * v_uv.y;
    vec3 base_color = vec3(0.0, 0.3, 0.6) * shade;
    vec3 color = apply_light(base_color, v_normal, v_view_dir, v_world_pos);
    out_color = vec4(color, 0.5);
}
