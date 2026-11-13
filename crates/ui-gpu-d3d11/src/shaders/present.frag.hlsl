// Copy a finished frame into the presented buffer. The destination view's format orders the
// channels, so a B8G8R8A8 back buffer needs no swizzle here.

Texture2D u_src : register(t0);
SamplerState u_samp : register(s0);

struct VsOut {
    float4 position : SV_Position;
    float2 v_uv : TEXCOORD0;
};

float4 main(VsOut input) : SV_Target {
    return u_src.Sample(u_samp, input.v_uv);
}
