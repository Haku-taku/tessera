
precision highp float;
in vec2 v_uv;
uniform sampler2D u_src;
uniform vec4 u_region; // the region to take, in frame uv: origin.xy, size.zw
out vec4 frag;

// The lens reads a capture rasterized at the density it is shown at, so face and rim need
// no runtime magnification filter (`LiquidGlassView.swift:288-322`).
void main() {
    frag = texture(u_src, u_region.xy + v_uv * u_region.zw);
}
