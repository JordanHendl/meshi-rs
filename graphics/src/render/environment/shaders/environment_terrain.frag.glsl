#version 450

layout(location = 0) in vec3 v_color;
layout(location = 0) out vec4 out_color;

struct TerrainBlendInputs {
    vec3 base_color;
    vec3 layer_a;
    vec3 layer_b;
    vec3 layer_c;
    vec3 layer_d;
    vec4 weights;
};

vec3 blend_terrain_layers(TerrainBlendInputs inputs) {
    return inputs.base_color;
}

void main() {
    TerrainBlendInputs blend = TerrainBlendInputs(
        v_color,
        vec3(0.0),
        vec3(0.0),
        vec3(0.0),
        vec3(0.0),
        vec4(0.0)
    );
    vec3 blended = blend_terrain_layers(blend);
    out_color = vec4(1.0, 0.0, 0.0, 1.0);
}
