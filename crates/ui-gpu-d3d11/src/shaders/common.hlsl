// D3D's NDC y grows upward from the top of the viewport while the engine's device rows
// grow downward, so every vertex shader converts through here and the flip lives once.

float2 device_to_ndc(float2 device) {
    return float2(
        device.x / u_viewport.x * 2.0 - 1.0,
        1.0 - device.y / u_viewport.y * 2.0
    );
}
