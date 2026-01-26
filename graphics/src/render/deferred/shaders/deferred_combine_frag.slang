struct DeferredCombineInfo {
  uint position_texture_id;
  uint albedo_texture_id;
  uint normal_texture_id;
  uint material_texture_id;
  uint shadow_texture_id;
  uint shadow_cascade_count;
  uint shadow_resolution;
  uint _padding;
};

struct ShadowCascadeInfo {
  float4 splits;
  float4x4 matrices[4];
};

static const uint MATERIAL_FLAG_PBR_NORMAL = 1u;
static const uint MATERIAL_FLAG_VERTEX_COLOR = 1u << 1;
static const uint MATERIAL_FLAG_UNLIT = 1u << 2;
static const uint MATERIAL_FLAG_TOON = 1u << 3;
static const uint MATERIAL_FLAG_EMISSIVE_ONLY = 1u << 4;

static const uint LIGHT_TYPE_DIRECTIONAL = 0;
static const uint LIGHT_TYPE_POINT = 1;
static const uint LIGHT_TYPE_SPOT = 2;
static const uint LIGHT_TYPE_AREA_RECT = 3;

struct Camera {
  column_major float4x4 world_from_camera;
  column_major float4x4 projection;
  float2 viewport;
  float near;
  float far;
  float fov_y_radians;
  uint projection_kind;
  float _padding;
};

struct Light {
  float4 position_type;
  float4 direction_range;
  float4 color_intensity;
  float4 spot_area;
  float4 extra;

  bool valid() {
    return (int)position_type.w >= 0;
  }

  float3 direction() {
    return direction_range.rgb;
  }

  float range() {
    return direction_range.w;
  }

  float3 color() {
    return color_intensity.rgb;
  }

  float intensity() {
    return color_intensity.w;
  }
};

uniform Texture2DMS<float4> meshi_bindless_textures[] : register(t0, space0);
uniform SamplerState meshi_bindless_samplers[] : register(s1, space0);

uniform StructuredBuffer<Camera> meshi_bindless_cameras : register(t2, space0);
uniform StructuredBuffer<Light> meshi_bindless_lights : register(t5, space0);

uniform StructuredBuffer<DeferredCombineInfo> per_obj_ssbo : register(t0, space1);
static DeferredCombineInfo per_obj = per_obj_ssbo[0];
uniform StructuredBuffer<ShadowCascadeInfo> shadow_cascade_ssbo : register(t1, space1);
static ShadowCascadeInfo shadow_cascade = shadow_cascade_ssbo[0];

struct FSInput {
  float4 svpos : SV_Position;
  float2 tex_coords : TEXCOORD0;
};

struct FSOutput {
  float4 color : SV_TARGET0;
};

float4 sample_texture(uint idx, int2 coords) {
  return meshi_bindless_textures[idx].Load(coords, 0);
}

float4x4 inverse(float4x4 m) {
  float n11 = m[0][0], n12 = m[1][0], n13 = m[2][0], n14 = m[3][0];
  float n21 = m[0][1], n22 = m[1][1], n23 = m[2][1], n24 = m[3][1];
  float n31 = m[0][2], n32 = m[1][2], n33 = m[2][2], n34 = m[3][2];
  float n41 = m[0][3], n42 = m[1][3], n43 = m[2][3], n44 = m[3][3];

  float t11 = n23 * n34 * n42 - n24 * n33 * n42 + n24 * n32 * n43 - n22 * n34 * n43 - n23 * n32 * n44 + n22 * n33 * n44;
  float t12 = n14 * n33 * n42 - n13 * n34 * n42 - n14 * n32 * n43 + n12 * n34 * n43 + n13 * n32 * n44 - n12 * n33 * n44;
  float t13 = n13 * n24 * n42 - n14 * n23 * n42 + n14 * n22 * n43 - n12 * n24 * n43 - n13 * n22 * n44 + n12 * n23 * n44;
  float t14 = n14 * n23 * n32 - n13 * n24 * n32 - n14 * n22 * n33 + n12 * n24 * n33 + n13 * n22 * n34 - n12 * n23 * n34;

  float det = n11 * t11 + n21 * t12 + n31 * t13 + n41 * t14;
  float idet = 1.0f / det;

  float4x4 ret;

  ret[0][0] = t11 * idet;
  ret[0][1] = (n24 * n33 * n41 - n23 * n34 * n41 - n24 * n31 * n43 + n21 * n34 * n43 + n23 * n31 * n44 - n21 * n33 * n44) * idet;
  ret[0][2] = (n22 * n34 * n41 - n24 * n32 * n41 + n24 * n31 * n42 - n21 * n34 * n42 - n22 * n31 * n44 + n21 * n32 * n44) * idet;
  ret[0][3] = (n23 * n32 * n41 - n22 * n33 * n41 - n23 * n31 * n42 + n21 * n33 * n42 + n22 * n31 * n43 - n21 * n32 * n43) * idet;

  ret[1][0] = t12 * idet;
  ret[1][1] = (n13 * n34 * n41 - n14 * n33 * n41 + n14 * n31 * n43 - n11 * n34 * n43 - n13 * n31 * n44 + n11 * n33 * n44) * idet;
  ret[1][2] = (n14 * n32 * n41 - n12 * n34 * n41 - n14 * n31 * n42 + n11 * n34 * n42 + n12 * n31 * n44 - n11 * n32 * n44) * idet;
  ret[1][3] = (n12 * n33 * n41 - n13 * n32 * n41 + n13 * n31 * n42 - n11 * n33 * n42 - n12 * n31 * n43 + n11 * n32 * n43) * idet;

  ret[2][0] = t13 * idet;
  ret[2][1] = (n14 * n23 * n41 - n13 * n24 * n41 - n14 * n21 * n43 + n11 * n24 * n43 + n13 * n21 * n44 - n11 * n23 * n44) * idet;
  ret[2][2] = (n12 * n24 * n41 - n14 * n22 * n41 + n14 * n21 * n42 - n11 * n24 * n42 - n12 * n21 * n44 + n11 * n22 * n44) * idet;
  ret[2][3] = (n13 * n22 * n41 - n12 * n23 * n41 - n13 * n21 * n42 + n11 * n23 * n42 + n12 * n21 * n43 - n11 * n22 * n43) * idet;

  ret[3][0] = t14 * idet;
  ret[3][1] = (n13 * n24 * n31 - n14 * n23 * n31 + n14 * n21 * n33 - n11 * n24 * n33 - n13 * n21 * n34 + n11 * n23 * n34) * idet;
  ret[3][2] = (n14 * n22 * n31 - n12 * n24 * n31 - n14 * n21 * n32 + n11 * n24 * n32 + n12 * n21 * n34 - n11 * n22 * n34) * idet;
  ret[3][3] = (n12 * n23 * n31 - n13 * n22 * n31 + n13 * n21 * n32 - n11 * n23 * n32 - n12 * n21 * n33 + n11 * n22 * n33) * idet;

  return ret;
}

