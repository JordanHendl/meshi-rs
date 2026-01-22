uniform Texture2D meshi_bindless_textures[] : register(t0, space0);
uniform SamplerState meshi_bindless_samplers[] : register(s1, space0);

struct PSInput {
    float4 position : SV_POSITION;
    [[vk::location(0)]] float2 tex_coords : TEXCOORD0;
    [[vk::location(1)]] float4 color : COLOR0;
    [[vk::location(2)]] nointerpolation uint texture_id : TEXCOORD1;
};

[shader("fragment")]
float4 main(PSInput input) : SV_Target0 {
    if (input.texture_id == 0xFFFFFFFF) {
        return input.color;
    }

    const float4 sample = meshi_bindless_textures[input.texture_id]
        .Sample(meshi_bindless_samplers[input.texture_id], input.tex_coords);
    return sample * input.color;
}
