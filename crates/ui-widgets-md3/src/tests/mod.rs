//! Shared fixtures for the pack's tests, plus one module per subject.

use crate::*;

use ui_core::geom::Color;
use ui_core::{
    AnimStore, BuiltinLayout, Constraints, Decl, Id, LayoutEngine, MonospaceMeasure, NodeKind,
    SizeClass, Tree,
};
use ui_widgets::{ScaffoldSpec, StyleKit, Widget};

mod button;
mod checkbox;
mod common_plane;
mod date_picker;
mod date_range_picker;
mod dialog;
mod fab;
mod icon_button;
mod list;
mod math;
mod menu;
mod nav_bar;
mod nav_drawer;
mod nav_rail;
mod progress;
mod radio;
mod scaffold;
mod scope;
mod search_view;
mod side_sheet;
mod slider;
mod snackbar;
mod spec;
mod split_button;
mod tab_bar;
mod text_field;
mod theme;
mod time_picker;
mod toggle;
mod token_parity;
mod toolbar;
mod tooltip;

/// The track *fill*: the 52×32 box. An outlined switch paints a 2pt wider ring before it, so
/// the fill is not always the first shape, but it is always the first 52×32 rect.
/// The fill colour of a rect shape (transparent for non-rects).
fn shape_fill(shape: &ui_core::draw::Shape) -> Color {
    match shape {
        ui_core::draw::Shape::Rect { fill, .. } => *fill,
        _ => Color::TRANSPARENT,
    }
}

fn track_shape(decl: &Decl) -> &ui_core::draw::Shape {
    decl.shapes
        .iter()
        .find(|s| match s {
            ui_core::draw::Shape::Rect { rect, .. } => {
                (rect.size.w - crate::toggle::TRACK.0).abs() < 0.01
                    && (rect.size.h - crate::toggle::TRACK.1).abs() < 0.01
            }
            _ => false,
        })
        .expect("the track fill is the 52x32 rect")
}

/// Rest sizes, as if nothing was ever pressed.
/// A content widget that knows nothing but its own fixed square.
struct Square {
    id: Id,
    side: f32,
}

impl Widget for Square {
    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        Decl::container(self.id, NodeKind::Box).size(SizeClass::fixed(self.side, self.side))
    }
}

fn lay_out(decl: &Decl, size: (f32, f32)) -> Tree {
    let mut tree = Tree::new();
    tree.apply(decl);
    let root = tree.root().expect("root node");
    let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
    engine.solve(
        &mut tree,
        root,
        Constraints::loose(ui_core::Size::new(size.0, size.1)),
    );
    tree
}

/// Lay `decl` out as the only child of a 200x100 column: a root node is forced to the
/// constraint, so a size class is only honoured one level down.
fn laid_out(decl: Decl) -> Tree {
    let root = Decl::container(Id::str("root"), NodeKind::Column).child(decl);
    lay_out(&root, (200.0, 100.0))
}

fn region_rect(tree: &Tree, id: Id) -> (f32, f32, f32, f32) {
    let nid = tree
        .find_by_id(id)
        .unwrap_or_else(|| panic!("missing {id:?}"));
    let r = tree.get(nid).expect("node").visual_rect();
    (r.origin.x, r.origin.y, r.size.w, r.size.h)
}

/// Build one toggle with a fresh store, so the first call lands at the target value.
fn toggle_decl(
    style: &dyn ui_widgets::StyleKit,
    env: &std::sync::Arc<ui_style::Env>,
    hot: bool,
) -> Decl {
    let id = Id::str("t");
    let mut store = AnimStore::new();
    // Scope is only visible in what the builder draws, so the hot state has to come
    // from real routing rather than being assumed.
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing {
        hovered: if hot { Some(id) } else { None },
        hovered_path: if hot { vec![id] } else { Vec::new() },
        ..ui_core::cx::Routing::default()
    });
    let mut cx = ui_widgets::Cx::new(&mut core, env.clone());
    let spec = ui_style::ToggleSpec::new(id, false, "");
    let control = style.toggle(&spec, &mut cx);
    cx.with_cx(|inner| control.decl(inner))
}

fn md3_scope(outline_width: f32) -> std::sync::Arc<ui_style::Env> {
    ui_style::Env::with(
        None,
        Md3Options {
            outline_width,
            ..Default::default()
        },
    )
}
