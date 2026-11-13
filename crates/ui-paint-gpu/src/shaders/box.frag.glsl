
precision highp float;
in vec2 v_uv;
uniform sampler2D u_src;
uniform vec2 u_texel;
uniform vec2 u_dir;
uniform int u_radius;
uniform mat4 u_color;
out vec4 frag;
void main() {
    vec4 acc = vec4(0.0);
    float span = float(2 * u_radius + 1);
    // A uniform bound is legal here and keeps the cost at one tap per tap.
    for (int i = -u_radius; i <= u_radius; i++) {
        vec2 off = u_dir * u_texel * float(i);
        // CLAMP_TO_EDGE keeps the edge pixels weighted like the CPU path does.
        acc += texture(u_src, v_uv + off);
    }
    frag = u_color * (acc / span);
}
