#version 450

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;

void main() {
    float shade = 0.4 + 0.6 * v_uv.y;
    out_color = vec4(0.0, 0.3, 0.6, 1.0) * shade;
}
