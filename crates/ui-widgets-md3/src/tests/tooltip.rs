//! The MD3 tooltips: the plain inverse-surface panel and the rich one, their anchors,
//! and the roles their tables name.

use super::*;

use crate::tooltip::{
    Md3RichToolTip, Md3ToolTip, RichToolTipAction, RichToolTipParams, ToolTipParams,
};
use ui_core::layout::{Anchor, OverlayAlign, Side};

#[test]
fn a_plain_tooltip_is_a_flipped_overlay_of_inverse_roles() {
    // Swapping the panel and the label roles is invisible to the compiler and leaves an
    // unreadable tooltip; the table file is what decides, so this reads the panel's decl.
    let pack = crate::pack::pack();
    let anchor = Anchor::Node(Id::str("save"));
    let tip = Md3ToolTip::new(
        ToolTipParams::new(Id::str("tip"), "Save the file", anchor),
        pack,
    );
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 200, 200, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let decl = cx.with_cx(|cx| tip.decl(cx));

    let overlay = decl.placement.overlay.expect("a tooltip is an overlay");
    assert_eq!(overlay.anchor, anchor);
    assert_eq!(overlay.side, Side::Above);
    assert_eq!(overlay.align, OverlayAlign::Center);
    assert!(
        overlay.flip,
        "the engine flips it when there is no room above"
    );
    assert_eq!(
        decl.fill(),
        Some(crate::paint::spec_ink(pack, "inverseSurface", 1.0))
    );
    let label = decl.children.first().expect("the label");
    let ui_core::NodeData::Text { ink, .. } = &label.data else {
        panic!("the panel paints text, not {:?}", label.kind);
    };
    assert_eq!(*ink, crate::paint::spec_ink(pack, "inverseOnSurface", 1.0));
}

#[test]
fn a_rich_tooltip_reads_its_container_and_bands_from_its_own_table() {
    // The rich surface shares the plain tooltip's shape but not its roles: reading the
    // inverse pair here would leave an unreadable panel, so the table decides.
    let pack = crate::pack::pack();
    let anchor = Anchor::Node(Id::str("info"));
    let actions = [RichToolTipAction::new(Id::str("tip/dismiss"), "Dismiss")];
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 300, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let tip = Md3RichToolTip::new_with(
        Md3Options::default(),
        RichToolTipParams::new(Id::str("rich"), "Body copy", anchor)
            .subhead("Subhead")
            .actions(&actions),
        &mut cx,
        pack,
    );
    let decl = cx.with_cx(|cx| tip.decl(cx));

    let overlay = decl
        .placement
        .overlay
        .expect("a rich tooltip is an overlay");
    assert_eq!(overlay.anchor, anchor);
    assert_eq!(overlay.side, Side::Above);
    assert!(
        overlay.flip,
        "the engine flips it when there is no room above"
    );
    assert_eq!(
        decl.fill(),
        Some(crate::paint::spec_ink(pack, "surfaceContainer", 1.0))
    );
    let band_ink = |band: usize| {
        let node = &decl.children[band].children[0];
        let ui_core::NodeData::Text { ink, .. } = &node.data else {
            panic!("band {band} paints text, not {:?}", node.kind);
        };
        *ink
    };
    let body = crate::paint::spec_ink(pack, "onSurfaceVariant", 1.0);
    assert_eq!(band_ink(0), body, "the subhead reads this table's role");
    assert_eq!(band_ink(1), body, "the supporting text reads the same role");
    let row = &decl.children[2].children[0];
    assert_eq!(row.id, Id::str("rich").with("action-row"));
    assert!(
        row.children
            .iter()
            .any(|child| child.id == Id::str("tip/dismiss")),
        "the action stays a button the app named"
    );
}
