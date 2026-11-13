
precision highp float;
in vec2 a_unit;
in vec4 a_rect;     // dest x, y, w, h in device pixels
in vec4 a_atlas;    // atlas x, y, w, h in texels
in vec4 a_color;    // straight rgb, alpha = coverage multiplier
in float a_kind;    // 0 = coverage atlas, 1 = colour atlas
uniform vec2 u_viewport;
uniform vec2 u_atlas_size;
uniform vec2 u_color_atlas_size;
out vec2 v_uv;
out vec4 v_color;
out float v_kind;
out vec2 v_device;
void main() {
    vec2 device = a_rect.xy + a_unit * a_rect.zw;
    // No y inversion: GL row 0 and a dma-buf row 0 are the same memory row, and
    // the compositor reads that row as the top of the image.
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    vec2 texel = a_atlas.xy + a_unit * a_atlas.zw;
    vec2 atlas_size = mix(u_atlas_size, u_color_atlas_size, step(0.5, a_kind));
    v_uv = texel / atlas_size;
    v_color = a_color;
    v_kind = a_kind;
    v_device = device;
}
