// The same gradient-corrected estimate the CPU rasteriser uses: exact on the axes, within
// about a pixel elsewhere.
float sd_ellipse(float2 p, float2 radii) {
    float k = length(p / radii);
    float gradient = length(p / (radii * radii));
    if (gradient <= 0.00001) {
        return -min(radii.x, radii.y);
    }
    return k * (k - 1.0) / gradient;
}

float4 main(PsIn input) : SV_Target {
    float d = sd_ellipse(input.v_device - input.v_centre, input.v_radii);
    float half_w = input.v_width * 0.5;
    float fill_cov = clamp(AA - d, 0.0, 1.0);
    float stroke_cov = half_w > 0.0 ? clamp(AA - (abs(d) - half_w), 0.0, 1.0) : 0.0;
    float cov = max(fill_cov, stroke_cov);
    if (cov <= 0.0) {
        discard;
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    // Fill and stroke meet only in the feather seam, so one blend covers both: the stroke
    // composites over the fill, weighted by its share of the pixel.
    float4 fill_p = float4(input.v_fill.rgb * input.v_fill.a, input.v_fill.a);
    float4 stroke_p = float4(input.v_stroke.rgb * input.v_stroke.a, input.v_stroke.a);
    float4 over = stroke_p + fill_p * (1.0 - input.v_stroke.a);
    float4 src = lerp(fill_p, over, clamp(stroke_cov / cov, 0.0, 1.0));
    float clip = clip_coverage(input.v_device);
    float a = src.a * cov * clip;
    if (a <= 0.0) {
        discard;
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    return float4(src.rgb * cov * clip, a);
}
