
precision highp float;
in vec2 v_uv;
in vec2 v_device;
uniform sampler2D u_src;
uniform vec4 u_tint;        // straight rgb, alpha = TintOpacity x modifier
uniform vec4 u_luminosity;  // straight rgb, alpha = TintLuminosityOpacity
uniform float u_noise;      // grain layer opacity
out vec4 frag;

const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

// Skia's luminosity/colour pair: keep hue and chroma, take the requested luminosity.
vec3 set_lum(vec3 c, float l) {
    vec3 d = c + vec3(l - dot(c, LUMA));
    float lo = min(min(d.r, d.g), d.b);
    float hi = max(max(d.r, d.g), d.b);
    float dl = dot(d, LUMA);
    if (lo < 0.0) {
        d = vec3(dl) + (d - vec3(dl)) * (dl / max(dl - lo, 1e-5));
    }
    if (hi > 1.0) {
        d = vec3(dl) + (d - vec3(dl)) * ((1.0 - dl) / max(hi - dl, 1e-5));
    }
    return clamp(d, 0.0, 1.0);
}

// Deterministic grain per device pixel; the reference tiles a 2% noise image instead.
float grain(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void main() {
    float cov = coverage(v_device);
    if (cov <= 0.0) {
        discard;
    }
    vec4 s = texture(u_src, v_uv);
    if (s.a <= 0.0) {
        discard;
    }
    // The frame holds premultiplied colour; the blend pair works on straight.
    vec3 c = s.rgb / s.a;
    c = mix(c, set_lum(c, dot(u_luminosity.rgb, LUMA)), u_luminosity.a);
    vec3 tinted = u_tint.a >= 0.999 ? u_tint.rgb : set_lum(u_tint.rgb, dot(c, LUMA));
    c = mix(c, tinted, u_tint.a);
    c = mix(c, vec3(grain(v_device)), u_noise);
    frag = vec4(c * s.a, s.a) * cov;
}
