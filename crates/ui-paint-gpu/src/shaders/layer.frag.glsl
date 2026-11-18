precision highp float;
in vec2 v_uv;
in vec2 v_device;
uniform sampler2D u_layer;
uniform float u_opacity;
uniform float u_skip_empty; // 1 when copying a backdrop: skip pixels nothing painted yet
uniform vec4 u_scrim;       // x runs it, y is the light it leaves, z is the peak's code
uniform vec4 u_scrim_hole;  // w of u_scrim says whether this is a hole: (x0, y0, x1, y1)
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
    if (u_scrim.x > 0.5 && s.a > 0.0) {
        // A scrim hides light; past the destination's peak a display shows the peak, so
        // that is the most light it has to hide.
        vec3 straight = min(s.rgb / s.a, vec3(u_scrim.z)) * u_scrim.y;
        s = vec4(straight * s.a, s.a);
        if (u_scrim.w > 0.5) {
            // The hole stays lit: fading the scrim out over its edge keeps the boundary
            // to one fade instead of the two a second scrim beside it would make.
            vec2 beyond = max(u_scrim_hole.xy - v_device, v_device - u_scrim_hole.zw);
            s *= clamp(max(beyond.x, beyond.y) + 0.5, 0.0, 1.0);
        }
    }
    // The layer holds premultiplied colour, so scaling all channels stays valid.
    frag = s * u_opacity * cov;
}
