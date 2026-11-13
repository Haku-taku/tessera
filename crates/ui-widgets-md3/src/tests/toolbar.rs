//! The MD3 toolbar's trailing FAB: the size and theme colours its own table names.

use super::*;

use crate::{Md3Toolbar, ToolbarAction, ToolbarFab, ToolbarParams, ToolbarTheme, ToolbarVariant};
use ui_core::cx::Routing;
use ui_core::draw::Shape;
use ui_core::layout::AxisSize;

const BAR: Id = Id::str("toolbar");
const FAB: Id = Id::str("toolbar/fab");

/// A 24dp mark in the ink the control hands it: the stand-in for a real glyph.
fn dot(id: Id, ink: Color) -> Decl {
    Decl::container(id, NodeKind::Box)
        .size(SizeClass::fixed(24.0, 24.0))
        .shapes(vec![Shape::Box {
            corners: ui_core::Corners::ZERO,
            fill: ink,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        }])
}

fn build(store: &mut AnimStore, theme: ToolbarTheme, medium: bool) -> Decl {
    let mut core = ui_core::cx::Cx::new(store, 0, 600, 64, 0.0, 0.0, 1.0, None);
    core.route(Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let actions = [ToolbarAction::new(
        Id::str("toolbar/member"),
        ui_widgets::IconSource::Custom(dot),
    )];
    let mut fab = ToolbarFab::new(FAB, ui_widgets::IconSource::Custom(dot));
    if medium {
        fab = fab.medium();
    }
    let bar = Md3Toolbar::new_with(
        Md3Options::default(),
        ToolbarParams {
            id: BAR,
            variant: ToolbarVariant::Floating,
            theme,
            actions: &actions,
            fab: Some(fab),
            disabled: false,
        },
        &mut cx,
        crate::pack::pack(),
    );
    cx.with_cx(|cx| bar.decl(cx))
}

#[test]
fn a_floating_toolbars_fab_reads_its_size_and_theme_from_its_own_table() {
    // The FAB's measurements and role names live in a subtree the bar's table excludes;
    // reading the bar's container role here would paint the FAB in the bar's colours.
    let pack = crate::pack::pack();
    let regular = build(&mut AnimStore::new(), ToolbarTheme::Standard, false);
    let fab = regular
        .children
        .iter()
        .find(|child| child.id == FAB)
        .expect("the fab is the bar's last child");
    assert_eq!(fab.size_class.width, AxisSize::fixed(56.0));
    assert_eq!(fab.size_class.height, AxisSize::fixed(56.0));
    assert_eq!(
        fab.fill(),
        Some(crate::paint::spec_ink(pack, "secondaryContainer", 1.0))
    );

    let medium = build(&mut AnimStore::new(), ToolbarTheme::Vibrant, true);
    let fab = medium
        .children
        .iter()
        .find(|child| child.id == FAB)
        .expect("the fab is the bar's last child");
    assert_eq!(fab.size_class.width, AxisSize::fixed(80.0));
    assert_eq!(
        fab.fill(),
        Some(crate::paint::spec_ink(pack, "tertiaryContainer", 1.0))
    );
}
