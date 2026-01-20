//scene_transform.comp.glsl
#version 450

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

struct SceneObject {
    mat4 local_transform;
    mat4 world_transform;
    uint scene_mask;
    uint scene_type;
    uint transformation;
    uint parent_slot;
    uint dirty;
    uint is_active;
    uint parent;
    uint child_count;
    uint children[16];
};

layout(set = 0, binding = 0) buffer SceneObjects {
    SceneObject objects[];
} in_list;

layout(set = 3, binding = 0) buffer Transformations {
    mat4 transforms[];
} transformations;

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (idx >= in_list.objects.length()) {
        return;
    }

    SceneObject obj = in_list.objects[idx];
    if (obj.is_active == 0) {
        return;
    }

    mat4 world = obj.local_transform;
    uint parent_slot = obj.parent_slot;

    while (parent_slot < in_list.objects.length()) {
        SceneObject parent = in_list.objects[parent_slot];
        world = parent.local_transform * world;
        parent_slot = parent.parent_slot;
    }

    in_list.objects[idx].world_transform = world;
    in_list.objects[idx].dirty = 0;

    uint transform_slot = in_list.objects[idx].transformation & 0xFFFFu;
    transformations.transforms[transform_slot] = world;
}
