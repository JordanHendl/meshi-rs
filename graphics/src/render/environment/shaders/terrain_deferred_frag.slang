struct Material {
  uint base_color_texture_id;
  uint normal_texture_id;
  uint metallic_roughness_texture_id;
  uint occlusion_texture_id;
  uint emissive_texture_id;
  uint material_flags;
  uint render_mask;
  uint _padding1;
};

static const uint MATERIAL_FLAG_PBR_NORMAL = 1u;
static const uint MATERIAL_FLAG_VERTEX_COLOR = 1u << 1;
static const uint MATERIAL_FLAG_UNLIT = 1u << 2;
static const uint MATERIAL_FLAG_TOON = 1u << 3;
static const uint MATERIAL_FLAG_EMISSIVE_ONLY = 1u << 4;
static const uint INVALID_BINDLESS_ID = 0xFFFFu;
static const uint INVALID_TEXTURE_ID = 0xFFFFFFFFu;

struct FSInput {
  [[vk::location(0)]] float3 world_pos : POSITION_WS;
  [[vk::location(1)]] float3 world_normal : NORMAL_WS;
  [[vk::location(2)]] float2 tex_coords : TEXCOORD0;
  [[vk::location(3)]] float4 color : COLOR0;
  [[vk::location(4)]] float4 world_tangent : TANGENT0;
  [[vk::location(5)]] nointerpolation uint blend_texture_id : TERRAIN_BLEND_TEXTURE;
  [[vk::location(6)]] nointerpolation uint blend_ids_texture_id : TERRAIN_BLEND_IDS_TEXTURE;
};

struct FSOutput {
  float4 position : SV_TARGET0;
  float4 albedo : SV_TARGET1;
  float4 normal : SV_TARGET2;
  float4 material_code : SV_TARGET3;
};

uniform Texture2D meshi_bindless_textures[] : register(t0, space0);
uniform SamplerState meshi_bindless_samplers[] : register(s1, space0);
uniform StructuredBuffer<Material> meshi_bindless_materials : register(t4, space0);

float4 sample_texture(uint idx, float2 coords) {
  return meshi_bindless_textures[idx].Sample(meshi_bindless_samplers[idx], coords);
}

float4 sample_bindless(uint idx, float2 coords, float4 fallback) {
  if (idx == INVALID_TEXTURE_ID) {
    return fallback;
  }
  return sample_texture(idx, coords);
}

bool valid_texture(uint texture_id) {
  return texture_id != INVALID_BINDLESS_ID;
}

float3 resolve_normal(Material material, FSInput input) {
  float3 n = normalize(input.world_normal);
  const bool use_pbr_normal = (material.material_flags & MATERIAL_FLAG_PBR_NORMAL) != 0;
  if (!use_pbr_normal || !valid_texture(material.normal_texture_id)) {
    return n;
  }

  float3 t = normalize(input.world_tangent.xyz);
  float3 b = normalize(cross(n, t)) * input.world_tangent.w;
  float3x3 tbn = float3x3(t, b, n);
  float3 normal_map =
    sample_texture(material.normal_texture_id, input.tex_coords).xyz * 2.0 - 1.0;
  return normalize(mul(normal_map, tbn));
}

float4 resolve_base_color(Material material, FSInput input) {
  const bool use_vertex_color = (material.material_flags & MATERIAL_FLAG_VERTEX_COLOR) != 0;
  const bool use_emissive = (material.material_flags & MATERIAL_FLAG_EMISSIVE_ONLY) != 0;
  if (use_vertex_color) {
    return input.color;
  }

  const uint texture_id = use_emissive ? material.emissive_texture_id : material.base_color_texture_id;
  if (valid_texture(texture_id)) {
    return sample_texture(texture_id, input.tex_coords);
  }

  return input.color;
}

[shader("fragment")]
FSOutput main(FSInput input) {
  FSOutput output;
  const float4 blend_weights = sample_bindless(
    input.blend_texture_id,
    input.tex_coords,
    float4(1.0, 0.0, 0.0, 0.0)
  );
  const float4 blend_ids = sample_bindless(
    input.blend_ids_texture_id,
    input.tex_coords,
    float4(0.0, 0.0, 0.0, 0.0)
  );
  const uint4 material_ids = uint4(blend_ids + 0.5);
  float total_weight = 0.0;
  float3 blended_color = float3(0.0, 0.0, 0.0);
  float3 blended_normal = float3(0.0, 0.0, 0.0);
  float metallic = 0.0;
  float roughness = 0.0;
  uint material_flags = 0u;

  for (int i = 0; i < 4; ++i) {
    const float weight = blend_weights[i];
    if (weight <= 0.0) {
      continue;
    }

    const uint material_id = material_ids[i];
    const Material material = meshi_bindless_materials[material_id];
    material_flags |= material.material_flags;

    const float4 base_color = resolve_base_color(material, input);
    blended_color += base_color.rgb * weight;
    blended_normal += resolve_normal(material, input) * weight;

    float material_metallic = 0.0;
    float material_roughness = 1.0;
    if (valid_texture(material.metallic_roughness_texture_id)) {
      const float4 metallic_roughness =
        sample_texture(material.metallic_roughness_texture_id, input.tex_coords);
      material_roughness = saturate(metallic_roughness.g);
      material_metallic = saturate(metallic_roughness.b);
    }

    metallic += material_metallic * weight;
    roughness += material_roughness * weight;
    total_weight += weight;
  }

  if (total_weight > 0.0) {
    const float inv_weight = 1.0 / total_weight;
    blended_color *= inv_weight;
    metallic *= inv_weight;
    roughness *= inv_weight;
    blended_normal = normalize(blended_normal * inv_weight);
  } else {
    blended_color = input.color.rgb;
    blended_normal = normalize(input.world_normal);
    metallic = 0.0;
    roughness = 1.0;
  }

  output.position = float4(input.world_pos, 1.0);
  output.albedo = float4(blended_color, 1.0);
  output.normal = float4(blended_normal, 1.0);
  output.material_code = float4(0.0, float(material_flags), metallic, roughness);
  return output;
}
