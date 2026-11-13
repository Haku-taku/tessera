//! The MD3 formula: the pack's body ink and size, laid out by the host's math face.

use super::*;

/// A host with no math face draws the formula's source — in the pack's `onSurface` ink and
/// body-large size. A role name the pack does not know shows up here, not on a screen.
#[test]
fn an_md3_formula_takes_the_packs_body_ink_and_size() {
    let pack = crate::pack::pack();
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let decl = cx.with_cx(|cx| Md3Math::new(Id::str("m"), "x^2", pack).resolve_in(cx));
    let ui_core::NodeData::Text { ink, block, .. } = &decl.data else {
        panic!("a host with no math face draws the source text");
    };
    assert_eq!(
        *ink,
        pack.ink("onSurface").expect("the pack knows onSurface")
    );
    assert_eq!(block.runs[0].style.size, typescale::BODY_LARGE.size);
}
