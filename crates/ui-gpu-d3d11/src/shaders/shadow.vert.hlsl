// One cast shadow: a Gaussian falloff of the caster's silhouette, optionally cut at a surface.

VsOut main(VsIn input) {
    // Grow by the casting band, three sigma of the Gaussian, and the offset: the whole
    // gradient fits.
    float pad = input.a_ring * 0.5 + input.a_soft.x * 1.5 + abs(input.a_soft.y);
    // An inset cast runs from the caster boundary out to the surface edge, so the quad has
    // to cover both; an outer one is gone three sigma past its own caster anyway.
    float4 area = input.a_rect;
    bool has_cut = input.a_cut.z > 0.0 && input.a_cut.w > 0.0;
    if (has_cut) {
        float2 lo = min(input.a_rect.xy, input.a_cut.xy);
        float2 hi = max(input.a_rect.xy + input.a_rect.zw, input.a_cut.xy + input.a_cut.zw);
        area = float4(lo, hi - lo);
    }
    float2 origin = area.xy - pad;
    float2 size = area.zw + 2.0 * pad;
    // Reaches `AA` past the falloff so the outermost covered pixel is still rasterised.
    float2 device = origin - AA + input.a_unit * (size + 2.0 * AA);
    VsOut output;
    output.position = float4(device_to_ndc(device), 0.0, 1.0);
    // Evaluate the SDF in the caster's own space, from the real device position, with the
    // caster moved by `dy`: the offset has to reach the distance, not just the quad.
    output.v_local = device - (input.a_rect.xy + input.a_rect.zw * 0.5
                               + float2(0.0, input.a_soft.y));
    output.v_half = input.a_rect.zw * 0.5;
    output.v_radius = input.a_radius;
    output.v_colour = input.a_colour;
    output.v_soft = input.a_soft;
    output.v_shape = input.a_shape;
    output.v_ring = input.a_ring;
    // The surface stays put; only the cast it bounds moves with `dy`.
    output.v_cut_local = device - (input.a_cut.xy + input.a_cut.zw * 0.5);
    output.v_cut_half = input.a_cut.zw * 0.5;
    output.v_inset = input.a_inset;
    output.v_device = device;
    return output;
}
