#version 450
#extension GL_EXT_samplerless_texture_functions : enable
#extension GL_EXT_nonuniform_qualifier : enable

layout(local_size_x = 32, local_size_y = 32, local_size_z = 1) in;

layout(set = 0, binding = 0) uniform DebugLayerParams {
    uvec2 output_resolution;
    uint input_texture;
    uint debug_selection;
    uvec4 debug_textures;
    uint override_texture;
    uvec3 _padding;
} debug_params;

layout(set = 0, binding = 1) uniform writeonly image2D debug_output;
layout(set = 1, binding = 0) uniform texture2D meshi_bindless_textures[];
layout(set = 1, binding = 1) uniform sampler meshi_bindless_samplers[];


vec4 sample_bindless(uint texture_id, vec2 uv) {
    if (texture_id == 0xFFFFFFFFu) {
        return vec4(0.0);
    }
    return texture(sampler2D(meshi_bindless_textures[texture_id], meshi_bindless_samplers[texture_id]), uv);
}

void main() {
    uvec2 gid = gl_GlobalInvocationID.xy;
    if (gid.x >= debug_params.output_resolution.x || gid.y >= debug_params.output_resolution.y) {
        return;
    }

    vec2 uv = (vec2(gid) + 0.5) / vec2(debug_params.output_resolution);
    uint selected = debug_params.debug_selection;
    uint texture_id = debug_params.input_texture;

    if (selected > 0u) {
        uint debug_index = selected - 1u;
        if (debug_index < 4u) {
            texture_id = debug_params.debug_textures[debug_index];
        }
    }

    if (debug_params.override_texture != 0xFFFFFFFFu) {
        texture_id = debug_params.override_texture;
    }

    vec4 color = sample_bindless(texture_id, uv);
    imageStore(debug_output, ivec2(gid), color);
}
