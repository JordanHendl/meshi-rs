#version 450

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer CloudState {
    vec4 values[];
} cloud_state;

layout(set = 1, binding = 0) uniform Params {
    float time;
    float delta_time;
    uint resolution;
    uint padding;
} params;

void main() {
    uint x = gl_GlobalInvocationID.x;
    uint y = gl_GlobalInvocationID.y;
    if (x >= params.resolution || y >= params.resolution) {
        return;
    }

    uint idx = y * params.resolution + x;
    float n = fract(sin(dot(vec2(x, y), vec2(12.9898, 78.233))) * 43758.5453 + params.time);
    cloud_state.values[idx] = vec4(n, n, n, 1.0);
}
