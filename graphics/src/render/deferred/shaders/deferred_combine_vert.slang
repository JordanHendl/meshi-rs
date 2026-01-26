struct VSOutput {
    float4 position : SV_POSITION;
    float2 tex_coords : TEXCOORD0;
};

[shader("vertex")]
VSOutput main(uint vertex_id : SV_VertexID) {
    VSOutput output;
    float2 positions[3] = {
        float2(-1.0, -1.0),
        float2(3.0, -1.0),
        float2(-1.0, 3.0)
    };
    float2 uvs[3] = {
        float2(0.0, 0.0),
        float2(2.0, 0.0),
        float2(0.0, 2.0)
    };

    output.position = float4(positions[vertex_id], 0.0, 1.0);
    output.tex_coords = uvs[vertex_id];
    return output;
}
