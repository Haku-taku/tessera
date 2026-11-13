float4 main(PsIn input) : SV_Target {
    float m = input.v_color.a * clip_coverage(input.v_device);
    float cover = sample_unit(u_atlas, input.v_uv).r;
    // The colour atlas is already premultiplied; scale it by alpha and clip only.
    float4 colour = sample_unit(u_color_atlas, input.v_uv) * m;
    float4 tinted = float4(input.v_color.rgb * (cover * m), cover * m);
    float4 result = lerp(tinted, colour, step(0.5, input.v_kind));
    if (result.a <= 0.0) {
        discard;
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    return result;
}
