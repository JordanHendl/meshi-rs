#version 450

#extension GL_EXT_nonuniform_qualifier : enable
layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer OceanWaves {
    vec4 values[];
} ocean_waves;

layout(set = 1, binding = 0) readonly buffer OceanParams {
    uint fft_size;
    float time;
    float time_scale;
    float _padding0;
    vec2 wind_dir;
    float wind_speed;
    float _padding1;
} params;

void main() {
    uint x = gl_GlobalInvocationID.x;
    uint y = gl_GlobalInvocationID.y;
    if (x >= params.fft_size || y >= params.fft_size) {
        return;
    }

    uint idx = y * params.fft_size + x;
    vec2 uv = vec2(float(x), float(y)) / max(float(params.fft_size - 1), 1.0);
    vec2 wind = params.wind_dir;
    float wind_len = max(length(wind), 0.001);
    vec2 wind_dir = wind / wind_len;
    vec2 dir_b = normalize(wind_dir + vec2(0.6, 0.2));
    vec2 dir_c = normalize(wind_dir + vec2(-0.4, 0.7));
    vec2 dir_d = normalize(vec2(-wind_dir.y, wind_dir.x));
    float time = params.time * params.time_scale;
    float speed = max(params.wind_speed, 0.1);

    float height = 0.0;
    float amplitude = 0.35;
    float base_freq = 0.6;
    vec2 world = (uv - 0.5) * float(params.fft_size);

    for (int i = 0; i < 8; ++i) {
        float octave = pow(2.0, float(i));
        float freq = base_freq * octave;
        float phase_a = dot(world, wind_dir) * freq + time * speed * 0.15 * octave;
        float phase_b = dot(world, dir_b) * freq * 0.9 + time * speed * 0.12 * octave;
        float phase_c = dot(world, dir_c) * freq * 1.1 + time * speed * 0.1 * octave;
        float phase_d = dot(world, dir_d) * freq * 0.75 + time * speed * 0.08 * octave;
        float wave = sin(phase_a) * 0.23
            + sin(phase_b) * 0.2
            + cos(phase_c) * 0.15
            + sin(phase_d) * 0.1;
        height += wave * amplitude;
        amplitude *= 0.55;
    }

    float chop = sin(dot(world, wind_dir) * base_freq * 2.5 + time * speed * 0.9);
    height += chop * 0.07;
    height += sin(dot(world, dir_d) * base_freq * 6.0 + time * speed * 1.4) * 0.02;

    ocean_waves.values[idx] = vec4(height, 0.0, 0.0, 1.0);
}
