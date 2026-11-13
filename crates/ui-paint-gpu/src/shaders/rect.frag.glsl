
precision highp float;
in vec2 v_local;
in vec2 v_half;
in vec4 v_radius;
in vec4 v_fill;
in vec4 v_border;
in vec4 v_border2;
in float v_border_g;
in float v_border_w;
in float v_shape;
in vec2 v_device;
in vec4 v_fill2;
in vec2 v_grad;
in vec4 v_stops;
in vec2 v_extent;
in vec2 v_notch;
in vec2 v_flip;
out vec4 frag;

// Coverage-weighted premultiplied colour so one blend equation covers rects
// and glyphs: rgb must be scaled by alpha, not just by coverage.
vec4 premul(vec4 straight, float coverage) {
    float a = straight.a * coverage;
    return vec4(straight.rgb * a, a);
}

// Map a gradient-space coordinate through a pair of stops.
float remap(float u, float lo, float hi) {
    return clamp((u - lo) / max(hi - lo, 1e-5), 0.0, 1.0);
}

void main() {
    float d = sd_round_box(v_local, v_half, v_radius, v_shape);
    // One device pixel of feather; the SDF is already in pixel units.
    float outer = clamp(AA - d, 0.0, 1.0);
    float width = v_border_w;
    // A gradient is sampled in the box's own space: v_local is centred, v_half is the
    // half extent, so t runs 0..1 across the box on the chosen axis.
    // `along` is the distance from the box's near edge; `span` is either the box's own
    // extent or the brush's shorter run (WinUI's elevation borders cover only a few units).
    bool vertical = v_grad.x < 0.5;
    float along = vertical ? (v_local.y + v_half.y) : (v_local.x + v_half.x);
    float box = vertical ? 2.0 * v_half.y : 2.0 * v_half.x;
    // A mirrored brush measures the same run from the far edge; the box mirrors with it.
    float along_f = v_flip.x > 0.5 ? box - along : along;
    float along_b = v_flip.y > 0.5 ? box - along : along;
    vec4 fill = v_fill;
    if (v_grad.y > 0.5) {
        float span = v_extent.x > 0.0 ? v_extent.x : box;
        fill = mix(v_fill, v_fill2, remap(along_f / max(span, 1e-5), v_stops.x, v_stops.y));
    }
    // Ring and inner area are disjoint parts of one shape: weight them by area and scale
    // once. Two stacked blends count the feather twice and bleed fill into the ring.
    vec4 fill_pm = premul(fill, 1.0);
    vec4 src = fill_pm;
    if (width > 0.0) {
        float inner = clamp(AA - (d + width), 0.0, 1.0);
        float ring = max(outer - inner, 0.0);
        // A notch cuts the stroke where it crosses the top edge; the fill survives, so a
        // transparent box shows the backdrop through the gap. The half-pixel is the stroke's
        // antialiased skirt, which reaches past the band and would otherwise survive as a line.
        float local_x = v_local.x + v_half.x;
        if (v_notch.y > v_notch.x && local_x >= v_notch.x && local_x <= v_notch.y
            && v_local.y <= -v_half.y + width + 0.5) {
            ring = 0.0;
        }
        vec4 stroke = v_border;
        if (v_border_g > 0.5) {
            float span = v_extent.y > 0.0 ? v_extent.y : box;
            stroke = mix(v_border, v_border2, remap(along_b / max(span, 1e-5), v_stops.z, v_stops.w));
        }
        vec4 border_pm = premul(stroke, 1.0);
        vec4 over = border_pm + fill_pm * (1.0 - border_pm.a);
        src = mix(fill_pm, over, clamp(ring / max(outer, 1e-5), 0.0, 1.0));
    }
    vec4 colour = src * outer;
    // The clip attenuates premultiplied colour uniformly.
    colour *= clip_coverage(v_device);
    if (colour.a <= 0.0) {
        discard;
    }
    frag = colour;
}
