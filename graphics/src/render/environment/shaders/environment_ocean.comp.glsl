#version 450

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer OceanWaves {
    vec4 values[];
} ocean_waves;

layout(set = 1, binding = 0) readonly buffer OceanParams {
    uint fft_size;
    float time;
    vec2 wind_dir;
    float wind_speed;
    float _padding;
} params;

void main() {
    uint x = gl_GlobalInvocationID.x;
    uint y = gl_GlobalInvocationID.y;
    if (x >= params.fft_size || y >= params.fft_size) {
        return;
    }

    uint idx = y * params.fft_size + x;
    float phase = (float(x + y) * 0.1) + params.time * 0.8;
    float wave = sin(phase) * params.wind_speed * 0.05;
    ocean_waves.values[idx] = vec4(wave, 0.0, 0.0, 1.0);
}
