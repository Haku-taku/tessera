
uniform vec4 u_rect;    // x, y, w, h in device pixels
uniform vec4 u_radius;  // tl, tr, br, bl in device pixels
uniform float u_exp;    // superellipse exponent

// Same level set as the CPU rasteriser, so both backends cut the same corner.
float snorm(float x, float y, float n) {
    if (n <= 1.0) return x + y;
    if (n >= 64.0) return max(x, y);
    return pow(pow(x, n) + pow(y, n), 1.0 / n);
}

float coverage(vec2 p) {
    float l = u_rect.x, t = u_rect.y;
    float r = l + u_rect.z, b = t + u_rect.w;
    if (p.x < l || p.x > r || p.y < t || p.y > b) return 0.0;
    if (u_radius.x > 0.0 && p.x < l + u_radius.x && p.y < t + u_radius.x)
        return clamp(u_radius.x + AA - snorm(abs(p.x - l - u_radius.x), abs(p.y - t - u_radius.x), u_exp), 0.0, 1.0);
    if (u_radius.y > 0.0 && p.x > r - u_radius.y && p.y < t + u_radius.y)
        return clamp(u_radius.y + AA - snorm(abs(p.x - r + u_radius.y), abs(p.y - t - u_radius.y), u_exp), 0.0, 1.0);
    if (u_radius.z > 0.0 && p.x > r - u_radius.z && p.y > b - u_radius.z)
        return clamp(u_radius.z + AA - snorm(abs(p.x - r + u_radius.z), abs(p.y - b + u_radius.z), u_exp), 0.0, 1.0);
    if (u_radius.w > 0.0 && p.x < l + u_radius.w && p.y > b - u_radius.w)
        return clamp(u_radius.w + AA - snorm(abs(p.x - l - u_radius.w), abs(p.y - b + u_radius.w), u_exp), 0.0, 1.0);
    return 1.0;
}
