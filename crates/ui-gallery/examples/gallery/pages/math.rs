// The math page: one formula drawn by each design system, so the ink and size are the axis.

use crate::shell::*;
use ui_widgets_fluent::FluentMath;
use ui_widgets_glass::GlassMath;
use ui_widgets_md3::Md3Math;

/// The same source in all three columns; LaTeX is language-neutral, so it is not catalogued.
const INLINE_SOURCE: &str = "a^2 + b^2 = c^2";
const DISPLAY_SOURCE: &str = r"\frac{\pi}{2}";
/// A source the parser refuses: the fraction stays, its denominator's run is boxed.
const REFUSED_SOURCE: &str = r"\frac{1}{\frc}";
/// The editable formula of each column, by that column's tag.
pub(crate) const MATH_COLUMNS: [&str; 3] = ["md3", "fluent", "glass"];

/// The structural editor of one column; its keystrokes report against this id.
pub(crate) fn math_edit_id(tag: &'static str) -> Id {
    Id::of(("math-edit", Page::Math.tag(), tag))
}

/// The read-only formula that demonstrates a refused source, in the first column.
pub(crate) fn math_refused_id() -> Id {
    Id::of(("math-refused", Page::Math.tag()))
}

/// What each structured formula starts with: an empty denominator is a clickable slot.
pub(crate) const STRUCTURED_SEED: &str = r"\frac{\pi}{} + x_1^2";

pub(crate) fn math_page(
    structured: &[String; MATH_COLUMNS.len()],
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let cx: &Cx<'_, '_> = cx;
    let mut row = Container::new(Id::of(("bodyrow", Page::Math.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    // The frame's own appearance, not the light pack: a fixed pack is a formula drawn in the
    // other theme's ink.
    let appearance = ui_style::StyleScope::of(&root_env()).appearance;
    let structured_caption = tr(cx, Msg::GALLERY_MATH_STRUCTURED_CAPTION);
    let refused_caption = tr(cx, Msg::GALLERY_MATH_REFUSED_CAPTION);
    let md3 = ui_widgets_md3::pack_of(appearance);
    let mut body = formulas(
        "md3",
        "body-large",
        "on-surface",
        cx,
        |id, source, display| {
            let math = Md3Math::new(id, source, md3);
            let math = if display { math.display() } else { math };
            math.resolve_in(cx)
        },
    );
    body = body.child_decl(structured_label("md3", &structured_caption));
    body = body.child_decl(
        Md3Math::new(math_edit_id("md3"), &structured[0], md3)
            .editable()
            .resolve_in(cx),
    );
    body = body.child_decl(refused_label(&refused_caption));
    body = body.child_decl(Md3Math::new(math_refused_id(), REFUSED_SOURCE, md3).resolve_in(cx));
    row = row.child(column(
        Page::Math,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(body),
        COL_CTRL_H,
        cx,
    ));
    let fluent = ui_widgets_fluent::pack_of(appearance);
    let mut body = formulas(
        "fluent",
        "Body",
        "TextFillColorPrimary",
        cx,
        |id, source, display| {
            let math = FluentMath::new(id, source, fluent);
            let math = if display { math.display() } else { math };
            math.resolve_in(cx)
        },
    );
    body = body.child_decl(structured_label("fluent", &structured_caption));
    body = body.child_decl(
        FluentMath::new(math_edit_id("fluent"), &structured[1], fluent)
            .editable()
            .resolve_in(cx),
    );
    row = row.child(column(
        Page::Math,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(body),
        COL_CTRL_H,
        cx,
    ));
    let glass = ui_widgets_glass::pack_of(appearance);
    let mut body = formulas(
        "glass",
        "Body",
        "primary text ink",
        cx,
        |id, source, display| {
            let math = GlassMath::new(id, source, glass);
            let math = if display { math.display() } else { math };
            math.resolve_in(cx)
        },
    );
    body = body.child_decl(structured_label("glass", &structured_caption));
    body = body.child_decl(
        GlassMath::new(math_edit_id("glass"), &structured[2], glass)
            .editable()
            .resolve_in(cx),
    );
    row = row.child(column(
        Page::Math,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        Box::new(body),
        COL_CTRL_H,
        cx,
    ));
    Box::new(row)
}

/// The caption above one column's structured formula.
fn structured_label(tag: &'static str, text: &str) -> Decl {
    label(
        Id::of(("math-structured-cap", tag)),
        text.to_owned(),
        11.0,
        muted(),
        COL_W - 24.0,
    )
}

/// The caption above the refused-source formula.
fn refused_label(text: &str) -> Decl {
    label(
        Id::of(("math-refused-cap",)),
        text.to_owned(),
        11.0,
        muted(),
        COL_W - 24.0,
    )
}

/// The two read-only formulas one pack draws: display style, then inline style, each captioned
/// with the role its size and ink came from. The editable rows are appended by the caller.
fn formulas(
    tag: &'static str,
    size: &str,
    ink: &str,
    cx: &Cx<'_, '_>,
    build: impl Fn(Id, &str, bool) -> Decl,
) -> Container {
    let mut body = Container::new(Id::of(("math-body", tag)), NodeKind::Column).gap(18.0);
    for (suffix, source, display, caption) in [
        (
            "display",
            DISPLAY_SOURCE,
            true,
            Msg::GALLERY_MATH_DISPLAY_CAPTION,
        ),
        (
            "inline",
            INLINE_SOURCE,
            false,
            Msg::GALLERY_MATH_INLINE_CAPTION,
        ),
    ] {
        let id = Id::of(("math-formula", tag, suffix));
        let args = [("size", arg(size.to_owned())), ("ink", arg(ink.to_owned()))];
        body = body.child_decl(label(
            id.with("cap"),
            tf(cx, caption, &args),
            11.0,
            muted(),
            COL_W - 24.0,
        ));
        body = body.child_decl(build(id, source, display));
    }
    body
}
