// Self-test checks for the carousel page.

use crate::selftest::*;

/// The engine's offset for sample `k`'s own strip.
fn strip_offset(lp: &AppLoop<Gallery>, k: usize) -> f32 {
    let strip = ui_widgets_md3::carousel_strip_id(carousel_id(k));
    let at = lp.scroll_offset(strip).unwrap_or_default();
    if CAROUSEL_SAMPLES[k].kind.vertical() {
        at.y
    } else {
        at.x
    }
}

pub(crate) fn carousel(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // The Carousel page: every layout puts its leading item after the 16dp gutter, the
    // weighted ones take the widths their ratios ask for, and each strip pages on its own.
    goto(lp, size, Page::Carousel);
    let at_rest: Vec<f32> = (0..3)
        .map(|j| {
            rect_of(lp, carousel_item_id(0, j))
                .map(|r| r.size.w)
                .unwrap_or(-1.0)
        })
        .collect();
    let widths_ok = (at_rest[0] - 156.0).abs() < 0.5
        && (at_rest[1] - 104.0).abs() < 0.5
        && (at_rest[2] - 52.0).abs() < 0.5;
    let gutter_ok = match (
        rect_of(lp, carousel_id(0)),
        rect_of(lp, carousel_item_id(0, 0)),
    ) {
        (Some(panel), Some(first)) => ((first.origin.x - panel.origin.x) - 16.0).abs() < 0.5,
        _ => false,
    };
    // A page that cannot fit the strip squeezes it, and the mask then cuts the item short;
    // the samples must fit so the last one is drawn whole.
    let whole = lp
        .tree()
        .find_by_id(carousel_item_id(4, 0))
        .and_then(|nid| lp.tree().get(nid))
        .map(|n| {
            (n.mask.size.h - n.rect.size.h).abs() < 0.5
                && (n.mask.size.w - n.rect.size.w).abs() < 0.5
        })
        .unwrap_or(false);

    // One press of sample 0's next button pages that strip by one page and no other.
    let others: Vec<f32> = (1..CAROUSEL_SAMPLES.len())
        .map(|k| strip_offset(lp, k))
        .collect();
    let mut scrolled = 0.0;
    if let Some(next) = rect_of(lp, carousel_next_id(0)) {
        click(lp, size, center(next));
        for _ in 0..4 {
            frame(lp, size, None);
        }
        scrolled = strip_offset(lp, 0);
    }
    let others_still = (1..CAROUSEL_SAMPLES.len()).all(|k| strip_offset(lp, k) == others[k - 1]);
    // One page retires the item that led, and the next one takes the gutter.
    let retired = rect_of(lp, carousel_item_id(0, 0)).is_none();
    let narrowed = match (
        rect_of(lp, carousel_id(0)),
        rect_of(lp, carousel_item_id(0, 1)),
    ) {
        (Some(panel), Some(next)) => ((next.origin.x - panel.origin.x) - 16.0).abs() < 0.5,
        _ => false,
    };
    let mut named = false;
    if let Some(item) = rect_of(lp, carousel_item_id(1, 1)) {
        click(lp, size, center(item));
        named = lp.app_mut().carousel_fired == Some((1, 1));
    }
    // The engine eases the strip to the next page: two 60 Hz frames leave it in flight,
    // short of the page it is heading for.
    let mut eased = false;
    if let Some(next) = rect_of(lp, carousel_next_id(0)) {
        let at = center(next);
        let button = ui_core::PointerButton::Primary;
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        frame(lp, size, Some(at));
        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        frame(lp, size, Some(at));
        lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
        // The release's frame only starts the settle; the one after it has moved.
        let _ = lp.build_with_dt(size, Some(at), 1.0 / 60.0);
        let _ = lp.build_with_dt(size, Some(at), 1.0 / 60.0);
        let in_flight = strip_offset(lp, 0);
        for _ in 0..4 {
            frame(lp, size, Some(at));
        }
        eased = in_flight > 156.0 && in_flight < 312.0 && (strip_offset(lp, 0) - 312.0).abs() < 0.5;
    }
    // A vertical strip follows the pointer's y travel: an upward drag scrolls it forward.
    let mut vertical = false;
    if let Some(panel) = rect_of(lp, carousel_id(4)) {
        let at = center(panel);
        // The pan begins where the threshold is crossed, so that move's travel is not the
        // content's: the second move, 100dp further on, is the one that counts.
        let to = ui_core::geom::Point::new(at.x, at.y - 110.0);
        let before = strip_offset(lp, 4);
        let button = ui_core::PointerButton::Primary;
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        frame(lp, size, Some(at));
        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        frame(lp, size, Some(at));
        // The first move past the threshold only starts the drag; the travel arrives on the
        // next one, as a real pointer sends many moves.
        let start = ui_core::geom::Point::new(at.x, at.y - 10.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: start });
        frame(lp, size, Some(start));
        lp.dispatch(ui_core::Event::PointerMoved { pos: to });
        frame(lp, size, Some(to));
        // Read before the release: letting go hands the speed to a fling, which carries the
        // strip past the travel the pointer asked for.
        let dragged = strip_offset(lp, 4);
        lp.dispatch(ui_core::Event::PointerReleased { pos: to, button });
        frame(lp, size, Some(to));
        vertical = (dragged - before - 100.0).abs() < 0.5;
    }
    let carousel_ok = widths_ok
        && gutter_ok
        && (scrolled - 156.0).abs() < 0.5
        && others_still
        && retired
        && narrowed
        && named
        && eased
        && whole
        && vertical;
    println!(
        "selftest: md3 carousel widths={at_rest:?} at-ratios={widths_ok} gutter={gutter_ok} \
         scrolled={scrolled:.0} others-still={others_still} retired={retired} \
         narrowed={narrowed} item={named} eased={eased} whole={whole} vertical={vertical} {}",
        if carousel_ok { "✓" } else { "✗" }
    );
    if !carousel_ok {
        pass = false;
    }
    pass
}
