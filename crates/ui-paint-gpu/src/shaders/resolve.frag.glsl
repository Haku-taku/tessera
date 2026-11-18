
precision highp float;
in vec2 v_uv;
uniform sampler2D u_src;
uniform vec2 u_viewport; // destination size, device pixels
uniform int u_ss;        // supersample factor the source was rendered at
out vec4 frag;

// Average the N x N source texels one destination pixel covers. Source row 0 is the
// destination's row 0, so no flip is involved; premultiplied encoded values average
// directly, which is the same rounding the frame's own blending uses.
void main() {
    vec4 acc = vec4(0.0);
    for (int j = 0; j < 4; ++j) {
        if (j >= u_ss) { break; }
        for (int i = 0; i < 4; ++i) {
            if (i >= u_ss) { break; }
            vec2 off = (vec2(float(i), float(j)) + 0.5) / float(u_ss) - 0.5;
            acc += texture(u_src, v_uv + off / u_viewport);
        }
    }
    frag = acc / float(u_ss * u_ss);
}
