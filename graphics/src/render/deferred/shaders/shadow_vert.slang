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
    Handle camera;
};

uniform StructuredBuffer<PerSceneData> per_scene_ssbo : register(t0, space2);
static PerSceneData scene = per_scene_ssbo[0];

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

struct PerObjectJointTransform {
    column_major float4x4 transform;
};

uniform StructuredBuffer<Camera> meshi_bindless_cameras : register(t2, space0);
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

    const Camera camera = meshi_bindless_cameras[scene.camera.id()];
    const float4x4 view = view_from_camera(camera);
    const float4x4 view_proj = mul(camera.projection, view);
    const float4x4 model = model_matrix(draw);
    const float4x4 skin = skinning_matrix(draw, vertex.joints, vertex.weights);
    const float4x4 mvp = mul(view_proj, model);

    const float4 skinned_pos = mul(skin, float4(vertex.position, 1.0));

    output.position = mul(mvp, skinned_pos);
    output.position.y = -output.position.y;
    return output;
}
