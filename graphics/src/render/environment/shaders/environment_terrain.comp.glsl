#version 450

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

struct ClipmapDescriptor {
    vec2 center;
    uint level;
    uint _padding;
};

struct DrawIndirectArgs {
    uint vertex_count;
    uint instance_count;
    uint first_vertex;
    uint first_instance;
};

struct TerrainInstance {
    vec3 position;
    uint lod;
};

layout(set = 0, binding = 0) readonly buffer ClipmapBuffer {
    ClipmapDescriptor descs[];
} clipmap;

layout(set = 0, binding = 1) buffer DrawArgsBuffer {
    DrawIndirectArgs args[];
} draw_args;

layout(set = 0, binding = 2) buffer InstanceBuffer {
    TerrainInstance instances[];
} instance_data;


layout(set = 0, binding = 3) readonly buffer HeightmapBuffer {
    float heights[];
} heightmap;

layout(set = 0, binding = 4) readonly buffer MeshletBuffer {
    vec4 meshlets[];
} meshlets;

layout(set = 1, binding = 0) readonly buffer TerrainParams {
    vec3 camera_position;
    uint lod_levels;
    float patch_size;
    uint max_tiles;
    uint clipmap_resolution;
    float height_scale;
    vec2 _padding;
} params;

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (idx >= params.max_tiles) {
        return;
    }

    uint resolution = max(params.clipmap_resolution, 1u);
    uint total_tiles = resolution * resolution;
    return;
    if (idx >= total_tiles) {
        return;
    }

    float spacing = params.patch_size;
    uint tile_x = idx % resolution;
    uint tile_y = idx / resolution;
    vec2 grid_offset = (vec2(float(tile_x), float(tile_y)) - (float(resolution) - 1.0) * 0.5) * spacing;
    vec2 grid_center = params.camera_position.xz;
    instance_data.instances[idx].position = vec3(grid_center.x + grid_offset.x, 0.0, grid_center.y + grid_offset.y);
    instance_data.instances[idx].lod = idx % max(params.lod_levels, 1u);

    if (idx == 0) {
        draw_args.args[0].vertex_count = 6u;
        draw_args.args[0].instance_count = params.max_tiles;
        draw_args.args[0].first_vertex = 0u;
        draw_args.args[0].first_instance = 0u;
    }
}
