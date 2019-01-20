#version 450

layout(location = 0) in vec4 a_position;
layout(location = 0) out vec2 v_position;
layout(binding = 0) uniform Globals {
    mat4 camera;
} u_globals;
layout(binding = 1) uniform CompTexUniforms {
    mat4 transform;
} u_tex;

void main() {
    v_position = a_position.zw;
    gl_Position = u_globals.camera * u_tex.transform * vec4(a_position.xy, 0, 1);
}
