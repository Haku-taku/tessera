
precision highp float;
in vec2 v_device;
in vec2 v_centre;
in vec2 v_radii;
in float v_width;
in vec4 v_fill;
in vec4 v_stroke;
out vec4 frag;

// The same gradient-corrected estimate the CPU rasteriser uses: exact on the axes, within
// about a pixel elsewhere.
float sd_ellipse(vec2 p, vec2 radii) {
    float k = length(p / radii);
    float gradient = length(p / (radii * radii));
    if (gradient <= 0.00001) {
        return -min(radii.x, radii.y);
    }
    return k * (k - 1.0) / gradient;
}

void main() {
    float d = sd_ellipse(v_device - v_centre, v_radii);
    float half_w = v_width * 0.5;
    float fill_cov = clamp(AA - d, 0.0, 1.0);
    float stroke_cov = half_w > 0.0 ? clamp(AA - (abs(d) - half_w), 0.0, 1.0) : 0.0;
    float cov = max(fill_cov, stroke_cov);
    if (cov <= 0.0) {
        discard;
    }
    // Fill and stroke meet only in the feather seam, so one blend covers both: the stroke
    // composites over the fill, weighted by its share of the pixel.
    vec4 fill_p = vec4(v_fill.rgb * v_fill.a, v_fill.a);
    vec4 stroke_p = vec4(v_stroke.rgb * v_stroke.a, v_stroke.a);
    vec4 over = stroke_p + fill_p * (1.0 - v_stroke.a);
    vec4 src = mix(fill_p, over, clamp(stroke_cov / cov, 0.0, 1.0));
    float clip = clip_coverage(v_device);
    float a = src.a * cov * clip;
    if (a <= 0.0) {
        discard;
    }
    frag = vec4(src.rgb * cov * clip, a);
}
