// Self-test checks for the chips page.

use crate::selftest::*;

pub(crate) fn chips(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // Chips: one height for all four kinds, and a filter chip whose state the app owns.
    goto(lp, size, Page::Chips);
    let heights: Vec<f32> = ["Assist", "Suggestion", "Filter", "Input"]
        .iter()
        .map(|label| {
            rect_of(lp, Id::of(("chip", Page::Chips.tag(), "flat", *label)))
                .map(|r| r.size.h)
                .unwrap_or(-1.0)
        })
        .collect();
    let heights_ok = heights.iter().all(|h| (h - 32.0).abs() < 0.5);
    let before = lp.app_mut().chip_sel[0];
    click(
        lp,
        size,
        center(rect_of(lp, chip_filter_id(0)).expect("filter chip")),
    );
    let after = lp.app_mut().chip_sel[0];
    println!(
        "selftest: md3 chip heights {heights:?} (want 32) {}, filter {before}->{after} {}",
        if heights_ok { "✓" } else { "✗" },
        if after != before { "✓" } else { "✗" }
    );
    if !(heights_ok && after != before) {
        pass = false;
    }

    // ── Glass chips: the capsule hugs its content (label + 2×8), the body toggles, and the
    // delete glyph reports from its own node and takes the chip out of the tree.
    let mark = |ok: bool| if ok { "✓" } else { "✗" };
    let plain = rect_of(lp, chip_glass_id(0)).expect("glass chip");
    let plain_label = rect_of(lp, chip_glass_id(0).with("label")).expect("glass chip label");
    let marked = Id::of(("chipglass", Page::Chips.tag(), "star"));
    let with_icon = rect_of(lp, marked).expect("glass icon chip");
    let icon = rect_of(lp, marked.with("icon")).expect("glass chip mark");
    let icon_label = rect_of(lp, marked.with("label")).expect("glass icon chip label");
    // The pad leads the label; a mark pushes the label out by its own box and the gap.
    let leading = (plain_label.origin.x - plain.origin.x - 12.0).abs() < 0.5
        && (icon.origin.x - with_icon.origin.x - 12.0).abs() < 0.5
        && (icon_label.origin.x - with_icon.origin.x - 34.0).abs() < 0.5;
    let hugs = (plain.size.h - plain_label.size.h - 16.0).abs() < 0.5;
    let was = lp.app_mut().chip_glass[0];
    click(lp, size, center(plain));
    let toggled = lp.app_mut().chip_glass[0] != was;
    let removable = chip_glass_removable_id();
    let delete = rect_of(lp, removable.with("delete")).expect("glass delete box");
    click(lp, size, center(delete));
    let gone = lp.app_mut().chip_glass_gone && rect_of(lp, removable).is_none();
    println!(
        "selftest: glass chip leading {leading} {}, hugs label {hugs} {}, toggles {toggled} {}, delete removes {gone} {}",
        mark(leading),
        mark(hugs),
        mark(toggled),
        mark(gone)
    );
    pass &= leading && hugs && toggled && gone;
    pass
}
