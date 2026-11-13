precision highp float;
in vec2 v_device;
in vec2 v_from;
in vec2 v_to;
in float v_width;
in vec4 v_fill;
out vec4 frag;

// Distance from `p` to the segment `a..b`; round caps fall out of clamping the
// projection, which is invisible at the stroke widths math uses.
float segment_distance(vec2 p, vec2 a, vec2 b) {
    vec2 ba = b - a;
    float length_squared = dot(ba, ba);
    float t = length_squared > 0.0
        ? clamp(dot(p - a, ba) / length_squared, 0.0, 1.0)
        : 0.0;
    return length(p - (a + ba * t));
}

void main() {
    float d = segment_distance(v_device, v_from, v_to) - v_width * 0.5;
    float cov = clamp(AA - d, 0.0, 1.0);
    float a = v_fill.a * cov * clip_coverage(v_device);
    if (a <= 0.0) {
        discard;
    }
    frag = vec4(v_fill.rgb * a, a);
}
