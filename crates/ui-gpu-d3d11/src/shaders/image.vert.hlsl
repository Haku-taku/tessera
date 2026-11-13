// One image: a rounded quad sampling a sub-rect of one texture.

VsOut main(VsIn input) {
    float2 device = input.a_rect.xy + input.a_unit * input.a_rect.zw;
    VsOut output;
    output.position = float4(device_to_ndc(device), 0.0, 1.0);
    output.v_local = input.a_unit * input.a_rect.zw - input.a_rect.zw * 0.5;
    output.v_half = input.a_rect.zw * 0.5;
    output.v_radius = input.a_radius;
    output.v_shape = input.a_shape;
    // The source sub-rect is mapped onto the whole destination, never tiled.
    output.v_uv = lerp(input.a_uv.xy, input.a_uv.zw, input.a_unit);
    output.v_device = device;
    return output;
}
