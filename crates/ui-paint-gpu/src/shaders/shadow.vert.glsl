precision highp float;
in vec2 a_unit;
in vec4 a_rect;      // the *shadowed* shape, device pixels, y down
in vec4 a_radius;
in vec4 a_colour;    // rgb + peak alpha
in vec2 a_soft;      // x: falloff width, y: dy
in float a_shape;
in float a_ring;     // casting band depth; 0 shadows the whole shape
in vec4 a_cut;       // the surface edge that stops the cast; empty casts freely
in float a_inset;    // 1: inside that edge; 0: outside it
uniform vec2 u_viewport;
out vec2 v_local;
out vec2 v_half;
out vec4 v_radius;
out vec4 v_colour;
out vec2 v_soft;
out float v_shape;
out float v_ring;
out vec2 v_cut_local;
out vec2 v_cut_half;
out float v_inset;
out vec2 v_device;
void main() {
    // Grow by the casting band, three sigma of the Gaussian, and the offset: the whole
    // gradient fits.
    vec2 pad = vec2(a_ring * 0.5 + a_soft.x * 1.5 + abs(a_soft.y));
    // An inset cast runs from the caster boundary out to the surface edge, so the quad has
    // to cover both; an outer one is gone three sigma past its own caster anyway.
    vec4 area = a_rect;
    bool has_cut = a_cut.z > 0.0 && a_cut.w > 0.0;
    if (has_cut) {
        vec2 lo = min(a_rect.xy, a_cut.xy);
        vec2 hi = max(a_rect.xy + a_rect.zw, a_cut.xy + a_cut.zw);
        area = vec4(lo, hi - lo);
    }
    vec2 origin = area.xy - pad;
    vec2 size = area.zw + 2.0 * pad;
    // Reaches `AA` past the falloff so the outermost covered pixel is still rasterised.
    vec2 device = origin - AA + a_unit * (size + 2.0 * AA);
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    // Evaluate the SDF in the caster's own space, from the real device position, with the
    // caster moved by `dy`: the offset has to reach the distance, not just the quad.
    v_local = device - (a_rect.xy + a_rect.zw * 0.5 + vec2(0.0, a_soft.y));
    v_half = a_rect.zw * 0.5;
    v_radius = a_radius;
    v_colour = a_colour;
    v_soft = a_soft;
    v_shape = a_shape;
    v_ring = a_ring;
    // The surface stays put; only the cast it bounds moves with `dy`.
    v_cut_local = device - (a_cut.xy + a_cut.zw * 0.5);
    v_cut_half = a_cut.zw * 0.5;
    v_inset = a_inset;
    v_device = device;
}
