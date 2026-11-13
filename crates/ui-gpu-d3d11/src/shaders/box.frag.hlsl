// Full-screen pass: one sliding-window box average along `u_dir`.
//
// The window is summed per fragment rather than carried across pixels, so the cost is
// O(radius) in the shader; three such passes still beat an explicit 85-tap kernel and,
// more importantly, are not limited by `u_weights`.

float4 main(PsIn input) : SV_Target {
    float4 acc = float4(0.0, 0.0, 0.0, 0.0);
    float span = float(2 * u_radius + 1);
    // A uniform bound is legal here and keeps the cost at one tap per tap.
    for (int i = -u_radius; i <= u_radius; i++) {
        float2 offset = u_dir * u_texel * float(i);
        // CLAMP_TO_EDGE keeps the edge pixels weighted like the CPU path does.
        acc += sample_unit(u_src, input.v_uv + offset);
    }
    return mul(u_color, acc / span);
}
