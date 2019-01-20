#version 450

layout(location = 0) in vec2 a_position;
layout(binding = 0) uniform Globals {
    mat4 camera;
} u_globals;
layout(binding = 1) uniform ShapeUniforms {
    mat4 model;
} u_shape;

void main() {
    gl_Position = u_globals.camera * u_shape.model * vec4(a_position, 0, 1);
}
