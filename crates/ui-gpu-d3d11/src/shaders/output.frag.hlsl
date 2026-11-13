// Output transform: sRGB-encoded premultiplied in, PQ-encoded premultiplied out.

float srgb_decode(float c) {
    return c <= 0.040449237 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}

float pq_encode(float l) {
    static const float m1 = 2610.0 / 16384.0;
    static const float m2 = 2523.0 / 32.0;
    static const float c1 = 3424.0 / 4096.0;
    static const float c2 = 2413.0 / 128.0;
    static const float c3 = 2392.0 / 128.0;
    float p = pow(l, m1);
    return pow((c1 + c2 * p) / (1.0 + c3 * p), m2);
}

float4 main(PsIn input) : SV_Target {
    float4 c = sample_unit(u_src, input.v_uv);
    if (c.a <= 0.0) {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    // Premultiplied encoded values cannot be decoded directly; un-premultiply first.
    float3 straight = c.rgb / c.a;
#ifdef LINEAR_DOMAIN
    // The frame already holds linear light: the decode happened when the primitives
    // were written, so only the gamut and the transfer to the output remain.
    float3 lin = float3(max(straight.r, 0.0), max(straight.g, 0.0), max(straight.b, 0.0));
#else
    float3 lin = float3(srgb_decode(straight.r), srgb_decode(straight.g), srgb_decode(straight.b));
#endif
    // sRGB -> BT.2020 in linear light; the surface is declared as BT.2020.
    lin = mul(u_gamut, lin);
    float3 code = float3(pq_encode(clamp(u_scale * lin.r + u_offset, 0.0, 1.0)),
                         pq_encode(clamp(u_scale * lin.g + u_offset, 0.0, 1.0)),
                         pq_encode(clamp(u_scale * lin.b + u_offset, 0.0, 1.0)));
    return float4(code * c.a, c.a);
}
