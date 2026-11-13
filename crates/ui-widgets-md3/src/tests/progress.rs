//! The MD3 linear progress indicator: a track, and an active bar `value` of the width long.

use super::*;

use ui_core::draw::Shape;
use ui_core::geom::Rect;
use ui_core::layout::AxisSize;

const BAR: Id = Id::str("progress");

fn build(store: &mut AnimStore, width: f32, value: f32) -> Decl {
    let mut core = ui_core::cx::Cx::new(store, 0, 600, 200, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control = Md3ProgressIndicator::new_with(
        Md3Options::default(),
        BAR,
        width,
        value,
        &mut cx,
        crate::pack::pack(),
    );
    cx.with_cx(|cx| control.decl(cx))
}

fn active_of(decl: &Decl) -> Option<Rect> {
    decl.shapes.iter().find_map(|s| match s {
        Shape::Rect { rect, .. } => Some(*rect),
        _ => None,
    })
}

fn build_wavy(store: &mut AnimStore, value: f32) -> Decl {
    let mut core = ui_core::cx::Cx::new(store, 0, 600, 200, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control = Md3ProgressIndicator::wavy_with(
        Md3Options::default(),
        BAR,
        240.0,
        value,
        &mut cx,
        crate::pack::pack(),
    );
    cx.with_cx(|cx| control.decl(cx))
}

/// The vertical span of every stroked segment, in the bar's own space.
fn wave_span(decl: &Decl) -> (f32, f32, f32, f32) {
    let mut span = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for shape in &decl.shapes {
        if let Shape::Line { from, to, .. } = shape {
            span.0 = span.0.min(from.y).min(to.y);
            span.1 = span.1.max(from.y).max(to.y);
            span.2 = span.2.min(from.x).min(to.x);
            span.3 = span.3.max(from.x).max(to.x);
        }
    }
    span
}

#[test]
fn the_wavy_bar_is_ten_dp_tall_and_paints_its_value_as_a_wave() {
    // The expressive form reads its own container and wave tokens; the classic table would
    // paint a 4 dp bar with no wave and no stop dot.
    let mut store = AnimStore::new();
    let half = build_wavy(&mut store, 0.5);
    assert_eq!(half.size_class.height, AxisSize::fixed(10.0));
    let (low, high, left, right) = wave_span(&half);
    assert!(high - low > 4.0, "the wave spans {} dp", high - low);
    assert!(
        left < 6.0 && (right - 120.0).abs() < 6.0,
        "the wave covers {left}..{right}, the value's half"
    );
    // The stop dot sits 5 dp from the trailing edge: half a dot plus half the spare height.
    let dot = half
        .shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Rect { rect, .. }
                if rect.origin.x > 200.0
                    && (rect.size.w - 4.0).abs() < 0.5
                    && (rect.size.h - 4.0).abs() < 0.5 =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .expect("the stop dot is painted");
    assert!(
        (dot.origin.x - 233.0).abs() < 1.0,
        "the dot starts at {}",
        dot.origin.x
    );

    // At 5% the envelope is flat: the bar keeps its height and paints a straight line.
    let flat = build_wavy(&mut AnimStore::new(), 0.05);
    let (low, high, _, _) = wave_span(&flat);
    assert!(high - low < 1.0, "flat at 5% still spans {}", high - low);
    assert_eq!(flat.size_class.height, AxisSize::fixed(10.0));
}

#[test]
fn the_active_bar_is_the_value_fraction_of_the_width_and_vanishes_at_zero() {
    // The value is the app's number, so the bar must be exactly that share of the width, and
    // a zero value must paint no nub — the slider's zero-length rule, reused.
    let mut store = AnimStore::new();
    let quarter = active_of(&build(&mut store, 240.0, 0.25)).expect("a quarter bar paints");
    assert!(
        (quarter.size.w - 60.0).abs() < 0.5,
        "got width {}",
        quarter.size.w
    );
    assert_eq!(
        (quarter.origin.x, quarter.origin.y),
        (0.0, 0.0),
        "it starts at the track's left"
    );
    let empty = build(&mut store, 240.0, 0.0);
    let painted = active_of(&empty).map_or(0.0, |r| r.size.w);
    assert_eq!(painted, 0.0, "zero value shows no visible bar");
}
