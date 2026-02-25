struct GuiVertex {
    float2 position;
    float2 uv;
    float4 color;
    uint texture_id;
    uint _padding0;
    uint _padding1;
    uint _padding2;
};

StructuredBuffer<GuiVertex> gui_vertices : register(t0, space1);
StructuredBuffer<uint> gui_indices : register(t1, space1);
struct GuiDrawParams {
    uint index_offset;
    uint _padding0;
    uint _padding1;
    uint _padding2;
};
StructuredBuffer<GuiDrawParams> gui_draw_ssbo : register(t2, space1);
static GuiDrawParams gui_draw = gui_draw_ssbo[0];

struct VSOutput {
    float4 position : SV_POSITION;
    [[vk::location(0)]] float2 tex_coords : TEXCOORD0;
    [[vk::location(1)]] float4 color : COLOR0;
    [[vk::location(2)]] nointerpolation uint texture_id : TEXCOORD1;
};

[shader("vertex")]
VSOutput main(uint vertex_id : SV_VertexID) {
    VSOutput output;
    const uint index = gui_indices[vertex_id + gui_draw.index_offset];
    const GuiVertex vertex = gui_vertices[index];
    float2 position = vertex.position;
    position.y = -position.y;
    output.position = float4(position, 0.0, 1.0);
    output.tex_coords = vertex.uv;
    output.color = vertex.color;
    output.texture_id = vertex.texture_id;
    return output;
}
