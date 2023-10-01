varying vec2 v_uv;
varying vec3 v_world_pos;
varying vec2 v_mask_uv;

#ifdef VERTEX_SHADER
attribute vec3 a_pos;
attribute vec2 a_uv;
uniform mat4 u_projection_matrix;
uniform mat4 u_view_matrix;
uniform mat4 u_model_matrix;
uniform mat3 u_mask_matrix;
void main() {
    v_uv = a_uv;
    v_world_pos = (u_model_matrix * vec4(a_pos, 1.0)).xyz;
    v_mask_uv = (u_mask_matrix * vec3(v_world_pos.xy, 1.0)).xy;
    gl_Position = u_projection_matrix * u_view_matrix * vec4(v_world_pos, 1.0);
}
#endif

#ifdef FRAGMENT_SHADER
uniform sampler2D u_texture;
uniform sampler2D u_mask;

void main() {
    if (texture2D(u_mask, v_mask_uv).a != 1) {
        discard;
    }
    gl_FragColor = texture2D(u_texture, v_uv);
    if (gl_FragColor.a != 1.0) {
        discard;
    }
}
#endif