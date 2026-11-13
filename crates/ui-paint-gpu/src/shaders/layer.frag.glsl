
precision highp float;
in vec2 v_uv;
in vec2 v_device;
uniform sampler2D u_layer;
uniform float u_opacity;
uniform float u_skip_empty; // 1 when copying a backdrop: skip pixels nothing painted yet
out vec4 frag;

void main() {
    float cov = coverage(v_device);
    if (cov <= 0.0) {
        frag = vec4(0.0);
        return;
    }
    vec4 s = texture(u_layer, v_uv);
    // Cleared-but-not-yet-repainted pixels are transparent; storing them as backdrop
    // would let the blur average alpha 0 in and punch a see-through band.
    if (u_skip_empty > 0.5 && s.a <= 0.0) {
        discard;
    }
    // The layer holds premultiplied colour, so scaling all channels stays valid.
    frag = s * u_opacity * cov;
}
