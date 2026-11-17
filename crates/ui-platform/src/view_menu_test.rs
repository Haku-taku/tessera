//! Engine test: a menu's panel belongs to the engine, so the platform taking its surface
//! back has to close the menu — there is no app decision to hand over.

use ui_core::{Decl, Id, NodeKind, SizeClass, geom, menu::MenuItem};

use crate::{App, AppLoop, OffscreenTarget};

/// An app with one node, so the menu has an owner to report to.
struct Bare;

impl App for Bare {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        crate::from_decl(
            Decl::container(Id::str("root"), NodeKind::Box).size(SizeClass::fixed(100.0, 100.0)),
        )
    }
}

/// A press that opens a menu leaves the button held by a background thread; the system ends
/// that grab the moment the panel opens, and Win32 reports the release as a dismissal. The
/// panel is the engine's declaration, so only the engine can close what it opened: routing
/// the dismissal to the app left a menu that could never be closed again.
#[test]
fn a_menu_panel_the_platform_took_back_closes_the_menu() {
    let mut app_loop = AppLoop::new(Bare);
    let mut target = OffscreenTarget::new(200, 200);
    app_loop.frame(&mut target, None);

    let opened = app_loop.open_menu_at(
        Id::str("root"),
        geom::Point::new(20.0, 20.0),
        vec![MenuItem::new(Id::str("row"), "Row")],
    );
    assert!(opened, "the menu opens");
    app_loop.frame(&mut target, None);
    let panel_id = crate::menu::Menu::panel_id(0);
    let node = app_loop
        .tree()
        .find_by_id(panel_id)
        .expect("the menu declares its panel");

    assert!(app_loop.dismiss_panel(node), "the open panel is dismissed");
    app_loop.frame(&mut target, None);
    assert!(
        app_loop.tree().find_by_id(panel_id).is_none(),
        "the taken-back panel closes the menu"
    );
}
