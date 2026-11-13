static const float TAU = 6.28318530718;

// Feather of one device pixel against a signed distance inside the stroke.
float feather(float d) {
    return clamp(AA - d, 0.0, 1.0);
}

float4 main(PsIn input) : SV_Target {
    float r_out = input.v_geom.x;
    float r_in = input.v_geom.y;
    float start = input.v_geom.z;
    float sweep = input.v_geom.w;
    float2 p = input.v_device - input.v_centre;
    float d = length(p);
    // Annulus first: the same level set the CPU rasteriser uses.
    float ring = max(r_in - d, d - r_out);
    float cov;
    if (sweep >= 1.0) {
        cov = feather(ring);
    } else {
        // Angular distance measured along the arc, in device pixels at this radius.
        float turns = frac(atan2(p.y, p.x) / TAU);
        float rel = frac(turns - start);
        float arc = TAU * d;
        float angle = (rel <= sweep)
            ? -min(rel, sweep - rel) * arc
            : min(rel - sweep, 1.0 - rel) * arc;
        cov = feather(max(ring, angle));
    }
    // Round caps are circles on the stroke's midline at both end points.
    if (input.v_caps > 0.5) {
        float half_w = (r_out - r_in) * 0.5;
        float mid = r_out - half_w;
        float2 a = float2(cos(start * TAU), sin(start * TAU));
        float2 b = float2(cos((start + sweep) * TAU), sin((start + sweep) * TAU));
        float dc = min(length(p - mid * a), length(p - mid * b));
        cov = max(cov, feather(dc - half_w));
    }
    float a = input.v_fill.a * cov * clip_coverage(input.v_device);
    if (a <= 0.0) {
        discard;
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    return float4(input.v_fill.rgb * a, a);
}
