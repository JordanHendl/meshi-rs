#version 450

#extension GL_EXT_scalar_block_layout : enable

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0) readonly buffer OceanSpectrumIn {
    vec4 values[];
} spectrum_in;

layout(set = 0, binding = 1) buffer OceanSpectrumOut {
    vec4 values[];
} spectrum_out;

layout(scalar, set = 1, binding = 0) readonly buffer OceanFftParams {
    uint fft_size;
    uint stage;
    uint direction;
    uint bit_reverse;
    float inverse;
    vec3 _padding;
} params;

uint reverse_bits(uint v, uint bits) {
    uint r = 0;
    for (uint i = 0; i < bits; ++i) {
        r = (r << 1) | (v & 1u);
        v >>= 1u;
    }
    return r;
}

vec2 complex_mul(vec2 a, vec2 b) {
    return vec2(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

vec4 apply_butterfly(vec4 even_val, vec4 odd_val, vec2 twiddle) {
    vec2 odd_h = complex_mul(odd_val.xy, twiddle);
    vec2 odd_v = complex_mul(odd_val.zw, twiddle);
    vec2 out_h = even_val.xy + odd_h;
    vec2 out_v = even_val.zw + odd_v;
    return vec4(out_h, out_v);
}

vec4 apply_butterfly_odd(vec4 even_val, vec4 odd_val, vec2 twiddle) {
    vec2 odd_h = complex_mul(odd_val.xy, twiddle);
    vec2 odd_v = complex_mul(odd_val.zw, twiddle);
    vec2 out_h = even_val.xy - odd_h;
    vec2 out_v = even_val.zw - odd_v;
    return vec4(out_h, out_v);
}

void main() {
    uint x = gl_GlobalInvocationID.x;
    uint y = gl_GlobalInvocationID.y;
    if (x >= params.fft_size || y >= params.fft_size) {
        return;
    }

    uint n = params.fft_size;
    uint log_n = uint(round(log2(float(n))));
    uint idx = y * n + x;

    if (params.bit_reverse == 1u) {
        uint bx = x;
        uint by = y;
        if (params.direction == 0u) {
            bx = reverse_bits(x, log_n);
        } else {
            by = reverse_bits(y, log_n);
        }
        uint src_idx = by * n + bx;
        spectrum_out.values[idx] = spectrum_in.values[src_idx];
        return;
    }

    uint m = 1u << (params.stage + 1u);
    uint half_m = m >> 1u;
    uint index = params.direction == 0u ? x : y;
    uint base = (index / m) * m;
    uint offset = index - base;
    uint even_index = base + (offset % half_m);
    uint odd_index = even_index + half_m;
    uint twiddle_index = offset % half_m;

    float angle = 6.28318530718 * float(twiddle_index) / float(m);
    float sign = params.inverse >= 0.5 ? 1.0 : -1.0;
    vec2 twiddle = vec2(cos(angle), sign * sin(angle));

    uint even_idx = params.direction == 0u ? y * n + even_index : even_index * n + x;
    uint odd_idx = params.direction == 0u ? y * n + odd_index : odd_index * n + x;
    vec4 even_val = spectrum_in.values[even_idx];
    vec4 odd_val = spectrum_in.values[odd_idx];

    vec4 result = (offset < half_m)
        ? apply_butterfly(even_val, odd_val, twiddle)
        : apply_butterfly_odd(even_val, odd_val, twiddle);

    spectrum_out.values[idx] = result;
}
