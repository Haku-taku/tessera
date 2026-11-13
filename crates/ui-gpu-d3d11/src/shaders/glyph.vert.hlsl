// One glyph: a quad sampling either the coverage atlas or the colour atlas.

VsOut main(VsIn input) {
    float2 device = input.a_rect.xy + input.a_unit * input.a_rect.zw;
    VsOut output;
    output.position = float4(device_to_ndc(device), 0.0, 1.0);
    float2 texel = input.a_atlas.xy + input.a_unit * input.a_atlas.zw;
    float2 atlas_size = lerp(u_atlas_size, u_color_atlas_size, step(0.5, input.a_kind));
    output.v_uv = texel / atlas_size;
    output.v_color = input.a_color;
    output.v_kind = input.a_kind;
    output.v_device = device;
    return output;
}
