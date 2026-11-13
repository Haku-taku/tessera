//! The common plane reaching the generic widgets, which no family crate owns.

use super::*;

// A generic button under an MD3 pack has no `buttonFill` role; the plane supplies the accent.
#[test]
fn a_generic_button_under_a_kit_pack_paints_the_kits_accent() {
    let pack = crate::pack::pack();
    let decl = ui_widgets::Button::new(Id::str("b"), "go").resolve(
        ui_widgets::WidgetState::default(),
        &pack,
        0.0,
    );
    let accent = pack
        .colour(ui_design::CommonColor::Accent)
        .value()
        .expect("accent")
        .value;
    assert_eq!(decl.fill(), Some(accent));
}

// The same for text: a scroller without its own roles reads `OnSurface`, not white.
#[test]
fn a_generic_list_under_a_kit_pack_reads_the_surface_roles() {
    let pack = crate::pack::pack();
    let list = ui_widgets::List {
        id: Id::str("list"),
        width: 200.0,
        height: 100.0,
        row_height: 20.0,
        rows: vec!["one".into()],
        pack: &pack,
    };
    let decl = list.resolve();
    let surface = pack
        .colour(ui_design::CommonColor::Surface)
        .value()
        .expect("surface")
        .value;
    assert_eq!(decl.fill(), Some(surface));
}

// A card under a kit pack has no `cardFill`; the plane's raised surface fills it.
#[test]
fn a_generic_card_under_a_kit_pack_reads_the_raised_surface() {
    let pack = crate::pack::pack();
    let card = ui_widgets::Card {
        id: Id::str("card"),
        title: Some("title".into()),
        title_ink: None,
        pack: &pack,
    };
    let decl = card.resolve(Vec::new());
    let raised = pack
        .colour(ui_design::CommonColor::SurfaceRaised)
        .value()
        .expect("raised surface")
        .value;
    assert_eq!(decl.fill(), Some(raised));
}
