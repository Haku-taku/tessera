// The cards page.

use crate::shell::*;

/// The three clickable cards, one per variant; each counts its own activations.
pub(crate) const CARD_CLICKS: usize = 3;
pub(crate) const CARD_VARIANTS: [&str; CARD_CLICKS] = ["elevated", "filled", "outlined"];
pub(crate) const CARD_VARIANT_TITLES: [Msg; CARD_CLICKS] = [
    Msg::GALLERY_CARD_ELEVATED,
    Msg::GALLERY_CARD_FILLED,
    Msg::GALLERY_CARD_OUTLINED,
];
pub(crate) const CARD_CONTENT_W: f32 = COL_W - 24.0 - 32.0;

pub(crate) fn card_id(tag: &'static str) -> Id {
    Id::of(("card", Page::Cards.tag(), tag))
}

/// The glass column's three surfaces: a card, a solid card and a plain container.
pub(crate) fn glass_card_id(tag: &'static str) -> Id {
    Id::of(("gcard", Page::Cards.tag(), tag))
}

/// What a card holds: a title and a body, at a fixed width so the three cards line up.
///
/// `Md3Card` owns the surface and never restyles what it holds, so the ink is the caller's:
/// `onSurface` for the title, `onSurfaceVariant` for the body, both at the disabled opacity
/// when the card is disabled. A fixed colour here is one theme's card in the other theme.
pub(crate) fn card_content(id: Id, title: &str, body: String, disabled: bool) -> Vec<Decl> {
    let pack = ui_widgets_md3::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let role = |name: &'static str| pack.ink(name).unwrap_or(Color::WHITE);
    let dim = if disabled {
        ui_widgets_md3::DISABLED_CONTENT_OPACITY
    } else {
        1.0
    };
    vec![
        Decl::container(id.with("content"), NodeKind::Column)
            .size(SizeClass::fixed(CARD_CONTENT_W, 44.0))
            .gap(4.0)
            .child(label(
                id.with("title"),
                title.to_string(),
                14.0,
                role("onSurface").faded(dim),
                CARD_CONTENT_W,
            ))
            .child(label(
                id.with("body"),
                body,
                12.0,
                role("onSurfaceVariant").faded(dim),
                CARD_CONTENT_W,
            )),
    ]
}

/// One card with its build-time axes: variant, whether it acts, and disabled.
#[allow(clippy::too_many_arguments)]
pub(crate) fn card(
    id: Id,
    title: &str,
    body: String,
    variant: ui_widgets_md3::CardVariant,
    clickable: bool,
    style: &Md3Kit,
    disabled: bool,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let env = if disabled {
        disabled_env()
    } else {
        style_env("md3")
    };
    let mut scoped = cx.at_scope(env);
    let content = card_content(id, title, body, disabled);
    let control = style.card(id, variant, content, clickable, &mut scoped);
    scoped.element(control)
}

