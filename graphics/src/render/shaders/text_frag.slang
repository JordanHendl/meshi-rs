uniform Texture2D meshi_bindless_textures[] : register(t0, space0);
uniform SamplerState meshi_bindless_samplers[] : register(s1, space0);

struct FSInput {
    float4 position : SV_POSITION;
    [[vk::location(0)]] float2 tex_coords : TEXCOORD0;
    [[vk::location(1)]] float4 color : COLOR0;
    [[vk::location(2)]] nointerpolation uint texture_id : TEXCOORD1;
};

[shader("fragment")]
float4 main(FSInput input) : SV_Target {
    const float4 sample = meshi_bindless_textures[input.texture_id]
        .Sample(meshi_bindless_samplers[input.texture_id], input.tex_coords);
    const float distance = sample.r;
    const float width = fwidth(distance);
    const float alpha = smoothstep(0.5 - width, 0.5 + width, distance);
    return float4(input.color.rgb, input.color.a * alpha);
}
