
precision highp float;
in vec2 a_unit;
in vec4 a_rect;      // x, y, w, h in device pixels, y down
in vec4 a_radius;    // tl, tr, br, bl
in vec4 a_fill;
in vec4 a_border;    // straight rgb + alpha
in vec4 a_border2;   // the stroke gradient's far stop
in float a_border_g; // 1 = the stroke carries a gradient
in float a_border_w; // border width in device pixels
in float a_shape;    // superellipse exponent 2^k
in vec4 a_fill2;     // gradient's far stop
in vec2 a_grad;      // x: 0 = vertical, 1 = horizontal; y: 1 = enabled
in vec4 a_stops;     // fill near/far, border near/far, as gradient-space u
in vec2 a_extent;    // fill and border gradient lengths in device px; 0 = the box's own
in vec2 a_notch;     // top-border gap, x range from the box's left edge; empty when x >= y
in vec2 a_flip;      // 1 = that gradient is measured from the box's far edge
uniform vec2 u_viewport;
out vec2 v_local;
out vec2 v_half;
out vec4 v_radius;
out vec4 v_fill;
out vec4 v_border;
out vec4 v_border2;
out float v_border_g;
out float v_border_w;
out float v_shape;
out vec2 v_device;
out vec4 v_fill2;
out vec2 v_grad;
out vec4 v_stops;
out vec2 v_extent;
out vec2 v_notch;
out vec2 v_flip;
void main() {
    // Reaches `AA` past the box, so a pixel centred just outside it still gets a fragment.
    vec2 device = a_rect.xy - AA + a_unit * (a_rect.zw + 2.0 * AA);
    // No y inversion: GL row 0 and a dma-buf row 0 are the same memory row, and
    // the compositor reads that row as the top of the image.
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    // From the real device position, not `a_unit`: a snapped quad spans more than the box,
    // and scaling `a_unit` would stretch the SDF space non-uniformly (ovals, fat strokes).
    v_local = device - (a_rect.xy + a_rect.zw * 0.5);
    v_half = a_rect.zw * 0.5;
    v_radius = a_radius;
    v_fill = a_fill;
    v_border = a_border;
    v_border2 = a_border2;
    v_border_g = a_border_g;
    v_border_w = a_border_w;
    v_shape = a_shape;
    v_device = device;
    v_fill2 = a_fill2;
    v_grad = a_grad;
    v_stops = a_stops;
    v_extent = a_extent;
    v_notch = a_notch;
    v_flip = a_flip;
}
