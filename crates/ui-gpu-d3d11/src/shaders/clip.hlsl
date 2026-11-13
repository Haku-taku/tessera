// Clip uniforms and the coverage test shared by every fragment program. The uniforms are
// declared in the program's constant buffer; this block only holds the functions.

// Superellipse norm of a non-negative pair; n = 2 is the circle, n = 1 a bevel.
float clip_lp_norm(float2 q, float n) {
    if (n <= 1.0) {
        return q.x + q.y;
    }
    if (n >= 64.0) {
        return max(q.x, q.y);
    }
    return pow(pow(q.x, n) + pow(q.y, n), 1.0 / n);
}

// Coverage of the rounded clip at a device position; 1.0 when no clip is set.
float clip_coverage(float2 device) {
    if (u_clip_on < 0.5) {
        return 1.0;
    }
    float2 half_size = u_clip_rect.zw * 0.5;
    float2 local = device - (u_clip_rect.xy + half_size);
    float4 r = u_clip_radius;
    // Same (tl, tr, br, bl) packing as `sd_round_box`, so the same quadrant pick.
    r.xy = (local.x > 0.0) ? float2(r.y, r.z) : float2(r.x, r.w);
    r.x = (local.y > 0.0) ? r.y : r.x;
    float2 q = abs(local) - half_size + r.x;
    float d = min(max(q.x, q.y), 0.0) + clip_lp_norm(max(q, float2(0.0, 0.0)), u_clip_shape) - r.x;
    return clamp(AA - d, 0.0, 1.0);
}
