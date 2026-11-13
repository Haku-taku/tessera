// One triangle covers the viewport from vertex ids alone; v=0 is the frame's top row.

struct VsOut {
    float4 position : SV_Position;
    float2 v_uv : TEXCOORD0;
};

VsOut main(uint id : SV_VertexID) {
    float2 uv = float2((id << 1u) & 2u, id & 2u);
    VsOut output;
    output.v_uv = uv;
    output.position = float4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    return output;
}
