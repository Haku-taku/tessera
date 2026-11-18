
precision highp float;
in vec2 v_uv;
in vec2 v_device;
uniform sampler2D u_src;
uniform vec4 u_body;     // luminance mix, pole, chroma scale, dark tone strength
uniform vec4 u_glass;    // semantic tint, straight rgba
uniform vec4 u_optical;  // refractiveIndex, dispersionStrength, thickness(dp), magnification
uniform vec4 u_light;    // glareIntensity, glareDistanceRange, glareEdgeSharpness, glareDirectionOffset
uniform vec4 u_kit;      // fresnelIntensity, fresnelDistanceRange, fresnelEdgeSharpness, glareAngleConvergence
uniform vec4 u_blend;    // live shape count, smooth-union radius k, glareOppositeSideBias, device scale
uniform vec4 u_capture;  // the captured region in device pixels: xy origin, zw size
uniform vec4 u_shape[4]; // centre.xy, half.xy of each fused shape, device pixels
uniform vec4 u_shape_radius[4]; // tl, tr, br, bl of each fused shape, device pixels
out vec4 frag;

// The reference normalises its distance by the glass view's height and rescales the gradient
// by this, so the normal's length — not just its direction — enters the mix weights.
const float NORMAL_SCALE = 1.414213562 * 1000.0;

const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

// The reference reaches its rim light through a 1414× debug gradient, which pins the whole
// silhouette at 1.0; this gain lights the rim without flattening it into a white stroke.
const float RIM_GAIN = 8.0;

// Apple's dark regular glass, measured over grey sweeps: a concave tone that lifts shadows.
float dark_tone(float l) {
    return 0.1148 + 0.7557 * (1.0 - exp(-max(l, 0.0) / 0.6787));
}

// The neutral body: move luminance, then scale chroma about the backdrop's own luminance.
vec3 body_transfer(vec3 c) {
    float l = dot(c, LUMA);
    if (u_body.w > 0.0) {
        l = mix(l, dark_tone(l), u_body.w);
    }
    l += (u_body.y - l) * u_body.x;
    return clamp(vec3(l) + (c - vec3(dot(c, LUMA))) * u_body.z, 0.0, 1.0);
}

// Body first, then the semantic colour: a brand tint never stands in for the body.
vec3 shade_sample(vec3 c) {
    return mix(body_transfer(c), u_glass.rgb, u_glass.a);
}

vec3 straight(vec4 s) {
    // The material's formulas, including the LAB conversions, are authored on sRGB values,
    // which is what every surface's frame holds.
    return s.a > 0.0 ? s.rgb / s.a : vec3(0.0);
}

// Branchless smooth union: at k = 0 this is exactly min(d1, d2).
float smooth_union(float d1, float d2, float k) {
    float e = max(k - abs(d1 - d2), 0.0);
    return min(d1, d2) - e * e * 0.25 / max(k, 1e-5);
}

// Distance to the fused group; the node's rect only bounds the capture.
float group_sdf(vec2 device) {
    float s0 = sd_round_box(device - u_shape[0].xy, u_shape[0].zw, u_shape_radius[0], u_exp);
    if (u_blend.x < 1.5) {
        return s0;
    }
    float k = u_blend.y;
    float s1 = sd_round_box(device - u_shape[1].xy, u_shape[1].zw, u_shape_radius[1], u_exp);
    if (u_blend.x < 2.5) {
        return smooth_union(s0, s1, k);
    }
    float s2 = sd_round_box(device - u_shape[2].xy, u_shape[2].zw, u_shape_radius[2], u_exp);
    float fwd = smooth_union(smooth_union(s0, s1, k), s2, k);
    float bwd = smooth_union(smooth_union(s2, s1, k), s0, k);
    if (u_blend.x < 3.5) {
        return mix(fwd, bwd, 0.5);
    }
    float s3 = sd_round_box(device - u_shape[3].xy, u_shape[3].zw, u_shape_radius[3], u_exp);
    fwd = smooth_union(fwd, s3, k);
    bwd = smooth_union(smooth_union(smooth_union(s3, s2, k), s1, k), s0, k);
    return mix(fwd, bwd, 0.5);
}

// ---- sRGB / CIELAB / LCH, as the reference carries them (`:256-396`) ----
vec3 linearize(vec3 c) {
    vec3 s = max(c, vec3(0.0));
    return mix(s / 12.92, pow((s + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), s));
}

vec3 gamma_encode(vec3 c) {
    vec3 s = max(c, vec3(0.0));
    return mix(12.92 * s, 1.055 * pow(s, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), s));
}

