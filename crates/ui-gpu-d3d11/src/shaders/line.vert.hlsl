// One straight stroke: its quad is the segment's bounding box plus the stroke and feather.

VsOut main(VsIn input) {
    float reach = input.a_width * 0.5 + AA;
    float2 lo = min(input.a_from, input.a_to) - reach;
    float2 hi = max(input.a_from, input.a_to) + reach;
    float2 device = lerp(lo, hi, input.a_unit);
    VsOut output;
    output.position = float4(device_to_ndc(device), 0.0, 1.0);
    output.v_device = device;
    output.v_from = input.a_from;
    output.v_to = input.a_to;
    output.v_width = input.a_width;
    output.v_fill = input.a_fill;
    return output;
}
