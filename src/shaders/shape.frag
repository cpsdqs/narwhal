#version 450

layout(location = 0) out vec4 out_color;
layout(push_constant) uniform ShapePushConstants {
    vec4 color;
} p_shape;

void main() {
    out_color = p_shape.color;
}
