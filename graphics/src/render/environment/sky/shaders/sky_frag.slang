// Procedural Sky Fragment Shader (sky_frag.slang)

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

struct SkyDrawParams
{
    uint camera_index;
    uint _padding0;
    uint _padding1;
    uint _padding2;
};

struct SkyConfig
{
    float3 horizon_init;
    float intensity_scale;
    float3 zenith_tint;
    float _padding0;
    float3 sun_dir;
    float sun_intensity;
    float3 sun_color;
    float sun_angular_radius;
    float3 moon_dir;
    float moon_intensity;
    float3 moon_color;
    float moon_angular_radius;
    float4 fog_color_density[2];
    float4 fog_height_falloff[2];
    float4 fog_noise_speed[2];
    float fog_time;
    float3 _padding1;
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
StructuredBuffer<SkyDrawParams> sky_draw_ssbo : register(t0, space1);
StructuredBuffer<SkyConfig> SkyParams : register(t1, space1);

static SkyDrawParams sky_draw = sky_draw_ssbo[0];
static SkyConfig sky = SkyParams[0];

float3 sky_gradient(float3 view_dir)
{
    float horizon_weight = saturate(view_dir.y * 0.5 + 0.5);
    float zenith_weight = pow(horizon_weight, 0.35);
    return lerp(sky.horizon_init, sky.zenith_tint, zenith_weight);
}

float disc_mask(float3 view_dir, float3 light_dir, float angular_radius)
{
    float dot_val = dot(view_dir, light_dir);
    float radius = max(angular_radius, 0.0001);
    float edge = max(radius * 0.15, 0.00005);
    float inner = cos(max(radius - edge, 0.0001));
    float outer = cos(radius + edge);
    return smoothstep(outer, inner, dot_val);
}

float hash21(float2 p)
{
    float n = dot(p, float2(127.1, 311.7));
    return frac(sin(n) * 43758.5453);
}

float noise2d(float2 p)
{
    float2 i = floor(p);
    float2 f = frac(p);
    float a = hash21(i);
    float b = hash21(i + float2(1.0, 0.0));
    float c = hash21(i + float2(0.0, 1.0));
    float d = hash21(i + float2(1.0, 1.0));
    float2 u = f * f * (3.0 - 2.0 * f);
    return lerp(lerp(a, b, u.x), lerp(c, d, u.x), u.y);
}

[shader("fragment")]
FSOutput main(FSInput input)
{
    FSOutput output;
    Camera camera = meshi_bindless_cameras[sky_draw.camera_index];

    float2 ndc = input.uv * 2.0 - 1.0;
    float aspect = camera.viewport.x / camera.viewport.y;
    float tan_half = tan(camera.fov_y_radians * 0.5);
    float3 dir = normalize(float3(ndc.x * aspect * tan_half, ndc.y * tan_half, -1.0));
    float3 world_dir = normalize(mul((float3x3)camera.world_from_camera, dir));

    float3 base_color = sky_gradient(world_dir);
    float sun_dot = max(dot(world_dir, sky.sun_dir), 0.0);
    float moon_dot = max(dot(world_dir, sky.moon_dir), 0.0);

    float sun_disc = disc_mask(world_dir, sky.sun_dir, sky.sun_angular_radius);
    float moon_disc = disc_mask(world_dir, sky.moon_dir, sky.moon_angular_radius);

    float sun_halo = pow(sun_dot, 256.0) * 0.1 + pow(sun_dot, 512.0) * 0.2;
    float moon_halo = pow(moon_dot, 64.0) * 0.25 + pow(moon_dot, 256.0) * 0.8;

    float3 sky_color = base_color;
    sky_color += sky.sun_color * sky.sun_intensity * (sun_disc + sun_halo);
    sky_color += sky.moon_color * sky.moon_intensity * (moon_disc + moon_halo);

    float height = saturate(-world_dir.y * 0.5 + 0.5);
    float horizon_factor = saturate(1.0 - height);
    [unroll]
    for (int i = 0; i < 2; i++)
    {
        float density = sky.fog_color_density[i].w;
        if (density > 0.0)
        {
            float3 fog_color = sky.fog_color_density[i].xyz;
            float layer_height = sky.fog_height_falloff[i].x;
            float falloff = sky.fog_height_falloff[i].y;
            float noise_scale = sky.fog_height_falloff[i].z;
            float noise_intensity = sky.fog_height_falloff[i].w;
            float2 noise_speed = sky.fog_noise_speed[i].xy;

            float height_profile = exp(-abs(height - layer_height) * falloff);
            float2 noise_uv = world_dir.xz * noise_scale + noise_speed * sky.fog_time;
            float noise_val = noise2d(noise_uv);
            float noise_mod = lerp(1.0, noise_val, noise_intensity);
            float fog_factor = saturate(density * height_profile * horizon_factor * noise_mod);
            sky_color = lerp(sky_color, fog_color, fog_factor);
        }
    }

    sky_color *= sky.intensity_scale;

    output.color = float4(sky_color, 1.0);
    return output;
}
