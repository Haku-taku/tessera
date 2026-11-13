//! Hand-drawn icon strokes: the engine has no rotation, so a diagonal is a staircase of squares.

/// Squares of side `thickness` covering `from` → `to`, ordered along the segment.
pub(crate) fn bars_along(
    from: (f32, f32),
    to: (f32, f32),
    thickness: f32,
) -> Vec<(f32, f32, f32, f32)> {
    let (x0, y0) = from;
    let (x1, y1) = to;
    let (dx, dy) = (x1 - x0, y1 - y0);
    // Half a stroke of travel per square: any coarser and the staircase shows gaps.
    let steps = (((dx * dx + dy * dy).sqrt() / (thickness * 0.5)).ceil() as usize).max(2);
    let mut bars = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let (cx, cy) = (x0 + dx * t, y0 + dy * t);
        bars.push((
            cx - thickness * 0.5,
            cy - thickness * 0.5,
            thickness,
            thickness,
        ));
    }
    bars
}

/// Linear interpolation between two points, in the drawing's own tuple space.
pub(crate) fn mix_point(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (
        ui_core::geom::lerp(a.0, b.0, t),
        ui_core::geom::lerp(a.1, b.1, t),
    )
}
