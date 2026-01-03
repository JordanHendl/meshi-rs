#version 450

layout(set = 0, binding = 0) readonly buffer OceanWaves {
    vec4 values[];
} ocean_waves;

layout(set = 1, binding = 0) readonly buffer OceanParams {
    uint fft_size;
    uint vertex_resolution;
    float patch_size;
    float time;
    vec2 wind_dir;
    float wind_speed;
    float _padding;
} params;

layout(location = 0) out vec2 v_uv;

vec2 vertex_uv(uint vertex_id) {
    vec2 positions[6] = vec2[](
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, 1.0),
        vec2(1.0, 0.0),
        vec2(1.0, 1.0)
    );

    return positions[vertex_id];
}

void main() {
    vec2 uv = vertex_uv(gl_VertexIndex);
    uint x = uint(uv.x * float(params.fft_size - 1));
    uint y = uint(uv.y * float(params.fft_size - 1));
    uint idx = y * params.fft_size + x;
    float height = ocean_waves.values[idx].x;

    vec2 pos = uv * 2.0 - 1.0;
    gl_Position = vec4(pos.x, height, pos.y, 1.0);
    v_uv = uv;
}
