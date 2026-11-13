
precision highp float;
in vec2 a_unit;
in vec2 a_centre;   // device pixels
in vec4 a_geom;     // outer radius, inner radius, start turns, sweep turns
in vec4 a_fill;     // straight rgb + alpha
in float a_caps;    // 1 = round caps at both ends
uniform vec2 u_viewport;
out vec2 v_device;
out vec2 v_centre;
out vec4 v_geom;
out vec4 v_fill;
out float v_caps;
void main() {
    // Reaches `AA` past the outer radius: the feather also covers pixels centred outside it.
    vec2 device = a_centre + (a_unit * 2.0 - 1.0) * (a_geom.x + AA);
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_device = device;
    v_centre = a_centre;
    v_geom = a_geom;
    v_fill = a_fill;
    v_caps = a_caps;
}
