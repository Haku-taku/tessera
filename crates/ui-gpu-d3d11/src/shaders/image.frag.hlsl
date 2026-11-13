float4 main(PsIn input) : SV_Target {
    float d = sd_round_box(input.v_local, input.v_half, input.v_radius, input.v_shape);
    // One device pixel of feather; the SDF is already in pixel units.
    float cov = clamp(AA - d, 0.0, 1.0);
    if (cov <= 0.0) {
        discard;
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    // The texture is uploaded premultiplied, so one factor covers shape and clip.
    float4 s = sample_unit(u_image, input.v_uv);
    float k = cov * clip_coverage(input.v_device);
    if (s.a * k <= 0.0) {
        discard;
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    return float4(s.rgb * k, s.a * k);
}
