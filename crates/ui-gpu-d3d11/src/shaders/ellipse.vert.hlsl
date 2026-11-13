// One ellipse: a filled body and an optional stroke, sharing one quad.

VsOut main(VsIn input) {
    // The quad reaches past the stroke and the feather: a pixel centred outside the curve
    // still takes coverage from it.
    float2 reach = input.a_radii + input.a_width * 0.5 + AA;
    float2 device = input.a_centre + (input.a_unit * 2.0 - 1.0) * reach;
    VsOut output;
    output.position = float4(device_to_ndc(device), 0.0, 1.0);
    output.v_device = device;
    output.v_centre = input.a_centre;
    output.v_radii = input.a_radii;
    output.v_width = input.a_width;
    output.v_fill = input.a_fill;
    output.v_stroke = input.a_stroke;
    return output;
}
