struct PSInput {
    float4 position : SV_POSITION;
    [[vk::location(0)]] float2 tex_coords : TEXCOORD0;
    [[vk::location(1)]] float4 color : COLOR0;
};

[shader("fragment")]
float4 main(PSInput input) : SV_Target0 {
    return input.color;
}
