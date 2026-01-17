#version 450

layout(local_size_x = 1, local_size_y = 1, local_size_z = 1) in;

struct Particle {
    vec3 position;
    float lifetime;
    vec3 velocity;
    float size;
    vec4 color;
};

struct DrawIndexedIndirectCommand {
    uint index_count;
    uint instance_count;
    uint first_index;
    int vertex_offset;
    uint first_instance;
};

layout(set = 0, binding = 0) readonly buffer Particles {
    Particle particles[];
} particle_data;

layout(set = 0, binding = 1) buffer DrawList {
    DrawIndexedIndirectCommand commands[];
} draw_list;

layout(set = 1, binding = 0) readonly buffer DrawParams {
    uint num_particles;
    uint index_count;
    uint first_index;
    int vertex_offset;
    uint first_instance;
    uint _padding0;
    uint _padding1;
    uint _padding2;
} params;

void main() {
    if (gl_GlobalInvocationID.x != 0) {
        return;
    }

    DrawIndexedIndirectCommand cmd;
    cmd.index_count = params.index_count;
    cmd.instance_count = params.num_particles;
    cmd.first_index = params.first_index;
    cmd.vertex_offset = params.vertex_offset;
    cmd.first_instance = params.first_instance;
    draw_list.commands[0] = cmd;
}
