// One ring sector: an annulus clipped to an angular sweep, with optional round caps.

VsOut main(VsIn input) {
    // Reaches `AA` past the outer radius: the feather also covers pixels centred outside it.
    float2 device = input.a_centre + (input.a_unit * 2.0 - 1.0) * (input.a_geom.x + AA);
    VsOut output;
    output.position = float4(device_to_ndc(device), 0.0, 1.0);
    output.v_device = device;
    output.v_centre = input.a_centre;
    output.v_geom = input.a_geom;
    output.v_fill = input.a_fill;
    output.v_caps = input.a_caps;
    return output;
}
