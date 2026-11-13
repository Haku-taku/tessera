// Every full-screen pass draws one instance of a rectangle covering the whole target.

VsOut main(VsIn input) {
    float2 device = input.a_rect.xy + input.a_unit * input.a_rect.zw;
    VsOut output;
    output.v_device = device;
    output.position = float4(device_to_ndc(device), 0.0, 1.0);
    // The layer was rendered with the same projection, so v=0 is device row 0.
    output.v_uv = input.a_unit;
    return output;
}
