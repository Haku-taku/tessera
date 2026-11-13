precision highp float;
in vec2 v_local;
in vec2 v_half;
in vec4 v_radius;
in vec4 v_colour;
in vec2 v_soft;
in float v_shape;
in float v_ring;
in vec2 v_cut_local;
in vec2 v_cut_half;
in float v_inset;
in vec2 v_device;
out vec4 frag;

// Standard normal CDF, Abramowitz & Stegun 7.1.26: |error| < 1.5e-7 for every x.
float normal_cdf(float x) {
    float z = x * 0.70710678118;
    float t = 1.0 / (1.0 + 0.3275911 * abs(z));
    float poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741
               + t * (-1.453152027 + t * 1.061405429))));
    float erf = 1.0 - poly * exp(-z * z);
    return z >= 0.0 ? 0.5 * (1.0 + erf) : 0.5 * (1.0 - erf);
}

void main() {
    float d = sd_round_box(v_local, v_half, v_radius, v_shape);
    // A Gaussian cast of the silhouette: `blur` is its radius, which CSS and Skia define as
    // two sigma, so the shadow is still gone about `blur` past the edge.
    float sigma = max(v_soft.x, 1e-4) * 0.5;
    // A ring caster is the band `ring` deep straddling the silhouette; the difference of the
    // two CDFs is that band blurred, and carries its sub-unit alpha on its own.
    float band = v_ring * 0.5;
    // An inset cast is the same falloff mirrored: the wall outside the surface edge throws
    // inward, so the surface edge carries half the peak on both sides.
    float a = v_inset > 0.5
        ? v_colour.a * normal_cdf(d / sigma)
        : v_ring > 0.0
            ? v_colour.a * (normal_cdf((d + band) / sigma) - normal_cdf((d - band) / sigma))
            : v_colour.a * normal_cdf(-d / sigma);
    // The surface's edge is the shadow's own clip, as CSS clips a box-shadow at the border
    // box. One pixel of feather, the same the surface's fill uses, so a translucent surface
    // shows no seam where its edge meets the cast.
    if (v_cut_half.x > 0.0 && v_cut_half.y > 0.0) {
        float edge = sd_round_box(v_cut_local, v_cut_half, v_radius, v_shape);
        float inside = clamp(AA - edge, 0.0, 1.0);
        a *= v_inset > 0.5 ? inside : 1.0 - inside;
    }
    a *= clip_coverage(v_device);
    if (a <= 0.0) {
        discard;
    }
    frag = vec4(v_colour.rgb * a, a);
}
