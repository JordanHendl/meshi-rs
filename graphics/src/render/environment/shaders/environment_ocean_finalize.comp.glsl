#version 450

#extension GL_EXT_scalar_block_layout : enable
layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0) readonly buffer OceanSpectrumSpatial {
    vec4 values[];
} spectrum_spatial;

layout(set = 0, binding = 1) buffer OceanWaves {
    vec4 values[];
} ocean_waves;

layout(scalar, set = 1, binding = 0) readonly buffer OceanFinalizeParams {
    uint fft_size;
    vec3 _padding;
} params;

float sample_height(uint x, uint y) {
    uint n = params.fft_size;
    uint idx = y * n + x;
    return spectrum_spatial.values[idx].x;
}

void main() {
    uint x = gl_GlobalInvocationID.x;
    uint y = gl_GlobalInvocationID.y;
    if (x >= params.fft_size || y >= params.fft_size) {
        return;
    }

    uint n = params.fft_size;
    float n_f = float(n);
    uint idx = y * n + x;
    vec4 value = spectrum_spatial.values[idx];
    float scale = 1.0 / max(n_f * n_f, 1.0);
    float height = value.x * scale;
    float velocity = value.z * scale;

    uint xl = (x + n - 1u) % n;
    uint xr = (x + 1u) % n;
    uint yd = (y + n - 1u) % n;
    uint yu = (y + 1u) % n;

    float height_l = sample_height(xl, y) * scale;
    float height_r = sample_height(xr, y) * scale;
    float height_d = sample_height(x, yd) * scale;
    float height_u = sample_height(x, yu) * scale;
    vec2 gradient = vec2(height_r - height_l, height_u - height_d) * 0.5 * n_f;
    ocean_waves.values[idx] = vec4(height, gradient.x, gradient.y, velocity);
}
