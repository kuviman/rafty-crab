#ifdef VERTEX_SHADER
attribute vec3 a_pos;
uniform mat4 u_projection_matrix;
uniform mat4 u_view_matrix;
uniform mat4 u_model_matrix;
void main() {
    gl_Position = u_projection_matrix * u_view_matrix * u_model_matrix * vec4(a_pos, 1.0);
}
#endif

#ifdef FRAGMENT_SHADER
uniform vec4 u_water_color;
void main() {
    gl_FragColor = u_water_color;
}
#endif