//scene_cull.comp.glsl
#version 450

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

struct Handle {
  uint val;
};

struct PerDrawData {
    scene_id: Handle,
    transform_id: Handle,
    material_id: Handle,
    skeleton_id: Handle,
    animation_state_id: Handle,
    per_obj_joints_id: Handle,
    vertex_id: Handle,
    vertex_count: uint,
    index_id: Handle,
    index_count: uint,
}

struct SceneBin {
    uint id;
    uint mask;
};

struct CulledObject {
    mat4 total_transform;
    uint transformation;
    uint object_id;
    uint bin_id;
};

struct DrawIndexedIndirectCommand {
    uint index_count;
    uint instance_count;
    uint first_index;
    int  vertex_offset;
    uint first_instance;
};

////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////

layout(set = 0, binding = 0) buffer DrawObjects {
    PerDrawData objects[];
} draws;

layout(set = 0, binding = 1) buffer SceneBins {
    SceneBin bins[];
} bins;

layout(set = 0, binding = 2) buffer CulledBins {
    CulledObject culled[];
} scene_objects;

layout(set = 0, binding = 3) buffer BinCounts {
    uint counts[];
} counts;

layout(set = 0, binding = 4) uniform SceneParams {
    uint num_bins;
    uint max_objects;
    uint num_views;
} params;

layout(set = 1, binding = 0) buffer Cameras {
    Handle cameras[];
} cameras;


void main() {
    uint idx = gl_GlobalInvocationID.x;

    // This dispatch is to build draws for each object in each bin for each view frustum
    uint max_num_draws = params.max_object * params.num_bins * params.num_views;
    if (idx >= max_num_draws) {
        return;
    }
}
