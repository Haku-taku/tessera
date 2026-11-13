precision highp float;
in vec2 a_unit;
in vec2 a_from;     // device pixels
in vec2 a_to;       // device pixels
in float a_width;   // stroke width, device pixels
in vec4 a_fill;     // straight rgb + alpha
uniform vec2 u_viewport;
out vec2 v_device;
out vec2 v_from;
out vec2 v_to;
out float v_width;
out vec4 v_fill;
void main() {
    // The quad is the segment's bounding box plus the stroke and the feather.
    float reach = a_width * 0.5 + AA;
    vec2 lo = min(a_from, a_to) - reach;
    vec2 hi = max(a_from, a_to) + reach;
    vec2 device = mix(lo, hi, a_unit);
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_device = device;
    v_from = a_from;
    v_to = a_to;
    v_width = a_width;
    v_fill = a_fill;
}
