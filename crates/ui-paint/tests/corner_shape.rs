//! Corner curve families: round, squircle, bevel, square.
//!
//! These are presets on one continuous axis (`superellipse(k)` in CSS), not separate
//! kinds, so the tests check the *geometry* each k produces rather than a switch.

use ui_core::NodeId;
use ui_core::damage::Damage;
use ui_core::draw::{DrawList, RectPrim};
use ui_core::geom::{Color, CornerShape, Corners, Rect};
use ui_paint::{CpuRenderer, Pixels};

const SIDE: u32 = 40;
const RADIUS: f32 = 20.0;

/// Render one square with the given corner shape.
fn render(shape: CornerShape) -> Pixels {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, SIDE as f32, SIDE as f32));
    list.push_rect(
        NodeId::from_raw(1, 0),
        RectPrim {
            rect: Rect::new(0.0, 0.0, SIDE as f32, SIDE as f32),
            radius: Corners::all(RADIUS).with_shape(shape),
            fill: Color::WHITE,
            border: None,
            border_gradient: None,
            gradient: None,
            notch: None,
        },
    );
    let mut target = Pixels::new(SIDE, SIDE);
    CpuRenderer::new().render(&list, &mut target, &Damage::Full);
    target
}

fn alpha(p: &Pixels, x: u32, y: u32) -> u8 {
    p.data[((y * p.width + x) * 4 + 3) as usize]
}

/// Total coverage inside the top-left corner box.
fn corner_area(p: &Pixels) -> u32 {
    let r = RADIUS as u32;
    (0..r)
        .map(|y| (0..r).map(|x| alpha(p, x, y) as u32).sum::<u32>())
        .sum()
}

#[test]
fn squircle_fills_more_of_the_corner_than_round() {
    // A higher exponent hugs the corner box more tightly, so a pixel the circle
    // excludes can still be inside the squircle.
    let round = render(CornerShape::ROUND);
    let squircle = render(CornerShape::SQUIRCLE);
    // (4,4) is 15.5px from the arc centre: the circle needs 21.9px (outside), the
    // squircle needs 18.4px (comfortably inside).
    assert_eq!(alpha(&round, 4, 4), 0, "the circle excludes (4,4)");
    assert_eq!(alpha(&squircle, 4, 4), 255, "the squircle includes (4,4)");
    assert!(
        corner_area(&squircle) > corner_area(&round),
        "a squircle must fill more corner area than a circle"
    );
}

#[test]
fn corner_fill_increases_monotonically_with_k() {
    // The axis is continuous, so filled area must be monotonic in k; a step backwards
    // would mean the exponent mapping is wrong.
    let areas: Vec<u32> = [
        CornerShape::BEVEL,
        CornerShape::ROUND,
        CornerShape::SQUIRCLE,
        CornerShape::from_k(4.0),
        CornerShape::SQUARE,
    ]
    .into_iter()
    .map(|s| corner_area(&render(s)))
    .collect();
    assert!(
        areas.windows(2).all(|w| w[0] < w[1]),
        "corner area must grow with k, got {areas:?}"
    );
}
