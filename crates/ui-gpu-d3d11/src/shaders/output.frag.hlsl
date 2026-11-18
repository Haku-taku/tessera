// Output transform: sRGB-encoded premultiplied in, PQ or scRGB premultiplied out.

float srgb_decode(float c) {
    // Mirrored through the origin: a channel outside the working triangle decodes as
    // negative light instead of collapsing to black.
    float magnitude = abs(c);
    float decoded = magnitude <= 0.040449237
        ? magnitude / 12.92
        : pow((magnitude + 0.055) / 1.055, 2.4);
    return c < 0.0 ? -decoded : decoded;
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
    // The frame holds sRGB codes on every surface; an extended one lets them climb past
    // 1.0 and fall below zero, and the decode follows the curve both ways.
    float3 lin = float3(srgb_decode(straight.r), srgb_decode(straight.g), srgb_decode(straight.b));
    // The surface's own gamut: BT.2020 on a PQ surface, the identity on scRGB.
    lin = mul(u_gamut, lin);
#ifdef SCRGB_OUTPUT
    // scRGB is linear light whose 1.0 is 80 nits, so the scale has already carried the
    // scene's white into the surface's unit and nothing is left to encode. Negative
    // channels are colours the display holds outside sRGB's triangle.
    float3 code = u_scale * lin + u_offset;
#else
    float3 code = float3(pq_encode(clamp(u_scale * lin.r + u_offset, 0.0, 1.0)),
                         pq_encode(clamp(u_scale * lin.g + u_offset, 0.0, 1.0)),
                         pq_encode(clamp(u_scale * lin.b + u_offset, 0.0, 1.0)));
#endif
    return float4(code * c.a, c.a);
}