float4x4 view_from_camera(Camera camera) {
  return inverse(camera.world_from_camera);
}

uint select_cascade(float view_depth, float4 splits, uint cascade_count) {
  uint idx = 0;
  if (cascade_count > 1 && view_depth > splits.x) {
    idx = 1;
  }
  if (cascade_count > 2 && view_depth > splits.y) {
    idx = 2;
  }
  if (cascade_count > 3 && view_depth > splits.z) {
    idx = 3;
  }
  return min(idx, max(cascade_count, 1u) - 1u);
}

float sample_shadow(uint shadow_id, float3 world_pos, uint cascade_index, uint cascade_count, float bias) {
  float4 shadow_pos = mul(shadow_cascade.matrices[cascade_index], float4(world_pos, 1.0));
  shadow_pos.xyz /= max(shadow_pos.w, 0.0001);
  shadow_pos.y = -shadow_pos.y;
  float2 uv = shadow_pos.xy * 0.5 + 0.5;
  if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
    return 1.0;
  }

  float depth = shadow_pos.z;
  if (depth < 0.0 || depth > 1.0) {
    return 1.0;
  }

  uint grid_x = (cascade_count > 2) ? 2u : max(cascade_count, 1u);
  uint grid_y = (cascade_count + grid_x - 1u) / grid_x;
  uint tile_x = cascade_index % grid_x;
  uint tile_y = cascade_index / grid_x;

  float2 atlas_uv = (float2(tile_x, tile_y) + uv) / float2(grid_x, grid_y);
  float2 atlas_size = float2(per_obj.shadow_resolution * grid_x, per_obj.shadow_resolution * grid_y);
  float2 atlas_pos = atlas_uv * atlas_size;

  float shadow = 0.0;
  [unroll]
  for (int y = -1; y <= 1; ++y) {
    [unroll]
    for (int x = -1; x <= 1; ++x) {
      float2 offset = float2(x, y);
      float2 sample_pos = clamp(atlas_pos + offset, float2(0.0, 0.0), atlas_size - 1.0);
      int2 coord = int2(sample_pos);
      float map_depth = meshi_bindless_textures[shadow_id].Load(coord, 0).x;
      shadow += (depth - bias) <= map_depth ? 1.0 : 0.0;
    }
  }

  return shadow / 9.0;
}

