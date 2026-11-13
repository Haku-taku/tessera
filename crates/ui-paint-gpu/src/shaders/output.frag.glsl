
precision highp float;
in vec2 v_uv;
uniform sampler2D u_src;
uniform float u_scale;
uniform float u_offset;
uniform mat3 u_gamut;
out vec4 frag;

float srgb_decode(float c) {
    return c <= 0.040449237 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}

float pq_encode(float l) {
    const float m1 = 2610.0 / 16384.0;
    const float m2 = 2523.0 / 32.0;
    const float c1 = 3424.0 / 4096.0;
    const float c2 = 2413.0 / 128.0;
    const float c3 = 2392.0 / 128.0;
    float p = pow(l, m1);
    return pow((c1 + c2 * p) / (1.0 + c3 * p), m2);
}

void main() {
    vec4 c = texture(u_src, v_uv);
    if (c.a <= 0.0) {
        frag = vec4(0.0);
        return;
    }
    // Premultiplied encoded values cannot be decoded directly; un-premultiply first.
    vec3 straight = c.rgb / c.a;
#ifdef LINEAR_DOMAIN
    // The frame already holds linear light: the decode happened when the primitives
    // were written, so only the gamut and the transfer to the output remain.
    vec3 lin = vec3(max(straight.r, 0.0), max(straight.g, 0.0), max(straight.b, 0.0));
#else
    vec3 lin = vec3(srgb_decode(straight.r), srgb_decode(straight.g), srgb_decode(straight.b));
#endif
    // sRGB -> BT.2020 in linear light; the surface is declared as BT.2020.
    lin = u_gamut * lin;
    vec3 code = vec3(pq_encode(clamp(u_scale * lin.r + u_offset, 0.0, 1.0)),
                     pq_encode(clamp(u_scale * lin.g + u_offset, 0.0, 1.0)),
                     pq_encode(clamp(u_scale * lin.b + u_offset, 0.0, 1.0)));
    frag = vec4(code * c.a, c.a);
}
