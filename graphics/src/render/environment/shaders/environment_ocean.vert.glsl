#version 450
#extension GL_EXT_nonuniform_qualifier : enable

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
    uint camera_index;
    float _padding;
} params;

layout(set = 2, binding = 0) readonly buffer Cameras {
    mat4 world_from_camera;
    mat4 projection;
    vec2 viewport;
    float near;
    float far;
    float fov_y_radians;
    uint projection_kind;
    float _padding;
} meshi_bindless_cameras[];

layout(location = 0) out vec2 v_uv;

vec2 camera_position() {
    return meshi_bindless_cameras[params.camera_index].world_from_camera[3].xz;
}

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
    uint grid_resolution = max(params.vertex_resolution, 2);
    uint quad_index = gl_VertexIndex / 6;
    uint local_vertex = gl_VertexIndex % 6;
    uint quad_x = quad_index % (grid_resolution - 1);
    uint quad_y = quad_index / (grid_resolution - 1);
    vec2 quad_origin = vec2(quad_x, quad_y) / float(grid_resolution - 1);
    vec2 quad_size = vec2(1.0 / float(grid_resolution - 1));
    vec2 uv = quad_origin + vertex_uv(local_vertex) * quad_size;
    uint x = uint(uv.x * float(params.fft_size - 1));
    uint y = uint(uv.y * float(params.fft_size - 1));
    uint idx = y * params.fft_size + x;
    float height = ocean_waves.values[idx].x;

    vec2 local = (uv * 2.0 - 1.0) * params.patch_size;
    vec2 world = local + camera_position();
    vec4 position = vec4(world.x, height, world.y, 1.0);
    mat4 view = inverse(meshi_bindless_cameras[params.camera_index].world_from_camera);
    gl_Position = meshi_bindless_cameras[params.camera_index].projection * view * position;
    v_uv = uv;
}
