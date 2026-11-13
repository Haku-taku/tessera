//! The MD3 menu: the surface's insets and the fill a selected row carries.

use super::*;

use crate::menu::{Md3Menu, Md3MenuEntry, Md3MenuItem, Md3MenuParams};
use crate::spec::{MenusFlags, menus_spec};
use ui_core::draw::Shape;
use ui_core::geom::{Corners, Insets};
use ui_core::layout::AxisSize;

const MENU: Id = Id::str("menu/test");
const CHOSEN: Id = Id::str("menu/test/chosen");
const PLAIN: Id = Id::str("menu/test/plain");

fn build(entries: Vec<Md3MenuEntry>) -> Decl {
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 800, 600, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control = Md3Menu::new_with(
        Md3MenuParams::new(MENU, entries),
        &mut cx,
        crate::pack::pack(),
    );
    cx.with_cx(|cx| control.decl(cx))
}

#[test]
fn a_disabled_selected_row_keeps_its_selection_and_fades_it() {
    // The disabled state must not fall back to `onSurface`: the selected pair is applied
    // after it and then faded, which is what the export's `disabled && selected` rows say.
    let plain = menus_spec(MenusFlags {
        disabled: true,
        ..MenusFlags::NONE
    });
    assert_eq!(plain.menu_item_label_text.color, "onSurface");
    assert_eq!(plain.menu_item_label_text.opacity, 0.38);

    let chosen = menus_spec(MenusFlags {
        selected: true,
        disabled: true,
        ..MenusFlags::NONE
    });
    assert_eq!(chosen.menu_item_container.color, "tertiaryContainer");
    assert_eq!(chosen.menu_item_container.opacity, 0.38);
    assert_eq!(chosen.menu_item_label_text.color, "onTertiaryContainer");
    assert_eq!(chosen.menu_item_label_text.opacity, 0.38);

    let hovered = menus_spec(MenusFlags {
        selected: true,
        hovered: true,
        ..MenusFlags::NONE
    });
    assert_eq!(hovered.menu_item_state_layer.color, "onTertiaryContainer");
    assert_eq!(hovered.menu_item_state_layer.opacity, 0.08);
}

#[test]
fn the_surface_insets_its_rows_and_only_a_selected_row_fills() {
    let decl = build(vec![
        Md3MenuEntry::Item(Md3MenuItem::new(CHOSEN, "Paste").selected(true)),
        Md3MenuEntry::Item(Md3MenuItem::new(PLAIN, "Copy")),
        Md3MenuEntry::Divider,
    ]);
    let params = decl.data.params().expect("a column");
    assert_eq!(params.gap, 2.0);
    assert_eq!(decl.box_model.padding, Insets::all(2.0));
    assert_eq!(params.corners, Some(Corners::all(4.0)));
    // The surface paints one box; every row paints its own fill over it.
    assert_eq!(decl.shapes.len(), 1);

    let rows = &decl.children;
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows[0].box_model.padding,
        Insets {
            left: 16.0,
            right: 16.0,
            top: 8.0,
            bottom: 8.0,
        }
    );
    assert_eq!(rows[0].size_class.height.min, 44.0);
    let fill = match rows[0].shapes.first() {
        Some(Shape::Box { fill, .. }) => *fill,
        other => panic!("a selected row paints a fill, got {other:?}"),
    };
    assert_eq!(
        fill,
        crate::paint::role(crate::pack::pack(), "tertiaryContainer")
    );
    assert!(
        rows[1].shapes.is_empty(),
        "an unselected resting row paints no fill"
    );

    // A divider is a 1 dp line inset by the row's padding, not a command row.
    assert_eq!(
        rows[2].box_model.padding,
        Insets {
            left: 16.0,
            right: 16.0,
            top: 0.0,
            bottom: 0.0,
        }
    );
    assert_eq!(rows[2].children[0].size_class.height, AxisSize::fixed(1.0));
}
