//! Geometry invariants of the two interactive glass controls.

use crate::*;

#[test]
fn the_melt_grows_the_pill_about_a_pinned_centre_and_stays_a_pill() {
    // The reference cross-fades two views about one centre, so the melt must never shift the
    // knob — not even where the spring overshoots past either end of 0..=1.
    let centre = |b: SwitchThumb| (b.x + b.w * 0.5, b.y + b.h * 0.5);
    for pos in [0.0f32, 0.5, 1.0] {
        let want = INSET + THUMB_W * 0.5 + travel() * pos;
        for t in [-0.05f32, 0.0, 0.25, 0.5, 1.0, 1.1] {
            let b = switch_thumb(pos, t, NO_JELLY);
            let (cx, cy) = centre(b);
            assert!((cx - want).abs() < 1e-3, "centre {cx} at pos {pos} t {t}");
            assert!((cy - (INSET + THUMB_H * 0.5)).abs() < 1e-3, "centre y {cy}");
            assert!((b.corner - b.h * 0.5).abs() < 1e-3, "still a pill at t {t}");
            // Both axes interpolate between the two reference sizes, so the melt only ever
            // passes between them (and the overshoot only just past the far one).
            let w = THUMB_W + (EXPANDED_W - THUMB_W) * t;
            let h = THUMB_H + (EXPANDED_H - THUMB_H) * t;
            assert!(
                (b.w - w).abs() < 1e-3 && (b.h - h).abs() < 1e-3,
                "{}x{}",
                b.w,
                b.h
            );
        }
    }
    // Sweeping the melt must never step the centre: a state-flipped anchor did exactly that.
    let mut last = centre(switch_thumb(0.7, 0.0, NO_JELLY));
    for k in 1..=100 {
        let now = centre(switch_thumb(0.7, k as f32 / 100.0, NO_JELLY));
        assert!(
            (now.0 - last.0).abs() < 0.05 && (now.1 - last.1).abs() < 0.05,
            "step {k}"
        );
        last = now;
    }
}

#[test]
fn a_pull_past_the_end_is_square_rooted_and_drags_the_track_along() {
    // 20.3 dp past the maximum reads as sqrt(20.3) = 4.506 dp of overshoot (`:771-778`), and
    // the track's far edge follows that overshoot exactly (`:371-386`).
    // (`:371-386`).
    let pull = 20.3;
    let v = rubber_band(1.0 + pull / slider_travel());
    let offset = rubber_band_offset(v);
    assert!((offset - pull.sqrt()).abs() < 1e-3, "offset {offset}");
    assert!(v > 1.0, "the thumb leaves the rest range");
    let rest = slider_thumb(1.0, 0.0, NO_JELLY);
    let pulled = slider_thumb(v, 0.0, NO_JELLY);
    assert!(
        (pulled.x - rest.x - offset).abs() < 1e-3,
        "the lens rides the overshoot"
    );
    let track = track_rect(offset);
    assert!(
        (track.size.h - (SLIDER_TRACK_H - offset / 3.0)).abs() < 1e-4,
        "it thins"
    );
    assert!(
        (track.origin.x + track.size.w - (SLIDER_WIDTH + offset)).abs() < 1e-4,
        "far edge {} vs {}",
        track.origin.x + track.size.w,
        SLIDER_WIDTH + offset
    );
    // The pull is elastic but bounded: ten times the pull is well under ten times the travel.
    let far = rubber_band_offset(rubber_band(1.0 + pull * 10.0 / slider_travel()));
    assert!(far < pull.sqrt() * 3.2, "far {far}");
    // The 2 dp floor holds however hard it is pulled.
    let thin = track_rect(rubber_band_offset(rubber_band(
        1.0 + 1000.0 / slider_travel(),
    )));
    assert!((thin.size.h - 2.0).abs() < 1e-4, "h {}", thin.size.h);
}