const mat3 RGB_TO_XYZ = mat3(0.4124, 0.3576, 0.1805,
                             0.2126, 0.7152, 0.0722,
                             0.0193, 0.1192, 0.9505);
const mat3 XYZ_TO_RGB = mat3(3.2406255, -1.537208, -0.4986286,
                             -0.9689307, 1.8757561, 0.0415175,
                             0.0557101, -0.2040211, 1.0569959);
const vec3 WHITE = vec3(0.95045592705, 1.0, 1.08905775076);

float lab_f(float x) {
    return x > 0.00885645167 ? pow(x, 1.0 / 3.0) : 7.78703703704 * x + 0.13793103448;
}

float lab_f_inv(float t) {
    return t > 0.206897 ? t * t * t : 0.12841854934 * (t - 0.137931034);
}

vec3 srgb_to_lab(vec3 srgb) {
    vec3 v = (RGB_TO_XYZ * linearize(srgb)) / WHITE;
    v = vec3(lab_f(v.x), lab_f(v.y), lab_f(v.z));
    return vec3(116.0 * v.y - 16.0, 500.0 * (v.x - v.y), 200.0 * (v.y - v.z));
}

vec3 lab_to_srgb(vec3 lab) {
    float w = (lab.x + 16.0) / 116.0;
    vec3 v = WHITE * vec3(lab_f_inv(w + lab.y / 500.0), lab_f_inv(w), lab_f_inv(w - lab.z / 200.0));
    return gamma_encode(XYZ_TO_RGB * v);
}

vec3 lab_to_lch(vec3 lab) {
    return vec3(lab.x, sqrt(dot(lab.yz, lab.yz)), atan(lab.z, lab.y) * (180.0 / 3.14159265358979));
}

vec3 lch_to_lab(vec3 lch) {
    float h = lch.z * (3.14159265358979 / 180.0);
    return vec3(lch.x, lch.y * cos(h), lch.y * sin(h));
}

// The capture is rasterized at the density the lens is shown at, so a bilinear tap already
// matches it; where the rim packs many texels into one pixel the footprint picks the level
// that averages exactly those texels.
vec3 sample_grad(vec2 uv, vec2 dx, vec2 dy) {
    return straight(textureGrad(u_src, clamp(uv, 0.0, 1.0), dx, dy));
}

// Capture uv at a device position: the capture spans the node plus its margin at one texel
// per device pixel, and the content sits at the material's magnification about the centre.
vec2 capture_uv(vec2 device) {
    vec2 centre = u_capture.xy + u_capture.zw * 0.5;
    return (centre + (device - centre) / max(u_optical.w, 0.05) - u_capture.xy) / u_capture.zw;
}

// Per-channel dispersion: red and blue sample the refraction offset at ±2 % × strength.
vec3 sample_dispersed(vec2 base, vec2 offset, float strength, vec2 dx, vec2 dy) {
    float r = 1.0 + 0.02 * strength;
    float b = 1.0 - 0.02 * strength;
    vec3 c;
    c.r = sample_grad(base + offset * r, dx, dy).r;
    c.g = sample_grad(base + offset, dx, dy).g;
    c.b = sample_grad(base + offset * b, dx, dy).b;
    return c;
}

// Distance to the silhouette in device pixels, negative inside, as the SDFs return it.
float shape_distance(vec2 device) {
    vec2 p = device - (u_rect.xy + u_rect.zw * 0.5);
    return u_blend.x > 0.5 ? group_sdf(device) : sd_round_box(p, u_rect.zw * 0.5, u_radius, u_exp);
}

// The reference normalises that distance by the glass view's own height (`:163-186`), not by
// the capture's: the capture may be wider than the node.
float normalised_distance(vec2 device) {
    return shape_distance(device) / u_rect.w;
}

// Central differences one pixel apart, scaled as the reference scales them: the normal's
// length, not just its direction, enters the Fresnel and glare weights.
vec2 compute_surface_normal(vec2 device) {
    vec2 g = vec2(
        normalised_distance(device + vec2(1.0, 0.0)) - normalised_distance(device - vec2(1.0, 0.0)),
        normalised_distance(device + vec2(0.0, 1.0)) - normalised_distance(device - vec2(0.0, 1.0)));
    return g * 0.5 * NORMAL_SCALE;
}

