// Self-test checks for the cards page.

use crate::selftest::*;

pub(crate) fn cards(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // Cards: all three variants report activation, a static one does not.
    goto(lp, size, Page::Cards);
    let before = lp.app_mut().card_hits;
    for tag in CARD_VARIANTS {
        click(lp, size, center(rect_of(lp, card_id(tag)).expect("card")));
    }
    click(
        lp,
        size,
        center(rect_of(lp, card_id("static")).expect("static card")),
    );
    let after = lp.app_mut().card_hits;
    let cards_ok = after.iter().zip(before.iter()).all(|(a, b)| *a == *b + 1);
    println!(
        "selftest: card activations {before:?} -> {after:?} (static must not count) {}",
        if cards_ok { "✓" } else { "✗" }
    );
    if !cards_ok {
        pass = false;
    }

    // ── The card owns the surface, never the content's ink: the page must take it from the
    // pack, and a disabled card dims it to MD3's 38 % content opacity.
    let pack = ui_widgets_md3::pack_of(appearance_of(lp.app_mut().dark));
    let want = pack.ink("onSurface").expect("the MD3 pack knows onSurface");
    let dimmed = want.faded(ui_widgets_md3::DISABLED_CONTENT_OPACITY);
    let mut ink_ok = 0usize;
    for tag in CARD_VARIANTS {
        if text_ink(lp, card_id(tag).with("title")) == Some(want) {
            ink_ok += 1;
        } else {
            println!("selftest: card {tag} title ink is not onSurface");
        }
    }
    let disabled_ok = text_ink(lp, card_id("elevated-off").with("title")) == Some(dimmed);
    println!(
        "selftest: card title ink {ink_ok}/3 onSurface {}, disabled at 38 % {}",
        if ink_ok == CARD_VARIANTS.len() {
            "✓"
        } else {
            "✗"
        },
        if disabled_ok { "✓" } else { "✗" },
    );
    pass &= ink_ok == CARD_VARIANTS.len() && disabled_ok;

    // ── Glass containers: the card insets its content by 16 and hugs it; the plain
    // container insets nothing and keeps the height it is given.
    let mark = |ok: bool| if ok { "✓" } else { "✗" };
    let card = rect_of(lp, glass_card_id("card")).expect("glass card");
    let content = rect_of(lp, glass_card_id("card").with("content")).expect("glass card content");
    let title = rect_of(lp, glass_card_id("card").with("title")).expect("glass card title");
    let plain = rect_of(lp, glass_card_id("plain")).expect("plain container");
    let plain_content = rect_of(lp, glass_card_id("plain").with("content")).expect("plain content");
    let inset = (title.origin.x - card.origin.x - 16.0).abs() < 0.5
        && (title.origin.y - card.origin.y - 16.0).abs() < 0.5;
    let hug = (card.size.h - content.size.h - 32.0).abs() < 0.5;
    let flush =
        (plain_content.origin.x - plain.origin.x).abs() < 0.5 && (plain.size.h - 56.0).abs() < 0.5;
    println!(
        "selftest: glass card inset {inset} {}, hugs {:.1}={:.1}+32 {}, plain flush {flush} (dx {:.1}, h {:.1}) {}",
        mark(inset),
        card.size.h,
        content.size.h,
        mark(hug),
        plain_content.origin.x - plain.origin.x,
        plain.size.h,
        mark(flush)
    );
    pass &= inset && hug && flush;
    pass
}