[shader("fragment")]
FSOutput main(FSInput input) {
  FSOutput output;
  uint position_id = per_obj.position_texture_id;
  uint albedo_id = per_obj.albedo_texture_id;
  uint normal_id = per_obj.normal_texture_id;
  uint material_id = per_obj.material_texture_id;

  float4 position = sample_texture(position_id, int2(input.svpos.xy));
  float4 albedo = sample_texture(albedo_id, int2(input.svpos.xy));
  float4 normal = sample_texture(normal_id, int2(input.svpos.xy));
  float4 material_code = sample_texture(material_id, int2(input.svpos.xy));

  float depth_factor = saturate(1.0 / (1.0 + length(position.xyz)));
  uint material_flags = uint(material_code.y + 0.5);
  bool is_unlit = (material_flags & MATERIAL_FLAG_UNLIT) != 0;
  bool is_toon = (material_flags & MATERIAL_FLAG_TOON) != 0;
  bool is_emissive_only = (material_flags & MATERIAL_FLAG_EMISSIVE_ONLY) != 0;
  float metallic = saturate(material_code.z);
  float roughness = saturate(material_code.w);
  float3 n = normalize(normal.xyz);
  float3 lighting = albedo.rgb * (0.02 + depth_factor * 0.1);

  if (is_unlit || is_emissive_only) {
    output.color = float4(albedo.rgb, 1.0);
    return output;
  }

  const Camera camera = meshi_bindless_cameras[0];
  float4x4 view = view_from_camera(camera);
  float4 view_pos = mul(view, float4(position.xyz, 1.0));
  float view_depth = -view_pos.z;
  uint cascade_count = max(per_obj.shadow_cascade_count, 1u);
  uint cascade_index = select_cascade(view_depth, shadow_cascade.splits, cascade_count);

  float3 camera_pos = camera.world_from_camera[3].xyz;
  float3 view_dir = normalize(camera_pos - position.xyz);
  float3 f0 = lerp(float3(0.04, 0.04, 0.04), albedo.rgb, metallic);
  float n_dot_v = max(dot(n, view_dir), 0.0);

  uint light_count = 0;
  meshi_bindless_lights.GetDimensions(light_count);
  for (uint light_index = 0; light_index < light_count; ++light_index) {
    const Light light = meshi_bindless_lights[light_index];
    if (!light.valid()) {
      continue;
    }

    uint light_type = uint(light.position_type.w + 0.5);
    float3 light_dir = float3(0.0, 0.0, 0.0);
    float attenuation = 1.0;
    if (light_type == LIGHT_TYPE_DIRECTIONAL) {
      light_dir = normalize(-light.direction());
    } else {
      float3 light_vec = light.position_type.xyz - position.xyz;
      float distance = length(light_vec);
      if (distance > 0.0) {
        light_dir = light_vec / distance;
      }
      float range = light.range();
      if (range > 0.0) {
        float falloff = saturate(1.0 - (distance / range));
        attenuation = falloff * falloff;
      }
      if (light_type == LIGHT_TYPE_SPOT) {
        float3 spot_dir = normalize(light.direction());
        float cos_theta = dot(-light_dir, spot_dir);
        float inner = light.spot_area.x;
        float outer = light.spot_area.y;
        float spot = saturate((cos_theta - outer) / max(inner - outer, 0.001));
        attenuation *= spot;
      }
    }

    float n_dot_l = max(dot(n, light_dir), 0.0);
    if (n_dot_l <= 0.0) {
      continue;
    }
    if (is_toon) {
      const float toon_steps = 4.0;
      n_dot_l = floor(n_dot_l * toon_steps) / toon_steps;
    }

    float3 half_vec = normalize(view_dir + light_dir);
    float n_dot_h = max(dot(n, half_vec), 0.0);
    float v_dot_h = max(dot(view_dir, half_vec), 0.0);

    float alpha = max(roughness * roughness, 0.001);
    float alpha2 = alpha * alpha;
    float denom = (n_dot_h * n_dot_h) * (alpha2 - 1.0) + 1.0;
    float d = alpha2 / max(3.14159265 * denom * denom, 0.001);

    float k = (roughness + 1.0);
    k = (k * k) / 8.0;
    float g_v = n_dot_v / max(n_dot_v * (1.0 - k) + k, 0.001);
    float g_l = n_dot_l / max(n_dot_l * (1.0 - k) + k, 0.001);
    float g = g_v * g_l;

    float3 f = f0 + (1.0 - f0) * pow(1.0 - v_dot_h, 5.0);
    float3 specular = (d * g) * f / max(4.0 * n_dot_v * n_dot_l, 0.001);
    float3 kd = (1.0 - f) * (1.0 - metallic);
    float3 diffuse = kd * albedo.rgb / 3.14159265;

    float shadow_factor = 1.0;
    if (light_type == LIGHT_TYPE_DIRECTIONAL && per_obj.shadow_texture_id != 0u) {
      uint light_flags = asuint(light.extra.x);
      bool casts_shadows = (light_flags & 1u) != 0u;
      if (casts_shadows) {
        float bias = max(0.0005, 0.002 * (1.0 - n_dot_l));
        shadow_factor = sample_shadow(per_obj.shadow_texture_id, position.xyz, cascade_index, cascade_count, bias);
      }
    }

    float3 light_color = light.color() * light.intensity();
    lighting += (diffuse + specular) * light_color * n_dot_l * attenuation * shadow_factor;
  }

  output.color = float4(lighting, 1.0);
  return output;
}
