
// Superellipse norm of a non-negative pair; n = 2 is the circle, n = 1 a bevel.
float lp_norm(vec2 q, float n) {
    if (n <= 1.0) {
        return q.x + q.y;
    }
    if (n >= 64.0) {
        return max(q.x, q.y);
    }
    return pow(pow(q.x, n) + pow(q.y, n), 1.0 / n);
}

// Signed distance to a rounded box; negative inside. `r` is (tl, tr, br, bl), y down, so the
// quadrant pick must take (tl,bl) on the left and (tr,br) on the right — iq's packing is not this one.
float sd_round_box(vec2 p, vec2 b, vec4 r, float n) {
    r.xy = (p.x > 0.0) ? vec2(r.y, r.z) : vec2(r.x, r.w);
    r.x  = (p.y > 0.0) ? r.y : r.x;
    vec2 q = abs(p) - b + r.x;
    return min(max(q.x, q.y), 0.0) + lp_norm(max(q, vec2(0.0)), n) - r.x;
}
