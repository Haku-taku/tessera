// The positioning page: relative shift, absolute pinning, fixed, and `z` order.

use crate::shell::*;
use ui_core::layout::AUTO;

/// tag and stage size; the self-test reads the same numbers the page lays out.
pub(crate) const POSITIONING_SAMPLES: [(&str, f32, f32); 3] = [
    ("relative", 320.0, 220.0),
    ("absolute", 320.0, 220.0),
    ("fixed", 320.0, 220.0),
];

/// How far the relative sample's block is displaced, in logical units.
pub(crate) const REL_SHIFT: (f32, f32) = (40.0, 30.0);

pub(crate) fn positioning_id(tag: &'static str, name: &'static str) -> Id {
    Id::of(("positioning", Page::Positioning.tag(), tag, name))
}

fn ink() -> Color {
    Color::from_rgba8(245, 247, 252, 255)
}

fn blue() -> Color {
    Color::from_rgba8(66, 108, 190, 255)
}

fn teal() -> Color {
    Color::from_rgba8(36, 130, 120, 255)
}

fn amber() -> Color {
    Color::from_rgba8(150, 96, 48, 255)
}

fn violet() -> Color {
    Color::from_rgba8(96, 74, 158, 255)
}

/// The stage every sample lays its boxes out on: a fixed-size, positioned box.
fn stage(tag: &'static str, w: f32, h: f32) -> Decl {
    Decl::container(positioning_id(tag, "stage"), NodeKind::Box)
        .size(SizeClass::fixed(w, h))
        .padding(Insets::all(12.0))
        .background(Color::from_rgba8(24, 26, 33, 255), Corners::all(10.0))
        .relative()
}

fn tile(id: Id, w: f32, h: f32, fill: Color, body: &str) -> Decl {
    Decl::container(id, NodeKind::Box)
        .size(SizeClass::fixed(w, h))
        .background(fill, Corners::all(8.0))
        .child(label(id.with("label"), body.to_string(), 11.0, ink(), w))
}

/// The block keeps its slot and only its pixels move; the dashed slot stays behind.
fn relative_sample(w: f32, h: f32, shifted: &str) -> Decl {
    let slot = Decl::container(positioning_id("relative", "slot"), NodeKind::Box)
        .size(SizeClass::fixed(120.0, 80.0))
        .shapes(vec![ui_core::draw::Shape::Box {
            corners: Corners::all(8.0),
            fill: Color::TRANSPARENT,
            gradient: None,
            border: Some((Color::from_rgba8(120, 130, 160, 255), 1.5)),
            shadow: None,
            image: None,
        }]);
    let mover = tile(
        positioning_id("relative", "mover"),
        120.0,
        80.0,
        blue(),
        shifted,
    )
    .relative()
    .inset(Insets::new(REL_SHIFT.0, REL_SHIFT.1, AUTO, AUTO));
    stage("relative", w, h).gap(12.0).child(
        Decl::container(positioning_id("relative", "row"), NodeKind::Row)
            .size(SizeClass::hug(0.0, 0.0))
            .gap(12.0)
            .child(slot)
            .child(mover),
    )
}

/// One badge pinned to the padding box's corner, one centred by auto margins.
fn absolute_sample(w: f32, h: f32, pin: &str, auto: &str, in_flow: &str) -> Decl {
    let pin = tile(positioning_id("absolute", "pin"), 72.0, 28.0, amber(), pin)
        .absolute()
        .inset(Insets::new(AUTO, AUTO, 0.0, 0.0));
    let centred = tile(
        positioning_id("absolute", "centre"),
        96.0,
        40.0,
        teal(),
        auto,
    )
    .absolute()
    .inset(Insets::all(0.0))
    .margin(Insets {
        left: AUTO,
        top: AUTO,
        right: AUTO,
        bottom: AUTO,
    });
    stage("absolute", w, h)
        .child(
            Decl::container(positioning_id("absolute", "flow"), NodeKind::Column)
                .size(SizeClass::fixed(120.0, 60.0))
                .background(Color::from_rgba8(44, 48, 60, 255), Corners::all(8.0))
                .child(label(
                    positioning_id("absolute", "flow").with("label"),
                    in_flow.to_string(),
                    11.0,
                    ink(),
                    110.0,
                )),
        )
        .child(pin)
        .child(centred)
}

/// Two overlapping blocks ordered by `z` rather than by declaration, plus a fixed badge.
fn fixed_sample(w: f32, h: f32, fixed: &str) -> Decl {
    // Declared high first, so only `z` can put it on top.
    let high = tile(
        positioning_id("fixed", "high"),
        140.0,
        90.0,
        violet(),
        "z 5",
    )
    .absolute()
    .inset(Insets::new(70.0, 60.0, AUTO, AUTO))
    .z(5);
    let low = tile(positioning_id("fixed", "low"), 140.0, 90.0, blue(), "z 1")
        .absolute()
        .inset(Insets::new(20.0, 20.0, AUTO, AUTO))
        .z(1);
    let badge = tile(positioning_id("fixed", "badge"), 64.0, 26.0, amber(), fixed)
        .fixed()
        .inset(Insets::new(AUTO, AUTO, 0.0, 0.0));
    stage("fixed", w, h).child(high).child(low).child(badge)
}

pub(crate) fn positioning_page(cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Positioning.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);

    let [rel, abs, fixed] = POSITIONING_SAMPLES;
    row = row.child(column(
        Page::Positioning,
        rel.0,
        Msg::GALLERY_POSITIONING_COLUMN_RELATIVE,
        from_decl(relative_sample(
            rel.1,
            rel.2,
            &tr(cx, Msg::GALLERY_POSITIONING_TILE_SHIFTED),
        )),
        COL_CTRL_H,
        cx,
    ));
    row = row.child(column(
        Page::Positioning,
        abs.0,
        Msg::GALLERY_POSITIONING_COLUMN_ABSOLUTE,
        from_decl(absolute_sample(
            abs.1,
            abs.2,
            &tr(cx, Msg::GALLERY_POSITIONING_TILE_PIN),
            &tr(cx, Msg::GALLERY_POSITIONING_TILE_AUTO),
            &tr(cx, Msg::GALLERY_POSITIONING_TILE_IN_FLOW),
        )),
        COL_CTRL_H,
        cx,
    ));
    row = row.child(column(
        Page::Positioning,
        fixed.0,
        Msg::GALLERY_POSITIONING_COLUMN_FIXED,
        from_decl(fixed_sample(
            fixed.1,
            fixed.2,
            &tr(cx, Msg::GALLERY_POSITIONING_TILE_FIXED),
        )),
        COL_CTRL_H,
        cx,
    ));
    Box::new(row)
}
