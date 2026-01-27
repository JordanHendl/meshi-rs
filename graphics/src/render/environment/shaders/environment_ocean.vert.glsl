#version 450
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable

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

vec2 safe_normalize(vec2 v) {
    float len = max(length(v), 0.001);
    return v / len;
}

layout(set = 0, binding = 0) readonly buffer OceanWaves {
    vec4 values[];
} ocean_waves[];

layout(scalar, set = 1, binding = 0) readonly buffer OceanParams {
    uvec4 cascade_fft_sizes;
    vec4 cascade_patch_sizes;
    vec4 cascade_blend_ranges;
    uint vertex_resolution;
    uint camera_index;
    uint base_tile_radius;
    uint max_tile_radius;
    uint far_tile_radius;
    float tile_height_step;
    float time;
    vec2 wind_dir;
    float wind_speed;
    float wave_amplitude;
    float gerstner_amplitude;
    float fresnel_bias;
    float fresnel_strength;
    float foam_strength;
    float foam_threshold;
    float foam_advection_strength;
    float foam_decay_rate;
    float foam_noise_scale;
    vec2 current;
    vec3 _padding1;
    vec4 absorption_coeff;
    vec4 shallow_color;
    vec4 deep_color;
    vec4 scattering_color;
    float scattering_strength;
    float turbidity_depth;
    float refraction_strength;
    float ssr_strength;
    float ssr_max_distance;
    float ssr_thickness;
    uint ssr_steps;
    float _padding2;
} params;

layout(set = 1, binding = 1) readonly buffer SceneCameras {
  Camera cameras[];
} meshi_bindless_cameras;

layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec3 v_normal;
layout(location = 2) out vec3 v_view_dir;
layout(location = 3) out vec3 v_world_pos;
layout(location = 4) out float v_velocity;
layout(location = 5) out vec2 v_flow;

vec2 camera_position() {
  Camera c = meshi_bindless_cameras.cameras[params.camera_index];
    return camera_position(c).xy;
}

vec3 camera_position_world() {
    Camera c = meshi_bindless_cameras.cameras[params.camera_index];
    return camera_position(c);
}

