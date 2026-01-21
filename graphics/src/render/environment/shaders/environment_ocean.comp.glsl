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
    float fft_size_f = max(float(params.fft_size), 1.0);
    vec2 uv = vec2(float(x), float(y)) / fft_size_f;
    vec2 wind = params.wind_dir;
    float wind_len = max(length(wind), 0.001);
    vec2 wind_dir = wind / wind_len;
    vec2 dir_b = normalize(wind_dir + vec2(0.6, 0.2));
    vec2 dir_c = normalize(wind_dir + vec2(-0.4, 0.7));
    vec2 dir_d = normalize(vec2(-wind_dir.y, wind_dir.x));
    float time = params.time * params.time_scale;
    float speed = max(params.wind_speed, 0.1);

    float height = 0.0;
    vec2 gradient = vec2(0.0);
    float two_pi = 6.28318530718;
    float base_amplitude = 0.18;
    float spectrum_scale = 0.9;
    float time_phase = time * speed * 0.12;

    for (int ky = -4; ky <= 4; ++ky) {
        for (int kx = -4; kx <= 4; ++kx) {
            if (kx == 0 && ky == 0) {
                continue;
            }

            vec2 k = vec2(float(kx), float(ky));
            float k2 = max(dot(k, k), 1.0);
            float k_len = sqrt(k2);
            vec2 k_dir = k / k_len;
            float alignment = max(dot(k_dir, wind_dir), 0.0);
            float amplitude = base_amplitude * exp(-k2 * 0.18) * (0.45 + 0.55 * alignment);
            float phase = two_pi * dot(k, uv);
            float dispersion = time_phase * (0.6 + 0.4 * alignment) * k_len;
            float angle = phase + dispersion;
            float wave = sin(angle);
            height += amplitude * wave;
            float cos_wave = cos(angle);
            gradient += amplitude * cos_wave * two_pi * k;
        }
    }

    height *= spectrum_scale;
    gradient *= spectrum_scale;

    ocean_waves.values[idx] = vec4(height, gradient.x, gradient.y, 1.0);
}
