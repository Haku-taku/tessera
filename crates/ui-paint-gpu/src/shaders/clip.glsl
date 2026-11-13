
uniform vec4 u_clip_rect;     // x, y, w, h in device pixels
uniform vec4 u_clip_radius;   // tl, tr, br, bl in device pixels
uniform float u_clip_shape;   // superellipse exponent
uniform float u_clip_on;

// Superellipse norm of a non-negative pair; n = 2 is the circle, n = 1 a bevel.
float clip_lp_norm(vec2 q, float n) {
    if (n <= 1.0) {
        return q.x + q.y;
    }
    if (n >= 64.0) {
        return max(q.x, q.y);
    }
    return pow(pow(q.x, n) + pow(q.y, n), 1.0 / n);
}

// Coverage of the rounded clip at a device position; 1.0 when no clip is set.
float clip_coverage(vec2 device) {
    if (u_clip_on < 0.5) {
        return 1.0;
    }
    vec2 half_size = u_clip_rect.zw * 0.5;
    vec2 local = device - (u_clip_rect.xy + half_size);
    vec4 r = u_clip_radius;
    // Same (tl, tr, br, bl) packing as `sd_round_box`, so the same quadrant pick.
    r.xy = (local.x > 0.0) ? vec2(r.y, r.z) : vec2(r.x, r.w);
    r.x = (local.y > 0.0) ? r.y : r.x;
    vec2 q = abs(local) - half_size + r.x;
    float d = min(max(q.x, q.y), 0.0) + clip_lp_norm(max(q, vec2(0.0)), u_clip_shape) - r.x;
    return clamp(AA - d, 0.0, 1.0);
}
