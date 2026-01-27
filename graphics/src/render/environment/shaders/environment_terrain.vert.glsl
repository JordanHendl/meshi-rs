#version 450

struct TerrainInstance {
    vec3 position;
    uint lod;
};

layout(set = 0, binding = 0) readonly buffer InstanceBuffer {
    TerrainInstance instances[];
} instance_data;

layout(set = 1, binding = 0) readonly buffer TerrainParams {
    vec3 camera_position;
    uint lod_levels;
    float patch_size;
    uint max_tiles;
    uint clipmap_resolution;
    float height_scale;
    vec2 _padding;
} params;

layout(set = 2, binding = 0) readonly buffer HeightmapBuffer {
    float heights[];
} heightmap;

layout(location = 0) out vec3 v_color;

vec2 quad_vertex(uint vertex_id) {
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
    TerrainInstance instance = instance_data.instances[gl_InstanceIndex];
    vec2 uv = quad_vertex(gl_VertexIndex);
    uint resolution = max(params.clipmap_resolution, 1u);
    uint tile_x = gl_InstanceIndex % resolution;
    uint tile_y = gl_InstanceIndex / resolution;
    uint height_index = tile_x + tile_y * resolution;
    float height = heightmap.heights[height_index] * params.height_scale;
    vec2 local = (uv * 2.0 - 1.0) * params.patch_size;
    vec3 world = instance.position + vec3(local.x, height, local.y);

    gl_Position = vec4(world.x, world.y, world.z, 1.0);
    v_color = vec3(0.2 + 0.1 * float(instance.lod), 0.5, 0.2);
}
