//! Paint-only shapes: one node, several painted things, no layout slots.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeKind};
use ui_platform::present::OffscreenTarget;
use ui_platform::{App, AppLoop};

fn fixed(id: &'static str, w: f32, h: f32, colour: Color) -> Decl {
    Decl {
        id: Id::str(id),
        kind: NodeKind::Box,
        shapes: vec![ui_core::draw::Shape::Box {
            corners: Corners::ZERO,
            fill: colour,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        }],
        data: ui_core::NodeData::Container(ui_core::tree::ContainerParams::default()),
        size_class: SizeClass {
            width: AxisSize::fixed(w),
            height: AxisSize::fixed(h),
            grow_x: Grow::No,
            grow_y: Grow::No,
        },
        ink_margin: Insets::ZERO,
        box_model: Default::default(),
        offset: ui_core::geom::Point::new(0.0, 0.0),
        cursor: None,
        sense: ui_core::Sense::NONE,
        disabled: false,
        grid: Default::default(),
        placement: Default::default(),
        env: None,
        children: Vec::new(),
    }
}

const RED: Color = Color::rgba(1.0, 0.0, 0.0, 1.0);
const GREEN: Color = Color::rgba(0.0, 1.0, 0.0, 1.0);
const BLUE: Color = Color::rgba(0.0, 0.0, 1.0, 1.0);

struct Painted;

impl App for Painted {
    fn view(&mut self, _info: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let decl = Decl::container(Id::str("root"), NodeKind::Box)
            .background(RED, Corners::ZERO)
            .size(SizeClass::fixed(40.0, 40.0))
            // A circle, in the node's own space.
            .shapes(vec![Shape::Rect {
                rect: ui_core::geom::Rect::new(2.0, 2.0, 10.0, 10.0),
                corners: Corners::all(5.0),
                fill: GREEN,
                gradient: None,
                border: None,
            }])
            .child(fixed("child", 4.0, 4.0, BLUE));
        ui_platform::from_decl(decl)
    }
}

#[test]
fn a_shape_paints_above_its_own_node_and_below_its_children() {
    // The order is what a control relies on: background, its own shapes, then content.
    let mut lp = AppLoop::new(Painted);
    let mut target = OffscreenTarget::new(80, 80);
    lp.frame(&mut target, None);
    let list = lp.draw_list();
    let root = lp.tree().find_by_id(Id::str("root")).expect("root");
    let child = lp.tree().find_by_id(Id::str("child")).expect("child");
    let owned: Vec<(bool, Color)> = (0..list.prims.rects.len())
        .filter_map(|i| {
            let owner = *list.owners.rects.get(i)?;
            let p = &list.prims.rects[i];
            Some((owner == root, p.fill))
        })
        .collect();
    // Root paints twice, then the child once.
    let root_fills: Vec<Color> = owned.iter().filter(|(r, _)| *r).map(|(_, c)| *c).collect();
    assert_eq!(root_fills, vec![RED, GREEN], "background, then the shape");
    let child_at = owned
        .iter()
        .position(|(r, c)| !*r && *c == BLUE)
        .expect("the child paints");
    let shape_at = owned
        .iter()
        .position(|(r, c)| *r && *c == GREEN)
        .expect("the shape paints");
    assert!(
        child_at > shape_at,
        "children paint above the node's own shapes"
    );
    let _ = child;
}