/// The card's build-time variants: three surfaces, a static one, and a disabled row.
pub(crate) fn cards_page(hits: [u32; CARD_CLICKS], cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    use ui_widgets_md3::CardVariant as V;
    let mut row = Container::new(Id::of(("bodyrow", Page::Cards.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(10.0);
    let mut variants = Container::new(card_id("variants"), NodeKind::Column).gap(10.0);
    for (k, (tag, variant)) in CARD_VARIANTS
        .iter()
        .zip([V::Elevated, V::Filled, V::Outlined])
        .enumerate()
    {
        variants = variants.child(card(
            card_id(tag),
            &tr(cx, CARD_VARIANT_TITLES[k]),
            tf(
                cx,
                Msg::GALLERY_CARD_ACTIVATED,
                &[("count", arg(hits[k] as i64))],
            ),
            variant,
            true,
            &style,
            false,
            cx,
        ));
    }
    col = col.child(Box::new(captioned_row(
        card_id("variants-cap"),
        Box::new(variants),
        tr(cx, Msg::GALLERY_CARDS_CAPTION_VARIANTS),
    )));
    col = col.child(Box::new(captioned_row(
        card_id("static-cap"),
        card(
            card_id("static"),
            &tr(cx, Msg::GALLERY_CARD_STATIC),
            tr(cx, Msg::GALLERY_CARD_STATIC_BODY),
            V::Filled,
            false,
            &style,
            false,
            cx,
        ),
        tr(cx, Msg::GALLERY_CARDS_CAPTION_STATIC),
    )));
    let mut disabled = Container::new(card_id("disabled"), NodeKind::Column).gap(10.0);
    for (k, (tag, variant)) in [
        ("elevated-off", V::Elevated),
        ("filled-off", V::Filled),
        ("outlined-off", V::Outlined),
    ]
    .into_iter()
    .enumerate()
    {
        disabled = disabled.child(card(
            card_id(tag),
            &tr(cx, CARD_VARIANT_TITLES[k]),
            tr(cx, Msg::GALLERY_CARD_DISABLED_BODY),
            variant,
            true,
            &style,
            true,
            cx,
        ));
    }
    col = col.child(Box::new(captioned_row(
        card_id("disabled-cap"),
        Box::new(disabled),
        tr(cx, Msg::GALLERY_CARDS_CAPTION_DISABLED),
    )));
    col = col.child_decl(label(
        Id::str("cards/readout"),
        tf(
            cx,
            Msg::GALLERY_CARDS_READOUT,
            &[("hits", arg(format!("{hits:?}")))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Cards,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_TALL_H,
        cx,
    ));
    row = row.child(pending_column(Page::Cards, "fluent", COL_W, COL_TALL_H, cx));

    // ── The glass column: the reference's card, its solid twin, and a plain container.
    let gid = glass_card_id;
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let ink = pack.ink("textFillColorPrimary").unwrap_or(Color::BLACK);
    let sub = Color { a: 0.65, ..ink };
    let mut glass_col = Container::new(Id::of(("body", "glass")), NodeKind::Column).gap(14.0);
    let title = tr(cx, Msg::GALLERY_CARD_GLASS_TITLE);
    let body = tr(cx, Msg::GALLERY_CARD_GLASS_BODY);
    let text = |id: Id, title: &str, body: &str| {
        Decl::container(id.with("content"), NodeKind::Column)
            .gap(2.0)
            .child(label(id.with("title"), title, 16.0, ink, CARD_CONTENT_W))
            .child(label(id.with("body"), body, 13.0, sub, CARD_CONTENT_W))
    };
    let card = ui_widgets_glass::GlassCard::new(
        ui_widgets_glass::GlassCardParams::card(gid("card")),
        &pack,
    )
    .child(text(gid("card"), &title, &body));
    glass_col = glass_col.child(Box::new(captioned_row(
        gid("card-cap"),
        Box::new(Container::new(gid("card-holder"), NodeKind::Column).child_decl(card.resolve())),
        tr(cx, Msg::GALLERY_CARDS_GLASS_CAPTION_CARD),
    )));
    let solid = ui_widgets_glass::GlassCard::new(
        ui_widgets_glass::GlassCardParams::card(gid("solid")).solid(),
        &pack,
    )
    .child(text(gid("solid"), &title, &body));
    glass_col = glass_col.child(Box::new(captioned_row(
        gid("solid-cap"),
        Box::new(Container::new(gid("solid-holder"), NodeKind::Column).child_decl(solid.resolve())),
        tr(cx, Msg::GALLERY_CARDS_GLASS_CAPTION_SOLID),
    )));
    let plain = ui_widgets_glass::GlassCard::new(
        ui_widgets_glass::GlassCardParams::container(gid("plain")),
        &pack,
    )
    .child(
        Decl::container(gid("plain").with("content"), NodeKind::Stack)
            .size(SizeClass::fixed(CARD_CONTENT_W, 56.0))
            .stack_align(ui_core::layout::OverlayAlign::Center)
            .child(label(
                gid("plain").with("label"),
                title.as_str(),
                15.0,
                ink,
                CARD_CONTENT_W,
            )),
    );
    glass_col = glass_col.child(Box::new(captioned_row(
        gid("plain-cap"),
        Box::new(Container::new(gid("plain-holder"), NodeKind::Column).child_decl(plain.resolve())),
        tr(cx, Msg::GALLERY_CARDS_GLASS_CAPTION_CONTAINER),
    )));
    glass_col = glass_col.child_decl(label(
        Id::str("cards/glass-note"),
        tr(cx, Msg::GALLERY_CARDS_GLASS_NOTE),
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Cards,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        Box::new(glass_col),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
