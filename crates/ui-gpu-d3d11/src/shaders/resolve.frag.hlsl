// Supersample resolve: the N x N box average one destination pixel covers.

// Average the N x N source texels one destination pixel covers. Source row 0 is the
// destination's row 0, so no flip is involved; premultiplied encoded values average
// directly, which is the same rounding the frame's own blending uses.
float4 main(PsIn input) : SV_Target {
    float4 acc = float4(0.0, 0.0, 0.0, 0.0);
    for (int j = 0; j < 4; ++j) {
        if (j >= u_ss) { break; }
        for (int i = 0; i < 4; ++i) {
            if (i >= u_ss) { break; }
            float2 offset = (float2(float(i), float(j)) + 0.5) / float(u_ss) - 0.5;
            acc += sample_unit(u_src, input.v_uv + offset / u_viewport);
        }
    }
    return acc / float(u_ss * u_ss);
}
