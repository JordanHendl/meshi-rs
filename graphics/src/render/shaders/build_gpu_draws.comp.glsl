#version 450
layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

struct Handle {
    uint val;
};

struct PerDrawData {
    Handle scene_id;
    Handle transform_id;
    Handle material_id;
    Handle skeleton_id;
    Handle animation_state_id;
    Handle per_obj_joints_id;
    uint vertex_id;
    uint vertex_count;
    uint index_id;
    uint index_count;
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
    int vertex_offset;
    uint first_instance;
};

layout(set = 0, binding = 0) readonly buffer DrawObjects {
    PerDrawData objects[];
} draws;

layout(set = 0, binding = 1) readonly buffer CulledBins {
    CulledObject objects[];
} culled;

layout(set = 0, binding = 2) readonly buffer BinCounts {
    uint counts[];
} counts;

layout(set = 0, binding = 3) buffer DrawList {
    DrawIndexedIndirectCommand commands[];
} draw_list;

layout(set = 1, binding = 0) readonly buffer DrawParams {
    uint bin;
    uint view;
    uint num_bins;
    uint max_objects;
    uint num_draws;
    uint _padding0;
    uint _padding1;
    uint _padding2;
} params;

uint handle_slot(Handle handle_value) {
    return handle_value.val & 0xFFFFu;
}

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (idx >= params.num_draws) {
        return;
    }

    PerDrawData draw = draws.objects[idx];
    DrawIndexedIndirectCommand cmd;
    cmd.index_count = draw.index_count;
    cmd.instance_count = 0u;
    cmd.first_index = draw.index_id;
    cmd.vertex_offset = 0;
    cmd.first_instance = idx;

    if (params.num_bins == 0u) {
        draw_list.commands[idx] = cmd;
        return;
    }

    if (params.bin >= params.num_bins) {
        draw_list.commands[idx] = cmd;
        return;
    }

    uint bin_offset = params.view * params.num_bins + params.bin;
    uint bin_count = counts.counts[bin_offset];
    uint scene_slot = handle_slot(draw.scene_id);

    bool visible = false;
    uint capped_count = min(bin_count, params.max_objects);
    for (uint i = 0u; i < capped_count; ++i) {
        uint cull_index = bin_offset * params.max_objects + i;
        if (culled.objects[cull_index].object_id == scene_slot) {
            visible = true;
            break;
        }
    }

    if (visible) {
        cmd.instance_count = 1u;
    }

    cmd.instance_count = 1u;
    draw_list.commands[idx] = cmd;
}
