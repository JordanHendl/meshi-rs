// Skybox Vertex Shader (skybox_vert.slang)

struct VSOutput
{
    float4 position : SV_POSITION;
    [[vk::location(0)]] float2 uv : TEXCOORD0;
};

[shader("vertex")]
VSOutput main(uint vertex_id : SV_VertexID)
{
    VSOutput output;
    float2 positions[3] = {
        float2(-1.0, -1.0),
        float2(3.0, -1.0),
        float2(-1.0, 3.0)
    };

    float2 pos = positions[vertex_id];
    output.position = float4(pos, 1.0, 1.0);
    output.uv = pos * 0.5 + 0.5;

    output.position.y = -output.position.y;
    return output;
}
