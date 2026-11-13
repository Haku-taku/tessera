// One filling quad: a rounded box with its fill and its stroke, both possibly gradients.

VsOut main(VsIn input) {
    // Reaches `AA` past the box, so a pixel centred just outside it still gets a fragment.
    float2 device = input.a_rect.xy - AA + input.a_unit * (input.a_rect.zw + 2.0 * AA);
    VsOut output;
    output.position = float4(device_to_ndc(device), 0.0, 1.0);
    // From the real device position, not `a_unit`: a snapped quad spans more than the box,
    // and scaling `a_unit` would stretch the SDF space non-uniformly (ovals, fat strokes).
    output.v_local = device - (input.a_rect.xy + input.a_rect.zw * 0.5);
    output.v_half = input.a_rect.zw * 0.5;
    output.v_radius = input.a_radius;
    output.v_fill = input.a_fill;
    output.v_border = input.a_border;
    output.v_border2 = input.a_border2;
    output.v_border_g = input.a_border_g;
    output.v_border_w = input.a_border_w;
    output.v_shape = input.a_shape;
    output.v_device = device;
    output.v_fill2 = input.a_fill2;
    output.v_grad = input.a_grad;
    output.v_stops = input.a_stops;
    output.v_extent = input.a_extent;
    output.v_notch = input.a_notch;
    output.v_flip = input.a_flip;
    return output;
}
