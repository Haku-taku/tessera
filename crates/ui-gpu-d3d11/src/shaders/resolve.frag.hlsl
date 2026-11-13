// Supersample resolve: the N x N box average one destination pixel covers.

float srgb_encode(float l) {
    return l <= 0.0031308 ? l * 12.92 : 1.055 * pow(l, 1.0 / 2.4) - 0.055;
}

// Average the N x N source texels one destination pixel covers. Source row 0 is the
// destination's row 0, so no flip is involved; premultiplied values average directly.
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
    float4 avg = acc / float(u_ss * u_ss);
    if (u_encode == 1 && avg.a > 0.0) {
        // Averaging happens in linear light; the encode is the last step, and it works
        // on straight channels, so un-premultiply first.
        float3 straight = avg.rgb / avg.a;
        avg = float4(float3(srgb_encode(max(straight.r, 0.0)),
                            srgb_encode(max(straight.g, 0.0)),
                            srgb_encode(max(straight.b, 0.0))) * avg.a, avg.a);
    }
    return avg;
}