#[test]
fn the_fill_vanishes_at_the_minimum_and_swallows_the_track_at_the_maximum() {
    // The reference fades the fill over 10 dp at the minimum end and lets it take the whole
    // track within 10 dp of the maximum (`:511-529`); the middle half fills to the lens.
    let at = |v: f32| fill_rect(0.0, thumb_centre(v));
    assert!(at(0.0).size.w.abs() < 1e-4, "w {}", at(0.0).size.w);
    assert!(
        (at(1.0).size.w - SLIDER_WIDTH).abs() < 1e-4,
        "w {}",
        at(1.0).size.w
    );
    assert!((at(0.5).size.w - thumb_centre(0.5)).abs() < 1e-4);
    // Halfway through each fade the fill is half weighted, never a step.
    let min_half = at(0.5 * 10.0 / slider_travel()).size.w;
    assert!(min_half > 0.0 && min_half < at(0.5 * 10.0 / slider_travel() + 0.01).size.w);
}

#[test]
fn the_morph_neck_peaks_where_the_travel_and_size_curves_part() {
    // The metaball bridge is the gap between the two curves: zero at both ends.
    let at = |raw| morph::compute(raw, 160.0, 0.0, 1.0, false);
    assert!(at(0.0).blend.abs() < 1e-4);
    assert!(at(1.0).blend.abs() < 1e-4);
    let mid = at(0.5);
    assert!(mid.blend > 1.0, "blend {} at half flight", mid.blend);
    assert!(mid.path_t > mid.size_t, "travel must lead size");
}

#[test]
fn closing_ramps_the_neck_back_up_as_the_droplet_returns() {
    // On close the bridge is not the curve gap: it fires by proximity, so the droplet
    // re-merges with the regrown trigger instead of landing as a separate shape.
    let at = |raw| morph::compute(raw, 160.0, 0.0, 1.0, true);
    assert!(at(1.0).blend.abs() < 1e-4);
    assert!(at(0.3).blend > 10.0, "blend {} on return", at(0.3).blend);
    assert!(
        at(0.3).anchor_scale > 0.5,
        "the trigger must be back to catch it"
    );
}

#[test]
fn jelly_squashes_along_the_motion_and_stretches_across_it() {
    // At the thumbs' tuning 4 units/s is already full distortion (0.6): 0.7 across, 1.18 along.
    assert_eq!(jelly_scale((0.0, 0.0), 2.0, 0.6), NO_JELLY);
    let (sx, sy) = jelly_scale((4.0, 0.0), 2.0, 0.6);
    assert!(
        (sx - 0.7).abs() < 1e-6 && (sy - 1.18).abs() < 1e-6,
        "{sx} {sy}"
    );
    // Straight down swaps the axes; a diagonal is symmetric between them.
    let (dx, dy) = jelly_scale((0.0, -4.0), 2.0, 0.6);
    assert!(
        (dx - 1.18).abs() < 1e-6 && (dy - 0.7).abs() < 1e-6,
        "{dx} {dy}"
    );
    let (qx, qy) = jelly_scale((4.0, 4.0), 2.0, 0.6);
    let axis = 0.6 / 2f32.sqrt();
    assert!(
        (qx - (1.0 - axis * 0.5) * (1.0 + axis * 0.3)).abs() < 1e-6,
        "{qx}"
    );
    assert!((qy - qx).abs() < 1e-6, "{qx} {qy}");
    // Past full distortion the clamp holds, and a broken velocity reads as rest.
    assert_eq!(
        jelly_scale((99.0, 0.0), 2.0, 0.6),
        jelly_scale((4.0, 0.0), 2.0, 0.6)
    );
    assert_eq!(jelly_scale((f32::NAN, 0.0), 2.0, 0.6), NO_JELLY);
}

#[test]
fn a_glass_formula_takes_the_packs_body_ink_and_size() {
    // A host with no math face draws the source text; the ink and size it draws with are the
    // pack's, so a role name the pack does not know shows up here rather than on a screen.
    let pack = pack();
    let mut store = ui_core::AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let decl = cx.with_cx(|cx| GlassMath::new(ui_core::Id::str("m"), "x^2", pack).resolve_in(cx));
    let ui_core::NodeData::Text { ink, block, .. } = &decl.data else {
        panic!("a host with no math face draws the source text");
    };
    assert_eq!(
        *ink,
        pack.ink("textFillColorPrimary")
            .expect("the pack knows its primary text ink")
    );
    assert_eq!(
        block.runs[0].style.size,
        pack.type_table.get(ui_style::TextRole::Body).size
    );
}
