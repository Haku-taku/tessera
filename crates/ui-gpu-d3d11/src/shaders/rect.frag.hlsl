// Coverage-weighted premultiplied colour so one blend equation covers rects
// and glyphs: rgb must be scaled by alpha, not just by coverage.
float4 premul(float4 straight, float cov) {
    float a = straight.a * cov;
    return float4(straight.rgb * a, a);
}

// Map a gradient-space coordinate through a pair of stops.
float remap_stops(float u, float lo, float hi) {
    return clamp((u - lo) / max(hi - lo, 1e-5), 0.0, 1.0);
}

float4 main(PsIn input) : SV_Target {
    float d = sd_round_box(input.v_local, input.v_half, input.v_radius, input.v_shape);
    // One device pixel of feather; the SDF is already in pixel units.
    float outer = clamp(AA - d, 0.0, 1.0);
    float width = input.v_border_w;
    // A gradient is sampled in the box's own space: v_local is centred, v_half is the
    // half extent, so t runs 0..1 across the box on the chosen axis.
    // `along` is the distance from the box's near edge; `span` is either the box's own
    // extent or the brush's shorter run (WinUI's elevation borders cover only a few units).
    bool vertical = input.v_grad.x < 0.5;
    float along = vertical ? (input.v_local.y + input.v_half.y) : (input.v_local.x + input.v_half.x);
    float box = vertical ? 2.0 * input.v_half.y : 2.0 * input.v_half.x;
    // A mirrored brush measures the same run from the far edge; the box mirrors with it.
    float along_f = input.v_flip.x > 0.5 ? box - along : along;
    float along_b = input.v_flip.y > 0.5 ? box - along : along;
    float4 fill = input.v_fill;
    if (input.v_grad.y > 0.5) {
        float span = input.v_extent.x > 0.0 ? input.v_extent.x : box;
        fill = lerp(input.v_fill, input.v_fill2,
                    remap_stops(along_f / max(span, 1e-5), input.v_stops.x, input.v_stops.y));
    }
    // Ring and inner area are disjoint parts of one shape: weight them by area and scale
    // once. Two stacked blends count the feather twice and bleed fill into the ring.
    float4 fill_pm = premul(fill, 1.0);
    float4 src = fill_pm;
    if (width > 0.0) {
        float inner = clamp(AA - (d + width), 0.0, 1.0);
        float ring = max(outer - inner, 0.0);
        // A notch cuts the stroke where it crosses the top edge; the fill survives, so a
        // transparent box shows the backdrop through the gap. The half-pixel is the stroke's
        // antialiased skirt, which reaches past the band and would otherwise survive as a line.
        float local_x = input.v_local.x + input.v_half.x;
        if (input.v_notch.y > input.v_notch.x && local_x >= input.v_notch.x
            && local_x <= input.v_notch.y
            && input.v_local.y <= -input.v_half.y + width + 0.5) {
            ring = 0.0;
        }
        float4 stroke = input.v_border;
        if (input.v_border_g > 0.5) {
            float span = input.v_extent.y > 0.0 ? input.v_extent.y : box;
            stroke = lerp(input.v_border, input.v_border2,
                          remap_stops(along_b / max(span, 1e-5), input.v_stops.z, input.v_stops.w));
        }
        float4 border_pm = premul(stroke, 1.0);
        float4 over = border_pm + fill_pm * (1.0 - border_pm.a);
        src = lerp(fill_pm, over, clamp(ring / max(outer, 1e-5), 0.0, 1.0));
    }
    float4 colour = src * outer;
    // The clip attenuates premultiplied colour uniformly.
    colour *= clip_coverage(input.v_device);
    if (colour.a <= 0.0) {
        discard;
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    return colour;
}
