// GPU-driven shadow map vertex shader.

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
};

uniform StructuredBuffer<PerDrawData> per_draw_ssbo : register(t0, space1);

struct PerSceneData {
    column_major float4x4 light_view_proj;
};

uniform StructuredBuffer<PerSceneData> per_scene_ssbo : register(t0, space2);
static PerSceneData scene = per_scene_ssbo[0];

struct Transformation {
    column_major float4x4 transform;
};

struct PerObjectJointTransform {
    column_major float4x4 transform;
};

uniform StructuredBuffer<Transformation> meshi_bindless_transformations : register(t3, space0);
uniform ByteAddressBuffer meshi_bindless_vertices : register(t12, space0);
uniform StructuredBuffer<PerObjectJointTransform> meshi_per_obj_joints : register(t15, space0);

struct VSInput {
    uint vertex_id : SV_VertexID;
    uint instance_id : SV_InstanceID;
};

struct VSOutput {
    float4 position : SV_POSITION;
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

float4x4 joint_skin_matrix(PerDrawData draw, uint joint_index) {
    const uint base = (uint)draw.per_obj_joints_id.id();
    return meshi_per_obj_joints[base + joint_index].transform;
}

float4x4 skinning_matrix(PerDrawData draw, uint4 joints, float4 weights) {
    if (!draw.per_obj_joints_id.valid()) {
        return identity_matrix();
    }

    const float weight_sum = weights.x + weights.y + weights.z + weights.w;
    if (weight_sum <= 0.0) {
        return identity_matrix();
    }

    float4x4 skin = float4x4(0.0);
    skin += weights.x * joint_skin_matrix(draw, joints.x);
    skin += weights.y * joint_skin_matrix(draw, joints.y);
    skin += weights.z * joint_skin_matrix(draw, joints.z);
    skin += weights.w * joint_skin_matrix(draw, joints.w);
    return skin;
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

    const float4x4 model = model_matrix(draw);
    const float4x4 skin = skinning_matrix(draw, vertex.joints, vertex.weights);
    const float4x4 mvp = mul(scene.light_view_proj, model);

    const float4 skinned_pos = mul(skin, float4(vertex.position, 1.0));

    output.position = mul(mvp, skinned_pos);
    return output;
}
