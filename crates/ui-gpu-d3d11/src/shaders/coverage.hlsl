// Rounded-shape coverage shared by every backdrop program. The uniforms (u_rect, u_radius,
// u_exp) are declared in the program's constant buffer.

// Same level set as the CPU rasteriser, so both backends cut the same corner.
float level_set(float x, float y, float n) {
    if (n <= 1.0) {
        return x + y;
    }
    if (n >= 64.0) {
        return max(x, y);
    }
    return pow(pow(x, n) + pow(y, n), 1.0 / n);
}

float coverage(float2 p) {
    float l = u_rect.x;
    float t = u_rect.y;
    float r = l + u_rect.z;
    float b = t + u_rect.w;
    if (p.x < l || p.x > r || p.y < t || p.y > b) {
        return 0.0;
    }
    if (u_radius.x > 0.0 && p.x < l + u_radius.x && p.y < t + u_radius.x) {
        return clamp(u_radius.x + AA
            - level_set(abs(p.x - l - u_radius.x), abs(p.y - t - u_radius.x), u_exp), 0.0, 1.0);
    }
    if (u_radius.y > 0.0 && p.x > r - u_radius.y && p.y < t + u_radius.y) {
        return clamp(u_radius.y + AA
            - level_set(abs(p.x - r + u_radius.y), abs(p.y - t - u_radius.y), u_exp), 0.0, 1.0);
    }
    if (u_radius.z > 0.0 && p.x > r - u_radius.z && p.y > b - u_radius.z) {
        return clamp(u_radius.z + AA
            - level_set(abs(p.x - r + u_radius.z), abs(p.y - b + u_radius.z), u_exp), 0.0, 1.0);
    }
    if (u_radius.w > 0.0 && p.x < l + u_radius.w && p.y > b - u_radius.w) {
        return clamp(u_radius.w + AA
            - level_set(abs(p.x - l - u_radius.w), abs(p.y - b + u_radius.w), u_exp), 0.0, 1.0);
    }
    return 1.0;
}
