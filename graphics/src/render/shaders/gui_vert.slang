struct GuiVertex {
    float2 position;
    float2 uv;
    float4 color;
};

StructuredBuffer<GuiVertex> gui_vertices : register(t0, space1);
StructuredBuffer<uint> gui_indices : register(t1, space1);

struct VSOutput {
    float4 position : SV_POSITION;
    [[vk::location(0)]] float2 tex_coords : TEXCOORD0;
    [[vk::location(1)]] float4 color : COLOR0;
};

[shader("vertex")]
VSOutput main(uint vertex_id : SV_VertexID) {
    VSOutput output;
    const uint index = gui_indices[vertex_id];
    const GuiVertex vertex = gui_vertices[index];
    output.position = float4(vertex.position, 0.0, 1.0);
    output.tex_coords = vertex.uv;
    output.color = vertex.color;
    return output;
}
