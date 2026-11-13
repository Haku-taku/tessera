
precision highp float;
in vec2 v_local;
in vec2 v_half;
in vec4 v_radius;
in float v_shape;
in vec2 v_uv;
in vec2 v_device;
uniform sampler2D u_image;
out vec4 frag;
void main() {
    float d = sd_round_box(v_local, v_half, v_radius, v_shape);
    // One device pixel of feather; the SDF is already in pixel units.
    float cov = clamp(AA - d, 0.0, 1.0);
    if (cov <= 0.0) {
        discard;
    }
    // The texture is uploaded premultiplied, so one factor covers shape and clip.
    vec4 s = texture(u_image, v_uv);
    float k = cov * clip_coverage(v_device);
    if (s.a * k <= 0.0) {
        discard;
    }
    frag = vec4(s.rgb * k, s.a * k);
}
