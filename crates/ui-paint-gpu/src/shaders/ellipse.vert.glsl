
precision highp float;
in vec2 a_unit;
in vec2 a_centre;   // device pixels
in vec2 a_radii;    // semi-axes, device pixels
in float a_width;   // stroke width, device pixels; 0 strokes nothing
in vec4 a_fill;     // straight rgb + alpha
in vec4 a_stroke;   // straight rgb + alpha
uniform vec2 u_viewport;
out vec2 v_device;
out vec2 v_centre;
out vec2 v_radii;
out float v_width;
out vec4 v_fill;
out vec4 v_stroke;
void main() {
    // The quad reaches past the stroke and the feather: a pixel centred outside the curve
    // still takes coverage from it.
    vec2 reach = a_radii + a_width * 0.5 + AA;
    vec2 device = a_centre + (a_unit * 2.0 - 1.0) * reach;
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_device = device;
    v_centre = a_centre;
    v_radii = a_radii;
    v_width = a_width;
    v_fill = a_fill;
    v_stroke = a_stroke;
}
