
precision highp float;
in vec2 v_uv;
in vec4 v_color;
in float v_kind;
in vec2 v_device;
uniform sampler2D u_atlas;
uniform sampler2D u_color_atlas;
out vec4 frag;
void main() {
    float m = v_color.a * clip_coverage(v_device);
    float cover = texture(u_atlas, v_uv).r;
    // The colour atlas is already premultiplied; scale it by alpha and clip only.
    vec4 colour = texture(u_color_atlas, v_uv) * m;
    vec4 tinted = vec4(v_color.rgb * (cover * m), cover * m);
    vec4 result = mix(tinted, colour, step(0.5, v_kind));
    if (result.a <= 0.0) {
        discard;
    }
    frag = result;
}
