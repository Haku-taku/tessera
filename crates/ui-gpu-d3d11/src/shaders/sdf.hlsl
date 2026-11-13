// Signed distance to a rounded box; negative inside. `r` is (tl, tr, br, bl), y down, so the
// quadrant pick must take (tl,bl) on the left and (tr,br) on the right — iq's packing is not this one.

// Superellipse norm of a non-negative pair; n = 2 is the circle, n = 1 a bevel.
float lp_norm(float2 q, float n) {
    if (n <= 1.0) {
        return q.x + q.y;
    }
    if (n >= 64.0) {
        return max(q.x, q.y);
    }
    return pow(pow(q.x, n) + pow(q.y, n), 1.0 / n);
}

float sd_round_box(float2 p, float2 b, float4 r, float n) {
    r.xy = (p.x > 0.0) ? float2(r.y, r.z) : float2(r.x, r.w);
    r.x = (p.y > 0.0) ? r.y : r.x;
    float2 q = abs(p) - b + r.x;
    return min(max(q.x, q.y), 0.0) + lp_norm(max(q, float2(0.0, 0.0)), n) - r.x;
}
