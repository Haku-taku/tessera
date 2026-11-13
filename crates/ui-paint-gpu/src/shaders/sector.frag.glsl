
precision highp float;
in vec2 v_device;
in vec2 v_centre;
in vec4 v_geom;
in vec4 v_fill;
in float v_caps;
out vec4 frag;

const float TAU = 6.28318530718;

// Feather of one device pixel against a signed distance inside the stroke.
float feather(float d) {
    return clamp(AA - d, 0.0, 1.0);
}

void main() {
    float r_out = v_geom.x;
    float r_in = v_geom.y;
    float start = v_geom.z;
    float sweep = v_geom.w;
    vec2 p = v_device - v_centre;
    float d = length(p);
    // Annulus first: the same level set the CPU rasteriser uses.
    float ring = max(r_in - d, d - r_out);
    float cov;
    if (sweep >= 1.0) {
        cov = feather(ring);
    } else {
        // Angular distance measured along the arc, in device pixels at this radius.
        float turns = fract(atan(p.y, p.x) / TAU);
        float rel = fract(turns - start);
        float arc = TAU * d;
        float angle = (rel <= sweep)
            ? -min(rel, sweep - rel) * arc
            : min(rel - sweep, 1.0 - rel) * arc;
        cov = feather(max(ring, angle));
    }
    // Round caps are circles on the stroke's midline at both end points.
    if (v_caps > 0.5) {
        float half_w = (r_out - r_in) * 0.5;
        float mid = r_out - half_w;
        vec2 a = vec2(cos(start * TAU), sin(start * TAU));
        vec2 b = vec2(cos((start + sweep) * TAU), sin((start + sweep) * TAU));
        float dc = min(length(p - mid * a), length(p - mid * b));
        cov = max(cov, feather(dc - half_w));
    }
    float a = v_fill.a * cov * clip_coverage(v_device);
    if (a <= 0.0) {
        discard;
    }
    frag = vec4(v_fill.rgb * a, a);
}
