#version 450

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

struct CulledObject {
    mat4 total_transform;
    uint transformation;
    uint object_id;
    uint bin_id;
};

struct SceneDrawRange {
    uint indexed_offset;
    uint indexed_count;
    uint non_indexed_offset;
    uint non_indexed_count;
};

struct SceneDrawMetadata {
    uint mesh_id;
    uint material_id;
    uint per_obj_joints_id;
};

struct SceneDrawListEntry {
    uint draw_index;
    uint mesh_id;
    uint material_id;
    uint per_obj_joints_id;
    uint object_id;
    uint draw_type;
};

struct SceneObjectSkinningInfo {
    uint skeleton_id;
    uint animation_state_id;
};

struct PerInstanceData {
    mat4 transform;
    uint transformation;
    uint material_id;
    uint camera;
    uint skeleton_id;
    uint animation_state_id;
    uint per_obj_joints_id;
    uint padding;
};

struct IndexedIndirectCommand {
    uint index_count;
    uint instance_count;
    uint first_index;
    int vertex_offset;
    uint first_instance;
};

struct IndirectCommand {
    uint vertex_count;
    uint instance_count;
    int first_vertex;
    uint first_instance;
};

layout(set = 0, binding = 0) readonly buffer CulledBins {
    CulledObject culled[];
} culled;

layout(set = 0, binding = 1) readonly buffer BinCounts {
    uint counts[];
} counts;

layout(set = 0, binding = 2) readonly buffer DrawRanges {
    SceneDrawRange ranges[];
} draw_ranges;

layout(set = 0, binding = 3) readonly buffer IndexedTemplates {
    IndexedIndirectCommand cmds[];
} indexed_templates;

layout(set = 0, binding = 4) readonly buffer DrawTemplates {
    IndirectCommand cmds[];
} draw_templates;

layout(set = 0, binding = 5) readonly buffer IndexedMetadata {
    SceneDrawMetadata entries[];
} indexed_metadata;

layout(set = 0, binding = 6) readonly buffer DrawMetadata {
    SceneDrawMetadata entries[];
} draw_metadata;

layout(set = 1, binding = 0) buffer IndexedArgs {
    IndexedIndirectCommand args[];
} indexed_args;

layout(set = 1, binding = 1) buffer DrawArgs {
    IndirectCommand args[];
} draw_args;

layout(set = 1, binding = 2) buffer DrawList {
    SceneDrawListEntry entries[];
} draw_list;

layout(set = 1, binding = 3) buffer DrawListCounts {
    uint counts[];
} draw_list_counts;

layout(set = 1, binding = 4) readonly buffer ObjectSkinning {
    SceneObjectSkinningInfo entries[];
} object_skinning;

layout(set = 1, binding = 5) buffer InstanceData {
    PerInstanceData entries[];
} instance_data;

layout(set = 1, binding = 6) buffer ActiveViews {
    uint count;
    uint padding0;
    uint padding1;
    uint padding2;
    uint slots[8];
} active_views;

layout(set = 1, binding = 7) uniform DrawParams {
    uint num_bins;
    uint max_objects;
    uint num_views;
    uint indexed_draws_per_view;
    uint non_indexed_draws_per_view;
    uint draw_list_capacity;
    uint mode;
} params;

void clear_outputs(uint idx) {
    if (idx < params.indexed_draws_per_view * params.num_views) {
        indexed_args.args[idx].instance_count = 0u;
    }
    if (idx < params.non_indexed_draws_per_view * params.num_views) {
        draw_args.args[idx].instance_count = 0u;
    }
    if (idx < params.num_views) {
        draw_list_counts.counts[idx] = 0u;
    }
}

