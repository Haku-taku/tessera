//! Layout computes a content mask; paint must honour it.
//!
//! Before this was wired, a child larger than its parent painted straight over the
//! parent's bounds — the mask existed on every node but never reached the renderer.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ContainerParams;
use ui_core::{Decl, Id, NodeData, NodeKind};
use ui_platform::present::OffscreenTarget;

use ui_platform::{App, AppLoop};

fn fixed_box(id: &'static str, w: f32, h: f32, color: Color) -> Decl {
    Decl {
        id: Id::str(id),
        kind: NodeKind::Box,
        shapes: vec![ui_core::draw::Shape::Box {
            corners: Corners::ZERO,
            fill: color,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        }],
        data: NodeData::Container(ContainerParams::default()),
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

/// A 80x80 rounded container (radius 40, squircle) with a child that fills it.
struct Rounded;

impl App for Rounded {
    fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let decl = {
            let parent = Decl {
                id: Id::str("parent"),
                kind: NodeKind::Column,
                shapes: vec![ui_core::draw::Shape::Box {
                    corners: Corners::all(40.0).squircle(),
                    fill: Color::from_rgba8(40, 40, 40, 255),
                    gradient: None,
                    border: None,
                    shadow: None,
                    image: None,
                }],
                // The curve cuts only where the container declares a clip.
                data: NodeData::Container(ContainerParams {
                    clip: ui_core::layout::Clip {
                        x: ui_core::layout::Overflow::Hidden,
                        y: ui_core::layout::Overflow::Hidden,
                    },
                    ..Default::default()
                }),
                size_class: SizeClass {
                    width: AxisSize::fixed(80.0),
                    height: AxisSize::fixed(80.0),
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
                children: vec![
                    fixed_box("child", 80.0, 80.0, Color::from_rgba8(255, 0, 0, 255)).into(),
                ],
            };
            Decl::container(Id::str("root"), NodeKind::Column)
                .background(Color::from_rgba8(0, 0, 0, 255), Corners::ZERO)
                .child(parent)
        };
        ui_platform::from_decl(decl)
    }
}
#[test]
fn a_rounded_parent_clips_its_child_to_the_curve() {
    // The child fills the parent's rect, so a rectangular clip would leave the corner
    // red; only a clip that follows the curve can empty it.
    let mut app = AppLoop::new(Rounded);
    let mut target = OffscreenTarget::new(120, 120);
    app.frame(&mut target, None);
    let px = &target.pixels;
    let at = |x: u32, y: u32| {
        let i = ((y * px.width + x) * 4) as usize;
        px.data[i + 2]
    };
    assert!(
        at(2, 2) < 60,
        "the corner must follow the curve, got red {}",
        at(2, 2)
    );
    assert!(
        at(40, 40) > 200,
        "the centre must stay painted, got red {}",
        at(40, 40)
    );
}

/// A clipped 40x40 parent holding a lifted 30x30 child drawn 80 px away, under a full-size
/// sibling declared after the parent: the child must still show, above that sibling.
struct Lifted;

impl App for Lifted {
    fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let parent = Decl {
            id: Id::str("parent"),
            kind: NodeKind::Column,
            shapes: Vec::new(),
            data: NodeData::Container(ContainerParams {
                clip: ui_core::layout::Clip {
                    x: ui_core::layout::Overflow::Hidden,
                    y: ui_core::layout::Overflow::Hidden,
                },
                ..Default::default()
            }),
            size_class: SizeClass {
                width: AxisSize::fixed(40.0),
                height: AxisSize::fixed(40.0),
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
            children: vec![
                fixed_box("lifted", 30.0, 30.0, Color::from_rgba8(255, 0, 0, 255))
                    .offset(80.0, 80.0)
                    .lifted()
                    .into(),
            ],
        };
        ui_platform::from_decl(
            Decl::container(Id::str("root"), NodeKind::Column)
                .background(Color::from_rgba8(0, 0, 0, 255), Corners::ZERO)
                .child(parent)
                .child(fixed_box(
                    "cover",
                    160.0,
                    160.0,
                    Color::from_rgba8(0, 0, 255, 255),
                )),
        )
    }
}

#[test]
fn a_lifted_child_escapes_its_clip_and_paints_above_later_siblings() {
    let mut app = AppLoop::new(Lifted);
    let mut target = OffscreenTarget::new(160, 160);
    app.frame(&mut target, None);
    let px = &target.pixels;
    let i = ((90 * px.width + 90) * 4) as usize;
    // Pixels are BGRA.
    let (blue, green, red) = (px.data[i], px.data[i + 1], px.data[i + 2]);
    assert!(
        red > 150 && green < 80 && blue < 80,
        "the lifted child must show above the sibling painted after its parent, got ({red},{green},{blue})"
    );
}

/// A box that clips its content, with a shadow of its own and a child far wider than it.
struct ClippedShadow;

impl App for ClippedShadow {
    fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let parent = Decl {
            id: Id::str("parent"),
            kind: NodeKind::Column,
            shapes: vec![ui_core::draw::Shape::Box {
                corners: Corners::ZERO,
                fill: Color::from_rgba8(0, 255, 0, 255),
                gradient: None,
                border: None,
                shadow: Some(ui_core::tree::ShadowSpec::black(4.0, 1.0, 0.0)),
                image: None,
            }],
            data: NodeData::Container(ContainerParams {
                clip: ui_core::layout::Clip {
                    x: ui_core::layout::Overflow::Hidden,
                    y: ui_core::layout::Overflow::Hidden,
                },
                ..Default::default()
            }),
            size_class: SizeClass {
                width: AxisSize::fixed(80.0),
                height: AxisSize::fixed(80.0),
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
            children: vec![
                fixed_box("child", 200.0, 200.0, Color::from_rgba8(255, 0, 0, 255)).into(),
            ],
        };
        ui_platform::from_decl(
            Decl::container(Id::str("root"), NodeKind::Column)
                .background(Color::from_rgba8(255, 255, 255, 255), Corners::ZERO)
                .padding(Insets::all(20.0))
                .child(parent),
        )
    }
}

#[test]
fn a_nodes_own_clip_cuts_its_children_but_not_its_own_shadow() {
    let mut app = AppLoop::new(ClippedShadow);
    let mut target = OffscreenTarget::new(160, 160);
    app.frame(&mut target, None);
    let px = &target.pixels;
    // Pixels are BGRA.
    let at = |x: u32, y: u32| {
        let i = ((y * px.width + x) * 4) as usize;
        (px.data[i + 2], px.data[i + 1], px.data[i])
    };

    // The box spans 20..100; half a pixel outside its edge the cast carries about 40 %.
    let (red, green, _) = at(19, 60);
    assert!(
        red < 190 && green < 190,
        "the box's own shadow must escape its clip, got ({red},{green})"
    );
    assert_eq!(at(5, 60), (255, 255, 255), "and stop inside its falloff");

    // The child is still cut at the box: red would reach x 220 otherwise.
    let (red, green, _) = at(110, 60);
    assert!(
        green > 200 && red > 200,
        "the child must stay clipped, got ({red},{green})"
    );
    let (red, green, _) = at(60, 60);
    assert!(
        red > 200 && green < 80,
        "the child must still paint inside the box, got ({red},{green})"
    );
}