float camera_far_plane() {
    return meshi_bindless_cameras.cameras[params.camera_index].far;
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

vec4 sample_waves(vec2 uv, uint cascade_index, uint fft_size) {
    vec2 wrapped_uv = fract(uv);
    float fft_size_f = float(fft_size);
    float fx = wrapped_uv.x * fft_size_f;
    float fy = wrapped_uv.y * fft_size_f;
    uint x0 = uint(floor(fx));
    uint y0 = uint(floor(fy));
    uint x1 = (x0 + 1) % fft_size;
    uint y1 = (y0 + 1) % fft_size;
    float tx = fx - float(x0);
    float ty = fy - float(y0);
    uint buffer_index = nonuniformEXT(cascade_index);
    uint max_index = max(ocean_waves[buffer_index].values.length(), 1);

    uint idx00 = min(y0 * fft_size + x0, max_index - 1);
    uint idx10 = min(y0 * fft_size + x1, max_index - 1);
    uint idx01 = min(y1 * fft_size + x0, max_index - 1);
    uint idx11 = min(y1 * fft_size + x1, max_index - 1);

    vec4 w00 = ocean_waves[buffer_index].values[idx00];
    vec4 w10 = ocean_waves[buffer_index].values[idx10];
    vec4 w01 = ocean_waves[buffer_index].values[idx01];
    vec4 w11 = ocean_waves[buffer_index].values[idx11];
    vec4 wx0 = mix(w00, w10, tx);
    vec4 wx1 = mix(w01, w11, tx);
    return mix(wx0, wx1, ty);
}

void main() {
    uint grid_resolution = max(params.vertex_resolution, 2);
    uint quad_index = gl_VertexIndex / 6;
    uint local_vertex = gl_VertexIndex % 6;
    uint quad_x = quad_index % (grid_resolution - 1);
    uint quad_y = quad_index / (grid_resolution - 1);
    float grid_scale = 1.0 / float(grid_resolution - 1);
    vec2 base_quad_origin = vec2(quad_x, quad_y) * grid_scale;
    vec2 base_quad_size = vec2(grid_scale);
    uint base_radius = max(params.base_tile_radius, 1);
    uint max_radius = max(params.max_tile_radius, base_radius);
    uint far_radius_cap = max(params.far_tile_radius, base_radius);
    float base_patch_size = max(params.cascade_patch_sizes.y, 0.001);
    float tile_size = max(base_patch_size * 2.0, 0.001);
    float height_step = max(params.tile_height_step, 0.001);
    float camera_height = abs(camera_position_world().y);
    float extra_radius_f = floor(camera_height / height_step);
    float max_extra = float(max_radius - base_radius);
    uint extra_radius = uint(clamp(extra_radius_f, 0.0, max_extra));
    uint height_radius = base_radius + extra_radius;
    float far_plane = max(camera_far_plane(), 0.0);
    float far_radius_f = ceil(far_plane / tile_size);
    uint far_radius = uint(clamp(far_radius_f, float(base_radius), float(far_radius_cap)));
    uint tile_radius = min(max(height_radius, far_radius), max_radius);
    uint tile_count = tile_radius * 2 + 1;
    if (gl_InstanceIndex >= tile_count * tile_count) {
        gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
        v_uv = vec2(0.0);
        v_normal = vec3(0.0, 1.0, 0.0);
        v_view_dir = vec3(0.0);
        v_world_pos = vec3(0.0);
        v_velocity = 0.0;
        return;
    }

    uint tile_x = gl_InstanceIndex % tile_count;
    uint tile_y = gl_InstanceIndex / tile_count;
    vec2 tile_grid = vec2(tile_count);
    vec2 tile_center = (tile_grid - 1.0) * 0.5;
    ivec2 tile_coord = ivec2(tile_x, tile_y) - ivec2(tile_center);
    uint ring = uint(max(abs(tile_coord.x), abs(tile_coord.y)));
    float ring_f = float(max(ring, 1u));
    float base_radius_f = float(base_radius);
    uint clip_level = 0u;
    if (ring > base_radius) {
        clip_level = uint(floor(log2(ring_f / base_radius_f)) + 1.0);
    }
    uint lod_step = min(1u << clip_level, grid_resolution - 1);
    uint lod_step_next = min(lod_step * 2u, grid_resolution - 1);
    uint clip_outer = max(base_radius * (1u << clip_level), 1u);
    float morph_band = max(base_radius_f * 0.25, 1.0);
    float morph = smoothstep(float(clip_outer) - morph_band, float(clip_outer), ring_f);
    vec2 tile_offset = (vec2(tile_coord)) * tile_size;
    vec2 snapped_origin = floor(camera_position() / tile_size) * tile_size;
    vec2 quad_center_uv = base_quad_origin + base_quad_size * 0.5;
    vec2 quad_center_local = (quad_center_uv * 2.0 - 1.0) * base_patch_size;
    vec2 quad_center_world = quad_center_local + snapped_origin + tile_offset;
    vec3 camera_world = camera_position_world();
    float distance = length(quad_center_world - camera_world.xz);
    float near_range = params.cascade_blend_ranges.x;
    float mid_range = params.cascade_blend_ranges.y;
    float far_range = params.cascade_blend_ranges.z;
    vec2 vertex_index = vec2(quad_x, quad_y) + vertex_uv(local_vertex);
    vec2 snapped_current =
        floor(vertex_index / float(lod_step) + 0.5) * float(lod_step) * grid_scale;
    vec2 snapped_next =
        floor(vertex_index / float(lod_step_next) + 0.5) * float(lod_step_next) * grid_scale;
    vec2 uv = mix(snapped_current, snapped_next, morph);
    vec2 local = (uv * 2.0 - 1.0) * base_patch_size;
    vec2 world = local + snapped_origin + tile_offset;
    vec2 wave_world = world + params.current * params.time;
    float w_near = 1.0 - smoothstep(near_range * 0.6, near_range, distance);
    float w_far = smoothstep(mid_range, far_range, distance);
    float w_mid = clamp(1.0 - w_near - w_far, 0.0, 1.0);
    float weight_sum = max(w_near + w_mid + w_far, 0.001);
    w_near /= weight_sum;
    w_mid /= weight_sum;
    w_far /= weight_sum;

    uint fft_near = max(params.cascade_fft_sizes.x, 1);
    uint fft_mid = max(params.cascade_fft_sizes.y, 1);
    uint fft_far = max(params.cascade_fft_sizes.z, 1);
    float patch_near = max(params.cascade_patch_sizes.x, 0.001);
    float patch_mid = max(params.cascade_patch_sizes.y, 0.001);
    float patch_far = max(params.cascade_patch_sizes.z, 0.001);
    vec4 waves_near = sample_waves(wave_world / (patch_near * 2.0) + vec2(0.5), 0u, fft_near);
    vec4 waves_mid = sample_waves(wave_world / (patch_mid * 2.0) + vec2(0.5), 1u, fft_mid);
    vec4 waves_far = sample_waves(wave_world / (patch_far * 2.0) + vec2(0.5), 2u, fft_far);
    float height = waves_near.x * w_near + waves_mid.x * w_mid + waves_far.x * w_far;
    vec2 gradient_world =
        waves_near.yz * (w_near / (patch_near)) +
        waves_mid.yz * (w_mid / (patch_mid)) +
        waves_far.yz * (w_far / (patch_far));
    float velocity = waves_near.w * w_near + waves_mid.w * w_mid + waves_far.w * w_far;
    vec2 wind_dir = safe_normalize(params.wind_dir);
    vec2 choppy_offset = -gradient_world * (base_patch_size * 0.15);
    vec4 position = vec4(world.x + choppy_offset.x, height * 100.0, world.y + choppy_offset.y, 1.0);
    vec3 normal = normalize(vec3(-gradient_world.x, 1.0, -gradient_world.y));
    mat4 view = inverse(camera_view());
    mat4 proj = camera_proj();
    gl_Position = proj * view * position;
    gl_Position.y = -gl_Position.y;
    v_uv = world / tile_size + vec2(0.5);
    v_normal = normal;
    v_view_dir = camera_position_world() - position.xyz;
    v_world_pos = position.xyz;
    v_velocity = velocity;
    v_flow = params.current + wind_dir * params.wind_speed * 0.08 - gradient_world * 0.5;
}
