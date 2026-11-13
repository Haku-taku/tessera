//! The MD3 scaffold: six regions, their sizes and their independence.

use super::*;

#[test]
fn the_scaffold_decl_is_reentrant() {
    let content = Square {
        id: Id::str("content"),
        side: 40.0,
    };
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let spec = ScaffoldSpec::main_only(Id::str("root"), &content);
    let control = Md3Kit::new().scaffold(&spec, &mut cx);
    assert_eq!(
        ui_widgets::Cx::for_test(|cx| control.decl(cx)),
        ui_widgets::Cx::for_test(|cx| control.decl(cx)),
        "decl() must be reentrant"
    );
}
