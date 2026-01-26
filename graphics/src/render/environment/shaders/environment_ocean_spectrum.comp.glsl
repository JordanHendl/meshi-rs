#version 450

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer OceanSpectrum {
    vec4 values[];
} ocean_spectrum;

layout(set = 1, binding = 0) readonly buffer OceanSpectrumParams {
    uint fft_size;
    float time;
    float time_scale;
    float wave_amplitude;
    vec2 wind_dir;
    float wind_speed;
    float capillary_strength;
    float patch_size;
    float fetch_length;
    vec2 swell_dir;
    float spectrum_scale;
    float swell_strength;
    float depth_meters;
    float depth_damping;
} params;

float hash11(float n) {
    return fract(sin(n) * 43758.5453123);
}

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

vec2 gaussian_random(vec2 seed) {
    float u1 = max(hash21(seed), 0.0001);
    float u2 = hash21(seed + vec2(17.3, 0.7));
    float r = sqrt(-2.0 * log(u1));
    float theta = 6.28318530718 * u2;
    return r * vec2(cos(theta), sin(theta));
}

float phillips_spectrum(vec2 k, vec2 wind_dir, float wind_speed, float amplitude) {
    float k_len = length(k);
    if (k_len < 0.0001) {
        return 0.0;
    }
    float g = 9.81;
    float L = (wind_speed * wind_speed) / g;
    float k_len2 = k_len * k_len;
    float k_len4 = k_len2 * k_len2;
    float k_dot_w = max(dot(normalize(k), wind_dir), 0.0);
    float alignment = k_dot_w * k_dot_w;
    float damping = 0.0012;
    float l = L * damping;
    float phillips = exp(-1.0 / (k_len2 * L * L)) / k_len4;
    phillips *= alignment * exp(-k_len2 * l * l);
    return amplitude * phillips;
}

float jonswap_spectrum(vec2 k, vec2 wind_dir, float wind_speed, float amplitude, float fetch_length) {
    float k_len = length(k);
    if (k_len < 0.0001) {
        return 0.0;
    }
    float g = 9.81;
    float omega = sqrt(g * k_len);
    float omega_p = max(0.855 * g / max(wind_speed, 0.1), 0.1);
    if (fetch_length > 1.0) {
        float denom = max(wind_speed * fetch_length, 0.1);
        float omega_fetch = 22.0 * pow((g * g) / denom, 0.3333333);
        omega_p = max(omega_fetch, 0.05);
    }
    float sigma = omega <= omega_p ? 0.07 : 0.09;
    float gamma = 3.3;
    float alpha = 0.0081;
    float r = exp(-pow(omega - omega_p, 2.0) / (2.0 * sigma * sigma * omega_p * omega_p));
    float peak = pow(gamma, r);
    float s = alpha * g * g * exp(-1.25 * pow(omega_p / omega, 4.0)) / pow(omega, 5.0);
    float directional = pow(max(dot(normalize(k), wind_dir), 0.0), 2.0);
    float dk = g / (2.0 * omega);
    return amplitude * s * peak * directional * max(dk, 0.0);
}

void main() {
    uint x = gl_GlobalInvocationID.x;
    uint y = gl_GlobalInvocationID.y;
    if (x >= params.fft_size || y >= params.fft_size) {
        return;
    }

    uint idx = y * params.fft_size + x;
    float fft_size_f = float(params.fft_size);
    float half_n = fft_size_f * 0.5;
    vec2 grid = vec2(float(x), float(y)) - vec2(half_n);
    float patch_size = max(params.patch_size, 0.01);
    float k_scale = 6.28318530718 / patch_size;
    vec2 k = grid * k_scale;
    float k_len = length(k);
    vec2 wind_dir = normalize(params.wind_dir + vec2(0.0001, 0.0001));
    vec2 swell_dir = normalize(params.swell_dir + vec2(0.0001, 0.0001));
    float wind_speed = max(params.wind_speed, 0.1);
    float amplitude = max(params.wave_amplitude, 0.0) * max(params.spectrum_scale, 0.0);
    float swell_strength = clamp(params.swell_strength, 0.0, 1.0);
    vec2 dominant_dir = normalize(mix(wind_dir, swell_dir, swell_strength));

    float phillips = phillips_spectrum(k, dominant_dir, wind_speed, amplitude * 0.6);
    float jonswap = jonswap_spectrum(k, dominant_dir, wind_speed, amplitude * 0.4, params.fetch_length);
    float spectrum = max(phillips + jonswap, 0.0);
    float depth = max(params.depth_meters, 0.0);
    if (depth > 0.0) {
        float depth_term = tanh(k_len * max(depth, 0.01));
        float damping = clamp(params.depth_damping, 0.0, 1.0);
        float depth_atten = mix(1.0, depth_term, damping);
        spectrum *= depth_atten;
    }

    vec2 seed = vec2(float(x), float(y));
    vec2 gaussian = gaussian_random(seed);
    float h0_scale = sqrt(spectrum * 0.5);
    vec2 h0 = gaussian * h0_scale;

    vec2 gaussian_neg = gaussian_random(-seed);
    vec2 h0_neg = gaussian_neg * h0_scale;

    float g = 9.81;
    float capillary = max(params.capillary_strength, 0.0) * 0.000074;
    float omega = sqrt(g * k_len + capillary * k_len * k_len * k_len);
    float time = params.time * params.time_scale;
    float phase = omega * time;
    float cos_phase = cos(phase);
    float sin_phase = sin(phase);

    vec2 exp_pos = vec2(cos_phase, sin_phase);
    vec2 exp_neg = vec2(cos_phase, -sin_phase);
    vec2 h_pos = vec2(
        h0.x * exp_pos.x - h0.y * exp_pos.y,
        h0.x * exp_pos.y + h0.y * exp_pos.x
    );
    vec2 h_neg = vec2(
        h0_neg.x * exp_neg.x - h0_neg.y * exp_neg.y,
        h0_neg.x * exp_neg.y + h0_neg.y * exp_neg.x
    );
    vec2 h = h_pos + vec2(h_neg.x, -h_neg.y);
    vec2 v = h * omega;

    ocean_spectrum.values[idx] = vec4(h, v);
}
