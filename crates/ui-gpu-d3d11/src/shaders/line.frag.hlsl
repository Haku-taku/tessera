// Distance from `p` to the segment `a..b`; round caps fall out of clamping the
// projection, which is invisible at the stroke widths math uses.
float segment_distance(float2 p, float2 a, float2 b) {
    float2 ba = b - a;
    float length_squared = dot(ba, ba);
    float t = length_squared > 0.0
        ? clamp(dot(p - a, ba) / length_squared, 0.0, 1.0)
        : 0.0;
    return length(p - (a + ba * t));
}

float4 main(PsIn input) : SV_Target {
    float d = segment_distance(input.v_device, input.v_from, input.v_to) - input.v_width * 0.5;
    float cov = clamp(AA - d, 0.0, 1.0);
    float a = input.v_fill.a * cov * clip_coverage(input.v_device);
    if (a <= 0.0) {
        discard;
        return float4(0.0, 0.0, 0.0, 0.0);
    }
    return float4(input.v_fill.rgb * a, a);
}
