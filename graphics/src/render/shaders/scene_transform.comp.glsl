#version 450

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

struct SceneObject {
    mat4 local_transform;
    mat4 world_transform;
    uint scene_mask;
    uint parent_slot;
    uint dirty;
    uint active;
    uint parent;
    uint child_count;
    uint children[16];
};

layout(set = 0, binding = 0) buffer SceneObjects {
    SceneObject objects[];
};

void main() {
    uint idx = gl_GlobalInvocationID.x;
    if (idx >= objects.length()) {
        return;
    }

    SceneObject obj = objects[idx];
    if (obj.active == 0) {
        return;
    }

    mat4 world = obj.local_transform;
    uint parent_slot = obj.parent_slot;

    while (parent_slot != 0xffffffffu) {
        SceneObject parent = objects[parent_slot];
        world = parent.local_transform * world;
        parent_slot = parent.parent_slot;
    }

    objects[idx].world_transform = world;
    objects[idx].dirty = 0;
}