// Bend at a distance: strongest at the rim, gone once the ray crosses the whole thickness.
float edge_shift_of(float d) {
    // The depth lives in `[0, thickness]`: beyond the rim there is no glass, so the bend
    // saturates instead of leaving the `asin` domain.
    float depth = clamp(-d / u_blend.w, 0.0, u_optical.z);
    float incident = asin(pow(1.0 - depth / u_optical.z, 2.0));
    return -tan(asin(sin(incident) / max(u_optical.x, 1.001)) - incident);
}

// The reference's offset (`:466-470`) in capture uv: the node's device height over its width
// on x, `contentsScale` on y — 70.7 dp of bend per unit edge shift, on either axis.
vec2 lens_offset_uv(vec2 normal, float shift) {
    return -normal * shift * 0.05 * u_blend.w * vec2(u_rect.w / u_rect.z, 1.0);
}

// Lens colour at one point, with the pixel's own filter footprint: shading is a function of
// position, so a caller may average the several points that fall in one pixel.
vec3 lens_colour(vec2 device, vec2 foot_x, vec2 foot_y) {
    float d = shape_distance(device);
    float depth = clamp(-d / u_blend.w, 0.0, u_optical.z);
    float edge_shift = edge_shift_of(d);
    vec2 base = capture_uv(device);
    if (edge_shift <= 0.0) {
        return shade_sample(sample_grad(base, foot_x, foot_y));
    }
    vec2 surface_normal = compute_surface_normal(device);
    vec2 offset_uv = lens_offset_uv(surface_normal, edge_shift);
    vec3 refracted = sample_dispersed(base, offset_uv, u_optical.y, foot_x, foot_y);
    vec3 colour = shade_sample(refracted);

    vec3 tint_half = mix(vec3(1.0), u_glass.rgb, u_glass.a * 0.5);
    if (u_kit.x > 0.0) {
        float f = clamp(pow(max(1.0 - depth / 1500.0
                                * pow(500.0 / u_kit.y, 2.0) + u_kit.z, 0.0), 5.0), 0.0, 1.0);
        vec3 lch = lab_to_lch(srgb_to_lab(tint_half));
        lch.x = clamp(lch.x + 20.0 * f * u_kit.x, 0.0, 100.0);
        colour = mix(colour, lab_to_srgb(lch_to_lab(lch)),
                     min(f * u_kit.x * 0.7 * length(surface_normal), 1.0));
    }
    float geom = clamp(pow(max(1.0 - depth / 1500.0
                               * pow(500.0 / u_light.y, 2.0) + u_light.z, 0.0), 5.0), 0.0, 1.0);
    float glare_angle = atan(surface_normal.y, surface_normal.x);
    glare_angle = glare_angle < 0.0 ? glare_angle + 6.28318530717959 : glare_angle;
    glare_angle = (glare_angle - 3.14159265358979 / 4.0 + u_light.w) * 2.0;
    float far_side = step(4.71238898038469, glare_angle) * step(glare_angle, 10.99557428756428)
                   + step(glare_angle, -1.5707963267949);
    // A specular lobe, not a stroke: only the rim facing the light glints, so the unlit sides
    // stay dark instead of the whole silhouette turning white.
    float lobe = pow(max(sin(glare_angle), 0.0), 1.0 + u_kit.w * 2.0);
    float angular = lobe * (1.0 + far_side * (u_blend.z - 1.0)) * 1.2 * u_light.x;
    // A specular highlight is white light. Lifting the backdrop's own LCH instead turns a
    // near-neutral backdrop's numerical hue into a coloured fringe, so mix toward white.
    float glare = angular * geom * RIM_GAIN;
    glare = glare / (1.0 + glare);
    colour = mix(colour, vec3(1.0), glare * 0.85);
    return colour;
}

void main() {
    // Derivatives are read first, in uniform control flow: a discarded neighbour leaves them
    // undefined, and the footprint they span is the filter width every sub-sample shares.
    float d = shape_distance(v_device);
    float shift = edge_shift_of(d);
    vec2 base = capture_uv(v_device);
    vec2 offset_uv = lens_offset_uv(compute_surface_normal(v_device), shift);
    vec2 foot_x = dFdx(base + offset_uv);
    vec2 foot_y = dFdy(base + offset_uv);

    // Coverage stays one device pixel: widening it blurs the silhouette instead of the band.
    if (d >= AA) {
        discard;
    }
    float t = clamp(AA - d, 0.0, 1.0);
    float alpha = t * t * (3.0 - 2.0 * t);
    vec3 colour = lens_colour(v_device, foot_x, foot_y);
    frag = vec4(colour * alpha, alpha);
}
