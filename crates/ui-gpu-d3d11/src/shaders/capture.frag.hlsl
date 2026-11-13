// The lens's own backdrop capture: one region of the frame, resampled into its own texture.
//
// The capture is rasterized at the density it is shown at, so face and rim need no runtime
// magnification filter (`LiquidGlassView.swift:288-322`).

float4 main(PsIn input) : SV_Target {
    return sample_unit(u_src, u_region.xy + input.v_uv * u_region.zw);
}
