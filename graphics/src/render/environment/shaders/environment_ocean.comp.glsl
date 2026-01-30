#version 450
#extension GL_EXT_scalar_block_layout : disable
#extension GL_EXT_nonuniform_qualifier : enable
layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer OceanWaves {
    vec4 values[];
} ocean_waves;

layout(std430, set = 1, binding = 0) readonly buffer OceanParams {
    uint fft_size;
    float time;
    float time_scale;
    float wave_amplitude;
    vec2 wind_dir;
    float wind_speed;
    float capillary_strength;
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
    float velocity = 0.0;
    float two_pi = 6.28318530718;
    float gravity = 9.81;
    float spectrum_scale = 1.1;
    float amplitude_scale = max(params.wave_amplitude, 0.0);
    float base_amplitude = 0.55 * amplitude_scale;
    float capillary_amplitude = 0.18 * max(params.capillary_strength, 0.0);
    float capillary_scale = 3.5;
    float capillary_term = 0.000074;
    float time_phase = time * speed * 0.12;

    for (int ky = -12; ky <= 12; ++ky) {
        for (int kx = -12; kx <= 12; ++kx) {
            if (kx == 0 && ky == 0) {
                continue;
            }

            vec2 k_base = vec2(float(kx), float(ky));
            float k2_base = dot(k_base, k_base);
            float k_len_base = max(sqrt(k2_base), 0.001);
            vec2 k_dir = k_base / k_len_base;
            float alignment = max(dot(k_dir, wind_dir), 0.0);
            float alignment_spread = alignment * alignment;

            float L = max(speed * speed / gravity, 0.001);
            float damping = 0.0012;
            float l = L * damping;
            float k2 = max(k2_base, 0.0001);
            float phillips = exp(-1.0 / (k2 * L * L)) / (k2 * k2);
            phillips *= alignment_spread * exp(-k2 * l * l);
            float amplitude = base_amplitude * sqrt(max(phillips, 0.0));

            float phase_rand = fract(sin(dot(k_base, vec2(127.1, 311.7))) * 43758.5453);
            float phase = two_pi * (dot(k_base, uv) + phase_rand);
            float omega = sqrt(gravity * k_len_base);
            float dispersion = time_phase * omega;
            float angle = phase + dispersion;
            float wave = sin(angle);
            float cos_wave = cos(angle);
            height += amplitude * wave;
            gradient += amplitude * cos_wave * two_pi * k_base;
            velocity += amplitude * cos_wave * omega;

            vec2 k_cap = k_base * capillary_scale;
            float k2_cap = dot(k_cap, k_cap);
            float k_len_cap = max(sqrt(k2_cap), 0.001);
            vec2 k_cap_dir = k_cap / k_len_cap;
            float cap_align = max(dot(k_cap_dir, wind_dir), 0.0);
            float cap_spread = cap_align * cap_align;
            float cap_phillips = exp(-1.0 / (k2_cap * L * L)) / (k2_cap * k2_cap);
            cap_phillips *= cap_spread * exp(-k2_cap * l * l * 12.0);
            float cap_amplitude = capillary_amplitude * sqrt(max(cap_phillips, 0.0));
            float cap_phase_rand = fract(sin(dot(k_base, vec2(269.5, 183.3))) * 41583.123);
            float cap_phase = two_pi * (dot(k_cap, uv) + cap_phase_rand);
            float cap_omega = sqrt(gravity * k_len_cap + capillary_term * k_len_cap * k_len_cap * k_len_cap);
            float cap_dispersion = time_phase * cap_omega;
            float cap_angle = cap_phase + cap_dispersion;
            float cap_wave = sin(cap_angle);
            float cap_cos = cos(cap_angle);
            height += cap_amplitude * cap_wave;
            gradient += cap_amplitude * cap_cos * two_pi * k_cap;
            velocity += cap_amplitude * cap_cos * cap_omega;
        }
    }

    height *= spectrum_scale;
    gradient *= spectrum_scale;
    velocity *= spectrum_scale;

    ocean_waves.values[idx] = vec4(height, gradient.x, gradient.y, velocity);
}
