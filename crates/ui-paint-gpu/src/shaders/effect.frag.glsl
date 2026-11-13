
precision highp float;
in vec2 v_uv;
uniform sampler2D u_src;
uniform vec2 u_texel;      // 1 / texture size
uniform vec2 u_dir;        // (1,0) horizontal, (0,1) vertical, (0,0) single tap
uniform int u_taps;        // weights in use, odd
uniform float u_weights[33];
uniform mat4 u_color;
out vec4 frag;
void main() {
    int radius = (u_taps - 1) / 2;
    vec4 acc = vec4(0.0);
    for (int i = 0; i < 33; i++) {
        if (i < u_taps) {
            vec2 off = u_dir * u_texel * float(i - radius);
            // CLAMP_TO_EDGE matches the CPU path's border clamp.
            acc += texture(u_src, v_uv + off) * u_weights[i];
        }
    }
    frag = u_color * acc;
}
