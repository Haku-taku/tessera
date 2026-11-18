float4 main(PsIn input) : SV_Target {
    float cov = coverage(input.v_device);
    if (cov <= 0.0) {
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    float4 s = sample_unit(u_layer, input.v_uv);
    // Cleared-but-not-yet-repainted pixels are transparent; storing them as backdrop
    // would let the blur average alpha 0 in and punch a see-through band.
    if (u_skip_empty > 0.5 && s.a <= 0.0) {
        discard;
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    if (u_scrim.x > 0.5 && s.a > 0.0) {
        // A scrim hides light; past the destination's peak a display shows the peak, so
        // that is the most light it has to hide.
        float3 straight = min(s.rgb / s.a, float3(u_scrim.z, u_scrim.z, u_scrim.z)) * u_scrim.y;
        s = float4(straight * s.a, s.a);
        if (u_scrim.w > 0.5) {
            // The hole stays lit: fading the scrim out over its edge keeps the boundary
            // to one fade instead of the two a second scrim beside it would make.
            float2 beyond = max(u_scrim_hole.xy - input.v_device, input.v_device - u_scrim_hole.zw);
            s *= clamp(max(beyond.x, beyond.y) + 0.5, 0.0, 1.0);
        }
    }
    // The layer holds premultiplied colour, so scaling all channels stays valid.
    return s * u_opacity * cov;
}
