// Full-screen pass: one separable convolution tap row plus a colour matrix.

float4 main(PsIn input) : SV_Target {
    int radius = (u_taps - 1) / 2;
    float4 acc = float4(0.0, 0.0, 0.0, 0.0);
    for (int i = 0; i < 33; i++) {
        if (i < u_taps) {
            float2 offset = u_dir * u_texel * float(i - radius);
            // CLAMP_TO_EDGE matches the CPU path's border clamp.
            acc += sample_unit(u_src, input.v_uv + offset) * tap_weight(i);
        }
    }
    return mul(u_color, acc);
}
