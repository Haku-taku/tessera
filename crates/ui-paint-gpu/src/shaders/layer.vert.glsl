
precision highp float;
in vec2 a_unit;
in vec4 a_rect;     // dest x, y, w, h in device pixels
uniform vec2 u_viewport;
out vec2 v_uv;
out vec2 v_device;
void main() {
    vec2 device = a_rect.xy + a_unit * a_rect.zw;
    v_device = device;
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    // The layer was rendered with the same projection, so v=0 is device row 0.
    v_uv = a_unit;
}
