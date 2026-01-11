struct TextGlyph {
    float2 origin;
    float2 size;
    float2 uv_min;
    float2 uv_max;
    float4 color;
    uint texture_id;
    uint _padding0;
    uint _padding1;
    uint _padding2;
};

StructuredBuffer<TextGlyph> text_glyph_ssbo : register(t0, space1);

struct VSOutput {
    float4 position : SV_POSITION;
    [[vk::location(0)]] float2 tex_coords : TEXCOORD0;
    [[vk::location(1)]] float4 color : COLOR0;
    [[vk::location(2)]] nointerpolation uint texture_id : TEXCOORD1;
};

[shader("vertex")]
VSOutput main(uint vertex_id : SV_VertexID, uint instance_id : SV_InstanceID) {
    VSOutput output;
    const TextGlyph glyph = text_glyph_ssbo[instance_id];

    const float2 corners[4] = {
        float2(0.0, 0.0),
        float2(1.0, 0.0),
        float2(1.0, 1.0),
        float2(0.0, 1.0),
    };

    const uint indices[6] = {0, 1, 2, 2, 3, 0};
    const float2 corner = corners[indices[vertex_id]];

    const float2 position = glyph.origin + glyph.size * corner;
    const float2 uv = lerp(glyph.uv_min, glyph.uv_max, corner);

    output.position = float4(position, 0.0, 1.0);
    output.tex_coords = uv;
    output.color = glyph.color;
    output.texture_id = glyph.texture_id;
    return output;
}
