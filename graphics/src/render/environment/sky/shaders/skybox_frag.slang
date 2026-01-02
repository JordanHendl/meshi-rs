// Skybox Fragment Shader (skybox_frag.slang)

struct Camera
{
    column_major float4x4 world_from_camera;
    column_major float4x4 projection;
    float2 viewport;
    float near;
    float far;
    float fov_y_radians;
    uint projection_kind;
    float _padding;
};

struct FSInput
{
    [[vk::location(0)]] float2 uv : TEXCOORD0;
};

struct FSOutput
{
    float4 color : SV_TARGET0;
};

StructuredBuffer<Camera> meshi_bindless_cameras : register(t2, space0);

TextureCube skybox_texture : register(t0, space1);
SamplerState skybox_sampler : register(s1, space1);

struct SkyboxParams
{
    uint camera_index;
    float intensity;
    float2 _padding;
};

StructuredBuffer<SkyboxParams> skybox_params : register(t2, space1);
static SkyboxParams skybox = skybox_params[0];

[shader("fragment")]
FSOutput main(FSInput input)
{
    FSOutput output;
    Camera camera = meshi_bindless_cameras[skybox.camera_index];

    float2 ndc = input.uv * 2.0 - 1.0;
    float aspect = camera.viewport.x / camera.viewport.y;
    float tan_half = tan(camera.fov_y_radians * 0.5);
    float3 dir = normalize(float3(ndc.x * aspect * tan_half, -ndc.y * tan_half, -1.0));
    float3 world_dir = normalize(mul((float3x3)camera.world_from_camera, dir));

    float4 color = skybox_texture.Sample(skybox_sampler, world_dir);
    output.color = color * skybox.intensity;
    return output;
}
