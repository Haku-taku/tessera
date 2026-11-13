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
    // The layer holds premultiplied colour, so scaling all channels stays valid.
    return s * u_opacity * cov;
}
