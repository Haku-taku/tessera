
precision highp float;
in vec2 a_unit;
in vec4 a_rect;      // dest x, y, w, h in device pixels
in vec4 a_radius;    // tl, tr, br, bl
in vec4 a_uv;        // u0, v0, u1, v1 normalised
in float a_shape;    // superellipse exponent 2^k
uniform vec2 u_viewport;
out vec2 v_local;
out vec2 v_half;
out vec4 v_radius;
out float v_shape;
out vec2 v_uv;
out vec2 v_device;
void main() {
    vec2 device = a_rect.xy + a_unit * a_rect.zw;
    // No y inversion: GL row 0 is the same memory row the compositor reads as the image top.
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_local = a_unit * a_rect.zw - a_rect.zw * 0.5;
    v_half = a_rect.zw * 0.5;
    v_radius = a_radius;
    v_shape = a_shape;
    // The source sub-rect is mapped onto the whole destination, never tiled.
    v_uv = mix(a_uv.xy, a_uv.zw, a_unit);
    v_device = device;
}
