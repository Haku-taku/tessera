// Composite a blurred backdrop as WinUI acrylic: luminosity, tint, then grain.

static const float3 LUMA = float3(0.2126, 0.7152, 0.0722);

// Skia's luminosity/colour pair: keep hue and chroma, take the requested luminosity.
float3 set_lum(float3 c, float l) {
    float3 d = c + (float3)(l - dot(c, LUMA));
    float lo = min(min(d.r, d.g), d.b);
    float hi = max(max(d.r, d.g), d.b);
    float dl = dot(d, LUMA);
    if (lo < 0.0) {
        d = (float3)dl + (d - (float3)dl) * (dl / max(dl - lo, 1e-5));
    }
    if (hi > 1.0) {
        d = (float3)dl + (d - (float3)dl) * ((1.0 - dl) / max(hi - dl, 1e-5));
    }
    return clamp(d, 0.0, 1.0);
}

// Deterministic grain per device pixel; the reference tiles a 2% noise image instead.
float grain(float2 p) {
    return frac(sin(dot(p, float2(12.9898, 78.233))) * 43758.5453);
}

float4 main(PsIn input) : SV_Target {
    float cov = coverage(input.v_device);
    if (cov <= 0.0) {
        discard;
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    float4 s = sample_unit(u_src, input.v_uv);
    if (s.a <= 0.0) {
        discard;
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    // The frame holds premultiplied colour; the blend pair works on straight.
    float3 c = s.rgb / s.a;
    c = lerp(c, set_lum(c, dot(u_luminosity.rgb, LUMA)), u_luminosity.a);
    float3 tinted = u_tint.a >= 0.999 ? u_tint.rgb : set_lum(u_tint.rgb, dot(c, LUMA));
    c = lerp(c, tinted, u_tint.a);
    c = lerp(c, (float3)grain(input.v_device), u_noise);
    return float4(c * s.a, s.a) * cov;
}
