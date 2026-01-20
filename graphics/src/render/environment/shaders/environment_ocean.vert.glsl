#version 450
#extension GL_EXT_nonuniform_qualifier : enable
struct Camera {
    mat4 world_from_camera;
    mat4 projection;
    vec2 viewport;
    float near;
    float far;
    float fov_y_radians;
    uint projection_kind;
    float _padding;
};

vec3 camera_position(const Camera cam) {
    return cam.world_from_camera[3].xyz;
}

vec4 camera_rotation_quat(const Camera cam)
{
    // Extract rotation matrix (world space)
    mat3 m = mat3(cam.world_from_camera);

    // Standard matrix ï¿½ï¿½ï¿½ï¿½ï¿½uaternion conversion
    float trace = m[0][0] + m[1][1] + m[2][2];
    vec4 q;

    if (trace > 0.0) {
        float s = sqrt(trace + 1.0) * 2.0;
        q.w = 0.25 * s;
        q.x = (m[2][1] - m[1][2]) / s;
        q.y = (m[0][2] - m[2][0]) / s;
        q.z = (m[1][0] - m[0][1]) / s;
    }
    else if (m[0][0] > m[1][1] && m[0][0] > m[2][2]) {
        float s = sqrt(1.0 + m[0][0] - m[1][1] - m[2][2]) * 2.0;
        q.w = (m[2][1] - m[1][2]) / s;
        q.x = 0.25 * s;
        q.y = (m[0][1] + m[1][0]) / s;
        q.z = (m[0][2] + m[2][0]) / s;
    }
    else if (m[1][1] > m[2][2]) {
        float s = sqrt(1.0 + m[1][1] - m[0][0] - m[2][2]) * 2.0;
        q.w = (m[0][2] - m[2][0]) / s;
        q.x = (m[0][1] + m[1][0]) / s;
        q.y = 0.25 * s;
        q.z = (m[1][2] + m[2][1]) / s;
    }
    else {
        float s = sqrt(1.0 + m[2][2] - m[0][0] - m[1][1]) * 2.0;
        q.w = (m[1][0] - m[0][1]) / s;
        q.x = (m[0][2] + m[2][0]) / s;
        q.y = (m[1][2] + m[2][1]) / s;
        q.z = 0.25 * s;
    }

    return normalize(q);
}

vec3 camera_rotation(const Camera cam) {
    // Extract camera basis vectors (world space)
    vec3 right   =  cam.world_from_camera[0].xyz;
    vec3 up      =  cam.world_from_camera[1].xyz;
    vec3 forward = -cam.world_from_camera[2].xyz;

    // Yaw (around Y)
    float yaw = atan(forward.x, forward.z);

    // Pitch (around X)
    float pitch = asin(clamp(forward.y, -1.0, 1.0));

    // Roll (around Z)
    float roll = atan(right.y, up.y);

    return vec3(pitch, yaw, roll);
}

vec3 camera_forward(const Camera cam) {
    return -cam.world_from_camera[2].xyz;
}

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

layout(set = 1, binding = 1) readonly buffer SceneCameras {
  Camera cameras[];
} meshi_bindless_cameras;

layout(location = 0) out vec2 v_uv;

vec2 camera_position() {
  Camera c = meshi_bindless_cameras.cameras[params.camera_index];
    return camera_position(c).xy;
}

mat4 camera_view() {
    return meshi_bindless_cameras.cameras[params.camera_index].world_from_camera;
}

mat4 camera_proj() {
    return meshi_bindless_cameras.cameras[params.camera_index].projection;
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
    uint idx = min(y * params.fft_size + x, ocean_waves.values.length());

    float height = ocean_waves.values[idx].x;

    vec2 local = (uv * 2.0 - 1.0) * params.patch_size;
    vec2 world = local + camera_position();
    vec4 position = vec4(world.x, height, world.y, 1.0);
    mat4 view = camera_view();
    mat4 proj = camera_proj();
    gl_Position = proj * view * position;
    gl_Position.y = -gl_Position.y;
    v_uv = uv;
}
