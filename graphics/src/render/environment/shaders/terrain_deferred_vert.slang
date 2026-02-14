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

  bool valid() {
    return id() != 0xFFFFu;
  }

  int id() {
    return int(m_packed & 0xFFFF);
  }
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

struct VSInput {
  uint vertex_id : SV_VertexID;
  uint instance_id : SV_InstanceID;
};

struct VSOutput {
  float4 position : SV_POSITION;
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

struct VertexData {
  float3 position;
  float3 normal;
  float4 tangent;
  float2 tex_coords;
  float4 color;
  uint4 joints;
  float4 weights;
};

static const uint VERTEX_STRIDE = 96u;

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

static const uint INVALID_BINDLESS_ID = 0xFFFFFFFFu;

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

float4x4 identity_matrix() {
  return float4x4(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0
  );
}

float4x4 model_matrix(PerDrawData draw) {
  if (draw.transform_id.valid()) {
    return meshi_bindless_transformations[draw.transform_id.id()].transform;
  }

  if (draw.scene_id.valid()) {
    return meshi_bindless_transformations[draw.scene_id.id()].transform;
  }

  return identity_matrix();
}



bool sample_terrain_surface(uint clipmap_sample, out float terrain_height, out float3 terrain_normal) {
  uint height_count = 0u;
  uint height_stride = 0u;
  terrain_height_buffer.GetDimensions(height_count, height_stride);
  uint normal_count = 0u;
  uint normal_stride = 0u;
  terrain_normal_buffer.GetDimensions(normal_count, normal_stride);
  if (clipmap_sample >= height_count || clipmap_sample >= normal_count) {
    terrain_height = 0.0;
    terrain_normal = float3(0.0, 1.0, 0.0);
    return false;
  }

  terrain_height = terrain_height_buffer[clipmap_sample].x;
  terrain_normal = normalize(terrain_normal_buffer[clipmap_sample].xyz);
  return true;
}

VertexData load_vertex_data(PerDrawData draw, uint vertex_id) {
  VertexData data;
  const uint base = draw.vertex_id + vertex_id * VERTEX_STRIDE;

  data.position = asfloat(meshi_bindless_vertices.Load3(base + 0));
  data.normal = asfloat(meshi_bindless_vertices.Load3(base + 12));
  data.tangent = asfloat(meshi_bindless_vertices.Load4(base + 24));
  data.tex_coords = asfloat(meshi_bindless_vertices.Load2(base + 40));
  data.color = asfloat(meshi_bindless_vertices.Load4(base + 48));
  data.joints = meshi_bindless_vertices.Load4(base + 64);
  data.weights = asfloat(meshi_bindless_vertices.Load4(base + 80));

  return data;
}

[shader("vertex")]
VSOutput main(VSInput input) {
  VSOutput output;
  const PerDrawData draw = per_draw_ssbo[input.instance_id];
  const VertexData vertex = load_vertex_data(draw, input.vertex_id);

  const Camera camera = meshi_bindless_cameras[scene.camera.id()];
  const float4x4 view = view_from_camera(camera);
  const float4x4 view_proj = mul(camera.projection, view);
  const float4x4 model = model_matrix(draw);

  float3 local_pos = vertex.position;
  float3 local_normal = vertex.normal;
  const uint clipmap_sample = clipmap_index(draw.terrain_clipmap_tile, vertex.tex_coords);
  float terrain_height = 0.0;
  float3 terrain_normal = local_normal;
  if (sample_terrain_surface(clipmap_sample, terrain_height, terrain_normal)) {
    local_pos.y = terrain_height + vertex.position.y;
    local_normal = terrain_normal;
  }

  const float4 world_pos = mul(model, float4(local_pos, 1.0));
  const float3 world_normal = normalize(mul(model, float4(local_normal, 0.0)).xyz);
  const float3 world_tangent = normalize(mul(model, float4(vertex.tangent.xyz, 0.0)).xyz);

  output.position = mul(view_proj, world_pos);
  output.world_pos = world_pos.xyz;
  output.world_normal = world_normal;
  output.tex_coords = vertex.tex_coords;
  output.color = vertex.color;
  output.world_tangent = float4(world_tangent, vertex.tangent.w);
  output.blend_texture_id = draw.terrain_blend_texture_id;
  output.blend_ids_texture_id = draw.terrain_blend_ids_texture_id;
  output.clipmap_tile_index = draw.terrain_clipmap_tile;
  output.material_id = draw.material_id.valid() ? uint(draw.material_id.id()) : 0xFFFFu;

  output.position.y = -output.position.y;
  return output;
}
