varying vec2 v_uv;

#ifdef VERTEX_SHADER
attribute vec3 a_pos;
uniform mat4 u_projection_matrix;
uniform mat4 u_view_matrix;
uniform mat4 u_model_matrix;
void main() {
    v_uv = a_pos.xy;
    gl_Position = u_projection_matrix * u_view_matrix * u_model_matrix * vec4(a_pos, 1.0);
}
#endif

#ifdef FRAGMENT_SHADER
uniform sampler2D u_texture;
float aa(float x) {
    float w = length(vec2(dFdx(x), dFdy(x)));
    return 1.0 - smoothstep(-w, w, x);
}

float read_sdf(sampler2D textureKEKW, vec2 uv) {
    return 1.0 - 2.0 * texture2D(textureKEKW, uv).x;
}

void main() {
    vec4 u_border_color = vec4(0.0, 0.0, 0.0, 1.0);
    vec4 u_color = vec4(0.0, 0.0, 0.0, 1.0);
    float u_outline_distance = 0.0;

    float dist = read_sdf(u_texture, v_uv);
    float inside = aa(dist);
    float inside_border = aa(dist - u_outline_distance);
    vec4 outside_color = vec4(u_border_color.xyz, 0.0);
    gl_FragColor = u_color * inside +
        (1.0 - inside) * (
            u_border_color * inside_border +
            outside_color * (1.0 - inside_border)
        );
}
#endif