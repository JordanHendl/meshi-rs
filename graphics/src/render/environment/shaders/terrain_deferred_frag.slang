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

struct Handle {
  uint m_packed;
};
struct PerDrawData {
  Handle scene_id;
  Handle transform_id;
  Handle material_id;
  Handle skeleton_id;
  Handle animation_state_id;
  Handle per_obj_joints_id;
  uint vertex_id;
  uint vertex_count;
  uint index_id;
  uint index_count;
  uint terrain_height_texture_id;
  uint terrain_normal_texture_id;
  uint terrain_blend_texture_id;
  uint terrain_blend_ids_texture_id;
  uint terrain_clipmap_tile;
};

static const uint MATERIAL_FLAG_PBR_NORMAL = 1u;
static const uint MATERIAL_FLAG_VERTEX_COLOR = 1u << 1;
static const uint MATERIAL_FLAG_UNLIT = 1u << 2;
static const uint MATERIAL_FLAG_TOON = 1u << 3;
static const uint MATERIAL_FLAG_EMISSIVE_ONLY = 1u << 4;
static const uint INVALID_BINDLESS_ID = 0xFFFFu;
static const uint INVALID_TEXTURE_ID = 0xFFFFFFFFu;
static const uint INVALID_MATERIAL_ID = 0xFFFFu;

struct FSInput {
  [[vk::location(0)]] float3 world_pos : POSITION_WS;
  [[vk::location(1)]] float3 world_normal : NORMAL_WS;
  [[vk::location(2)]] float2 tex_coords : TEXCOORD0;
  [[vk::location(3)]] float4 color : COLOR0;
  [[vk::location(4)]] float4 world_tangent : TANGENT0;
  [[vk::location(5)]] nointerpolation uint blend_texture_id : TERRAIN_BLEND_TEXTURE;
  [[vk::location(6)]] nointerpolation uint blend_ids_texture_id : TERRAIN_BLEND_IDS_TEXTURE;
  [[vk::location(7)]] nointerpolation uint clipmap_tile_index : TERRAIN_CLIPMAP_TILE;
  [[vk::location(8)]] nointerpolation uint material_id : TERRAIN_MATERIAL_ID;
};

struct PerSceneData {
  Handle camera;
  uint2 clipmap_grid_size;
  uint clipmap_tile_texel_count;
  uint _padding;
};

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

struct Transformation {
  column_major float4x4 transform;
};


struct FSOutput {
  float4 position : SV_TARGET0;
  float4 albedo : SV_TARGET1;
  float4 normal : SV_TARGET2;
  float4 material_code : SV_TARGET3;
};

uniform StructuredBuffer<PerDrawData> per_draw_ssbo : register(t1, space0);
uniform StructuredBuffer<PerSceneData> per_scene_ssbo : register(t2, space0);
uniform StructuredBuffer<float4> terrain_height_buffer : register(t3, space0);
uniform StructuredBuffer<float4> terrain_normal_buffer : register(t4, space0);
uniform StructuredBuffer<float4> terrain_blend_buffer : register(t5, space0);
uniform StructuredBuffer<float4> terrain_blend_ids_buffer : register(t6, space0);
uniform StructuredBuffer<float4> terrain_hole_mask_buffer : register(t7, space0);

uniform Texture2D meshi_bindless_textures[] : register(t0, space1);
uniform SamplerState meshi_bindless_samplers[] : register(s1, space1);
uniform StructuredBuffer<Material> meshi_bindless_materials : register(t3, space1);
uniform StructuredBuffer<Camera> meshi_bindless_cameras : register(t4, space1);
uniform StructuredBuffer<Transformation> meshi_bindless_transformations : register(t0, space2);
uniform ByteAddressBuffer meshi_bindless_vertices : register(t1, space2);

static PerSceneData scene = per_scene_ssbo[0];

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

uint clipmap_index(uint tile_index, float2 uv) {
  const uint2 grid = scene.clipmap_grid_size;
  if (grid.x == 0u || grid.y == 0u) {
    return 0u;
  }
  const float2 clamped = saturate(uv);
  const float2 texel = clamped * float2(grid.x - 1u, grid.y - 1u);
  const uint2 texel_coord = uint2(texel + 0.5);
  return tile_index * scene.clipmap_tile_texel_count + texel_coord.y * grid.x + texel_coord.x;
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
  const uint clipmap_sample = clipmap_index(input.clipmap_tile_index, input.tex_coords);
  const float4 blend_weights = terrain_blend_buffer[clipmap_sample];
  const float4 blend_ids = terrain_blend_ids_buffer[clipmap_sample];
  const bool use_fallback_material =
    all(blend_ids == float4(0.0, 0.0, 0.0, 0.0)) && input.material_id != INVALID_MATERIAL_ID;
  const uint4 material_ids = use_fallback_material
    ? uint4(input.material_id, 0u, 0u, 0u)
    : uint4(blend_ids + 0.5);
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
  output.albedo = input.color;
  output.normal = float4(input.world_normal, 1.0);
  output.material_code = float4(0.0, float(material_flags), metallic, roughness);
  return output;
}