void build_draws(uint idx) {
    uint total_bins = params.num_bins * params.num_views;
    uint total_slots = total_bins * params.max_objects;
    if (idx >= total_slots) {
        return;
    }

    uint view_bin = idx / params.max_objects;
    uint slot = idx % params.max_objects;
    if (slot >= counts.counts[view_bin]) {
        return;
    }

    uint view = view_bin / params.num_bins;
    if (view >= active_views.count) {
        return;
    }
    uint instance_stride = params.indexed_draws_per_view + params.non_indexed_draws_per_view;
    CulledObject culled = culled.culled[idx];
    SceneDrawRange range = draw_ranges.ranges[culled.object_id];
    SceneObjectSkinningInfo skinning = object_skinning.entries[culled.object_id];
    uint camera_slot = active_views.slots[view];

    uint indexed_base = view * params.indexed_draws_per_view;
    for (uint i = 0u; i < range.indexed_count; ++i) {
        uint draw_index = range.indexed_offset + i;
        uint output_index = indexed_base + draw_index;
        IndexedIndirectCommand template_cmd = indexed_templates.cmds[draw_index];
        template_cmd.instance_count = 1u;
        template_cmd.first_instance = view * instance_stride + draw_index;
        indexed_args.args[output_index] = template_cmd;

        uint instance_index = template_cmd.first_instance;
        SceneDrawMetadata meta = indexed_metadata.entries[draw_index];
        instance_data.entries[instance_index].transform = culled.total_transform;
        instance_data.entries[instance_index].transformation = culled.transformation;
        instance_data.entries[instance_index].material_id = meta.material_id;
        instance_data.entries[instance_index].camera = camera_slot;
        instance_data.entries[instance_index].skeleton_id = skinning.skeleton_id;
        instance_data.entries[instance_index].animation_state_id =
            skinning.animation_state_id;
        instance_data.entries[instance_index].per_obj_joints_id = meta.per_obj_joints_id;
        uint list_index = atomicAdd(draw_list_counts.counts[view], 1u);
        if (list_index < params.draw_list_capacity) {
            uint list_offset = view * params.draw_list_capacity + list_index;
            draw_list.entries[list_offset].draw_index = draw_index;
            draw_list.entries[list_offset].mesh_id = meta.mesh_id;
            draw_list.entries[list_offset].material_id = meta.material_id;
            draw_list.entries[list_offset].per_obj_joints_id = meta.per_obj_joints_id;
            draw_list.entries[list_offset].object_id = culled.object_id;
            draw_list.entries[list_offset].draw_type = 0u;
        }
    }

    uint draw_base = view * params.non_indexed_draws_per_view;
    for (uint i = 0u; i < range.non_indexed_count; ++i) {
        uint draw_index = range.non_indexed_offset + i;
        uint output_index = draw_base + draw_index;
        IndirectCommand template_cmd = draw_templates.cmds[draw_index];
        template_cmd.instance_count = 1u;
        template_cmd.first_instance =
            view * instance_stride + params.indexed_draws_per_view + draw_index;
        draw_args.args[output_index] = template_cmd;

        uint instance_index = template_cmd.first_instance;
        SceneDrawMetadata meta = draw_metadata.entries[draw_index];
        instance_data.entries[instance_index].transform = culled.total_transform;
        instance_data.entries[instance_index].transformation = culled.transformation;
        instance_data.entries[instance_index].material_id = meta.material_id;
        instance_data.entries[instance_index].camera = camera_slot;
        instance_data.entries[instance_index].skeleton_id = skinning.skeleton_id;
        instance_data.entries[instance_index].animation_state_id =
            skinning.animation_state_id;
        instance_data.entries[instance_index].per_obj_joints_id = meta.per_obj_joints_id;
        uint list_index = atomicAdd(draw_list_counts.counts[view], 1u);
        if (list_index < params.draw_list_capacity) {
            uint list_offset = view * params.draw_list_capacity + list_index;
            draw_list.entries[list_offset].draw_index = draw_index;
            draw_list.entries[list_offset].mesh_id = meta.mesh_id;
            draw_list.entries[list_offset].material_id = meta.material_id;
            draw_list.entries[list_offset].per_obj_joints_id = meta.per_obj_joints_id;
            draw_list.entries[list_offset].object_id = culled.object_id;
            draw_list.entries[list_offset].draw_type = 1u;
        }
    }
}

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (params.mode == 0u) {
        clear_outputs(idx);
        return;
    }

    build_draws(idx);
}
