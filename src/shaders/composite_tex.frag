#version 450

layout(location = 0) in vec2 v_position;
layout(binding = 2) uniform sampler2D u_texture;
layout(location = 0) out vec4 out_color;

void main() {
    out_color = texture(u_texture, v_position);
    if (out_color.a <= 0.) discard;
}
