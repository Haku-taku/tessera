//! Instance layouts every backend must agree on.

/// Floats per rounded-rect instance: rect, radius, fill, border, shape, far stop, gradient,
/// border width, notch. Colour and width are separate slots: one slot cannot carry both.
pub const RECT_STRIDE: i32 = 39;

/// Floats per glyph instance: dest rect, atlas rect, colour, kind.
pub const GLYPH_STRIDE: i32 = 13;

/// Floats per image instance: dest rect, radius, uv rect, shape exponent.
pub const IMAGE_STRIDE: i32 = 13;

/// Floats per shadow instance: rect, radius, colour, blur + dy, shape, surface edge.
pub const SHADOW_STRIDE: i32 = 21;

/// Largest tap count the effect shader accepts; keep in sync with `u_weights[33]`.
pub const MAX_TAPS: u32 = 33;

/// Unit quad shared by every instanced program; scaled per instance.
pub const UNIT_QUAD: [f32; 12] = [
    0.0, 0.0, 1.0, 0.0, 1.0, 1.0, //
    0.0, 0.0, 1.0, 1.0, 0.0, 1.0,
];
