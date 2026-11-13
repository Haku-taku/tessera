// The text field and search pages.

use ui_widgets_glass::{
    GlassSearchBar, GlassSearchBarParams, GlassTextField, GlassTextFieldParams,
};

use crate::pages::glyphs::sf_glyph;
use crate::shell::*;

/// The glass column's live field, its search variant, and a still one.
pub(crate) fn glass_text_field_id() -> Id {
    Id::str("glass/text-field")
}

pub(crate) fn glass_search_field_id() -> Id {
    Id::str("glass/text-field/search")
}

pub(crate) fn glass_still_field_id() -> Id {
    Id::str("glass/text-field/still")
}

/// The glass column's secret field: marks on screen, the real buffer in the app.
pub(crate) fn glass_secure_field_id() -> Id {
    Id::str("glass/text-field/secure")
}

/// How many fields on this page are live; each buffer belongs to the app.
pub(crate) const TF_LIVE: usize = 2;
/// The sample width inside a `COL_W` column.
pub(crate) const TF_W: f32 = 332.0;

pub(crate) fn text_field_id(k: usize) -> Id {
    Id::of(("text-field", k))
}

/// The Fluent column's live field.
pub(crate) fn fluent_text_field_id() -> Id {
    Id::str("fluent/text-field")
}

pub(crate) fn text_field_clear_id() -> Id {
    Id::str("text-field/clear")
}

/// The shortcut field that captures a chord for the app.
pub(crate) fn shortcut_field_id() -> Id {
    Id::str("shortcut/field")
}

pub(crate) fn tf_block(k: usize, caption: String, control: Box<dyn Element>) -> Box<dyn Element> {
    Box::new(captioned_row(Id::of(("tf/sample", k)), control, caption))
}

/// The Fluent column's blocks: their ids must not collide with the MD3 rows, or the tree
/// would merge the two columns into one set of samples.
pub(crate) fn tf_block_fluent(
    k: usize,
    caption: String,
    control: Box<dyn Element>,
) -> Box<dyn Element> {
    Box::new(captioned_row(Id::of(("tf/fluent", k)), control, caption))
}

/// The search bar whose query the app owns.
pub(crate) fn search_id() -> Id {
    Id::str("search/bar")
}

/// The bar's one trailing action, named by the app.
pub(crate) fn search_action_id() -> Id {
    Id::str("search/bar/mic")
}

/// The glass search bar's live query, and the still sample without a cancel button.
pub(crate) fn glass_search_id() -> Id {
    Id::str("glass/search")
}

pub(crate) fn glass_search_quiet_id() -> Id {
    Id::str("glass/search/quiet")
}

/// The search view the app opens by itself, and the action that closes it.
pub(crate) fn search_view_id() -> Id {
    Id::str("search/view")
}

pub(crate) fn search_view_close_id() -> Id {
    Id::str("search/view/close")
}

/// MD3 search bars: a live query, the contained variant, and a query too long to show.
pub(crate) fn search_page(
    value: &str,
    acted: Option<&str>,
    view_open: bool,
    view_value: &str,
    glass: &str,
    glass_acted: Option<&str>,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Search.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(8.0);

    let hint = tr(&scoped, Msg::GALLERY_SEARCH_HINT);
    let bar = style.search_bar(
        ui_widgets_md3::SearchBarParams::new(search_id(), 332.0)
            .hint(&hint)
            .text(value)
            .leading(ms("search"))
            .action(search_action_id(), ms("close")),
        &mut scoped,
    );
    let caption = tr(&scoped, Msg::GALLERY_SEARCH_CAPTION_DOCKED);
    col = col.child(Box::new(captioned_row(
        Id::str("search/live"),
        scoped.element(bar),
        caption,
    )));

    let contained_hint = tr(&scoped, Msg::GALLERY_SEARCH_HINT_CONTAINED);
    let contained = style.search_bar(
        ui_widgets_md3::SearchBarParams::new(Id::str("search/contained"), 240.0)
            .contained(true)
            .hint(&contained_hint)
            .leading(ms("search")),
        &mut scoped,
    );
    let caption = tr(&scoped, Msg::GALLERY_SEARCH_CAPTION_CONTAINED);
    col = col.child(Box::new(captioned_row(
        Id::str("search/contained-row"),
        scoped.element(contained),
        caption,
    )));

    // The fake query is simulated user input: data, not chrome, so it stays as written.
    let long = style.search_bar(
        ui_widgets_md3::SearchBarParams::new(Id::str("search/long"), 332.0)
            .text("a query far longer than this bar can ever show at once")
            .leading(ms("search")),
        &mut scoped,
    );
    let caption = tr(&scoped, Msg::GALLERY_SEARCH_CAPTION_LONG);
    col = col.child(Box::new(captioned_row(
        Id::str("search/long-row"),
        scoped.element(long),
        caption,
    )));

    // The bar's two optional slots: a helper line under the pill and a trailing avatar.
    let supporting = tr(&scoped, Msg::GALLERY_SEARCH_SUPPORTING);
    let addressed = style.search_bar(
        ui_widgets_md3::SearchBarParams::new(Id::str("search/avatar"), 332.0)
            .hint(&hint)
            .leading(ms("search"))
            .supporting(&supporting)
            .avatar(
                Decl::container(Id::str("search/avatar/mark"), NodeKind::Box)
                    .size(SizeClass::fixed(30.0, 30.0))
                    .background(Color::from_rgba8(198, 166, 118, 255), Corners::all(15.0)),
            ),
        &mut scoped,
    );
    let caption = tr(&scoped, Msg::GALLERY_SEARCH_CAPTION_SUPPORTING);
    col = col.child(Box::new(captioned_row(
        Id::str("search/avatar-row"),
        scoped.element(addressed),
        caption,
    )));

    let readout = tf(
        &scoped,
        Msg::GALLERY_SEARCH_READOUT,
        &[
            (
                "query",
                arg(if value.is_empty() {
                    "—".to_string()
                } else {
                    format!("\"{value}\"")
                }),
            ),
            ("action", arg(acted.unwrap_or("—").to_string())),
        ],
    );
    col = col.child_decl(label(
        Id::str("search/readout"),
        readout,
        11.0,
        muted(),
        332.0,
    ));

    // The same surface as a search view: shut it is the bar's one line, open it is a page
    // over its results. Clicking the bar opens it; its leading back arrow closes it.
    let view_hint = tr(&scoped, Msg::GALLERY_SEARCH_VIEW_HINT);
    let hit_ink = muted();
    let mut hits = Decl::container(Id::str("search/view/hits"), NodeKind::Column)
        .padding(Insets {
            left: 12.0,
            right: 12.0,
            top: 8.0,
            bottom: 8.0,
        })
        .gap(10.0);
    for (k, text) in ["Alpha", "Beta", "Gamma"].into_iter().enumerate() {
        hits = hits.child(label(
            Id::of(("search/view/hit", k)),
            text,
            14.0,
            hit_ink,
            300.0,
        ));
    }
    let view = style.search_view(
        ui_widgets_md3::SearchViewParams::new(
            search_view_id(),
            ui_core::geom::Size::new(TF_W, 56.0),
            ui_core::geom::Size::new(TF_W, 380.0),
        )
        .text(view_value)
        .hint(&view_hint)
        .expanded(view_open)
        .leading(ms("arrow_back"))
        .action(search_view_close_id(), ms("close"))
        .results(vec![hits]),
        &mut scoped,
    );
    let caption = tr(&scoped, Msg::GALLERY_SEARCH_CAPTION_VIEW);
    col = col.child(Box::new(captioned_row(
        Id::str("search/view-row"),
        scoped.element(view),
        caption,
    )));
    row = row.child(column(
        Page::Search,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_TALL_H,
        &scoped,
    ));
    row = row.child(pending_column(
        Page::Search,
        "fluent",
        COL_W,
        COL_TALL_H,
        &scoped,
    ));
    // The MD3 scope ends here; the glass column builds against the root scope.
    drop(scoped);

    // ── The glass column: a live capsule bar with its clear and cancel buttons, and one
    // without a cancel button.
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let mut glass_col = Container::new(Id::of(("body", "glass")), NodeKind::Column).gap(14.0);
    let placeholder = tr(cx, Msg::GALLERY_TF_SEARCH);
    let caption = tr(cx, Msg::GALLERY_SEARCH_CAPTION_GLASS);
    let bar = GlassSearchBar::new(
        GlassSearchBarParams::new(glass_search_id(), glass, TF_W)
            .placeholder(&placeholder)
            .cancel(true),
        cx,
        &pack,
    );
    glass_col = glass_col.child(Box::new(captioned_row(
        glass_search_id().with("cap"),
        cx.element(Box::new(bar)),
        caption,
    )));
    let caption = tr(cx, Msg::GALLERY_SEARCH_CAPTION_GLASS_QUIET);
    let quiet = GlassSearchBar::new(
        GlassSearchBarParams::new(glass_search_quiet_id(), "", TF_W).placeholder(&placeholder),
        cx,
        &pack,
    );
    glass_col = glass_col.child(Box::new(captioned_row(
        glass_search_quiet_id().with("cap"),
        cx.element(Box::new(quiet)),
        caption,
    )));
    let readout = tf(
        cx,
        Msg::GALLERY_SEARCH_READOUT,
        &[
            (
                "query",
                arg(if glass.is_empty() {
                    "—".to_string()
                } else {
                    format!("\"{glass}\"")
                }),
            ),
            ("action", arg(glass_acted.unwrap_or("—").to_string())),
        ],
    );
    glass_col = glass_col.child_decl(label(
        Id::str("glass/search/readout"),
        readout,
        11.0,
        muted(),
        TF_W,
    ));
    row = row.child(column(
        Page::Search,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        Box::new(glass_col),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}

/// MD3 text fields: two live buffers the engine edits, then the build-time variants.
pub(crate) fn text_field_page(
    values: &[String; TF_LIVE],
    fluent: &str,
    glass: &str,
    secure: &str,
    shortcut: Option<ui_core::KeyChord>,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::TextField.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(8.0);

    let labels = [
        tr(&scoped, Msg::GALLERY_TF_FILLED),
        tr(&scoped, Msg::GALLERY_TF_OUTLINED),
    ];
    let placeholder = tr(&scoped, Msg::GALLERY_TF_PLACEHOLDER);
    let menu_clear = tr(&scoped, Msg::GALLERY_TF_MENU_CLEAR);
    let mut live = Container::new(Id::str("tf/live"), NodeKind::Row)
        .gap(12.0)
        .cross_align(0.0);
    for (k, variant) in [
        (0, ui_widgets_md3::TextFieldVariant::Filled),
        (1, ui_widgets_md3::TextFieldVariant::Outlined),
    ] {
        let control = style.text_field(
            ui_widgets_md3::TextFieldParams::new(text_field_id(k), variant, 160.0)
                .label(&labels[k])
                .placeholder(&placeholder)
                .text(&values[k])
                // An app-declared row; the engine appends its own text rows below it.
                .menu(if k == 0 {
                    vec![ui_core::MenuItem::new(
                        Id::str("tf/menu/clear"),
                        menu_clear.clone(),
                    )]
                } else {
                    Vec::new()
                }),
            &mut scoped,
        );
        live = live.child(scoped.element(control));
    }
    col = col.child(Box::new(live));

    let shown: Vec<String> = values
        .iter()
        .map(|v| {
            if v.is_empty() {
                "—".to_string()
            } else {
                format!("\"{v}\"")
            }
        })
        .collect();
    let mut readout = Container::new(Id::str("tf/readout"), NodeKind::Row)
        .gap(10.0)
        .cross_align(0.0)
        .cross_pos(0.5)
        .child_decl(label(
            Id::str("tf/readout-text"),
            tf(
                &scoped,
                Msg::GALLERY_TF_READOUT,
                &[("buffers", arg(shown.join(" · ")))],
            ),
            11.0,
            muted(),
            250.0,
        ));
    let clear_label = tr(&scoped, Msg::GALLERY_TF_CLEAR_BUTTON);
    let clear = style.button(
        ui_widgets_md3::ButtonParams::new(text_field_clear_id(), clear_label)
            .style(ui_widgets_md3::ButtonStyle::Tonal)
            .size(ui_widgets_md3::ButtonSize::Base),
        &mut scoped,
    );
    readout = readout.child(scoped.element(clear));
    col = col.child(Box::new(readout));

    // A shortcut editor: focus it and the next chord is captured, not performed.
    let chord_caption = tr(&scoped, Msg::GALLERY_TF_CHORD_CAPTION);
    let chord_placeholder = tr(&scoped, Msg::GALLERY_TF_SHORTCUT_PLACEHOLDER);
    let mut chord_row = Container::new(Id::str("tf/chord"), NodeKind::Row)
        .gap(12.0)
        .cross_align(0.0)
        .child_decl(label(
            Id::str("tf/chord-caption"),
            chord_caption,
            11.0,
            muted(),
            250.0,
        ));
    let chord = style.shortcut_field(
        ui_widgets_md3::ShortcutFieldParams {
            id: shortcut_field_id(),
            chord: shortcut,
            placeholder: &chord_placeholder,
            width: 220.0,
            disabled: false,
        },
        &mut scoped,
    );
    chord_row = chord_row.child(scoped.element(chord));
    col = col.child(Box::new(chord_row));

    let label_text = tr(&scoped, Msg::GALLERY_TF_LABEL);
    let supporting_text = tr(&scoped, Msg::GALLERY_TF_SUPPORTING);
    let name_text = tr(&scoped, Msg::GALLERY_TF_NAME);
    let email = tr(&scoped, Msg::GALLERY_TF_EMAIL);
    let search_text = tr(&scoped, Msg::GALLERY_TF_SEARCH);
    let error_supporting = tr(&scoped, Msg::GALLERY_TF_ERROR_SUPPORTING);
    let disabled_text = tr(&scoped, Msg::GALLERY_TF_DISABLED);
    let captions = [
        tr(&scoped, Msg::GALLERY_TF_CAPTION_FILLED_LABEL),
        tr(&scoped, Msg::GALLERY_TF_CAPTION_FILLED_FLOATS),
        tr(&scoped, Msg::GALLERY_TF_CAPTION_OUTLINED_FLOATS),
        tr(&scoped, Msg::GALLERY_TF_CAPTION_OUTLINED_ICON),
        tr(&scoped, Msg::GALLERY_TF_CAPTION_OUTLINED_ERROR),
        tr(&scoped, Msg::GALLERY_TF_CAPTION_OUTLINED_DISABLED),
    ];
    // Every string is read before the closure takes its mutable borrow of `scoped`.
    let mut sample = |k: usize, params: ui_widgets_md3::TextFieldParams<'_>| -> Box<dyn Element> {
        let control = style.text_field(params, &mut scoped);
        tf_block(k, captions[k].clone(), scoped.element(control))
    };
    col = col.child(sample(
        0,
        ui_widgets_md3::TextFieldParams::new(
            Id::of(("tf/field", 0)),
            ui_widgets_md3::TextFieldVariant::Filled,
            TF_W,
        )
        .label(&label_text)
        .supporting(&supporting_text),
    ));
    col = col.child(sample(
        1,
        ui_widgets_md3::TextFieldParams::new(
            Id::of(("tf/field", 1)),
            ui_widgets_md3::TextFieldVariant::Filled,
            TF_W,
        )
        .label(&name_text)
        .text("Ada Lovelace"),
    ));
    col = col.child(sample(
        2,
        ui_widgets_md3::TextFieldParams::new(
            Id::of(("tf/field", 2)),
            ui_widgets_md3::TextFieldVariant::Outlined,
            TF_W,
        )
        .label(&email)
        .text("ada@example.com"),
    ));
    col = col.child(sample(
        3,
        ui_widgets_md3::TextFieldParams::new(
            Id::of(("tf/field", 3)),
            ui_widgets_md3::TextFieldVariant::Outlined,
            TF_W,
        )
        .label(&search_text)
        .leading(ms("search")),
    ));
    col = col.child(sample(
        4,
        ui_widgets_md3::TextFieldParams::new(
            Id::of(("tf/field", 4)),
            ui_widgets_md3::TextFieldVariant::Outlined,
            TF_W,
        )
        .label(&email)
        .text("not-an-email")
        .error(true)
        .supporting(&error_supporting),
    ));
    col = col.child(sample(
        5,
        ui_widgets_md3::TextFieldParams::new(
            Id::of(("tf/field", 5)),
            ui_widgets_md3::TextFieldVariant::Outlined,
            TF_W,
        )
        .label(&disabled_text)
        .disabled(true),
    ));
    row = row.child(column(
        Page::TextField,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_TALL_H,
        &scoped,
    ));
    // WinUI's text box: a header above the field, the placeholder inside it, and a clear
    // button that appears with text. Only the first field is live.
    let kit = ui_widgets_fluent::FluentKit::new();
    let mut fcol = Container::new(Id::of(("body", "fluent")), NodeKind::Column).gap(14.0);
    {
        let mut scoped = cx.at_scope(style_env("fluent"));
        let header = tr(&scoped, Msg::GALLERY_TF_NAME);
        let placeholder = tr(&scoped, Msg::GALLERY_TF_PLACEHOLDER);
        let caption = tr(&scoped, Msg::GALLERY_TF_CAPTION_FLUENT);
        let control = kit.text_box(
            ui_widgets_fluent::TextBoxParams::new(fluent_text_field_id(), fluent)
                .header(&header)
                .placeholder(&placeholder),
            &mut scoped,
        );
        fcol = fcol.child(tf_block_fluent(0, caption, scoped.element(control)));
    }
    {
        let mut scoped = cx.at_scope(style_env("fluent"));
        let header = tr(&scoped, Msg::GALLERY_TF_EMAIL);
        let caption = tr(&scoped, Msg::GALLERY_TF_CAPTION_FLUENT_TEXT);
        let control = kit.text_box(
            ui_widgets_fluent::TextBoxParams::new(
                Id::str("fluent/text-field-text"),
                "ada@example.com",
            )
            .header(&header),
            &mut scoped,
        );
        fcol = fcol.child(tf_block_fluent(1, caption, scoped.element(control)));
    }
    {
        let mut scoped = cx.at_scope(style_env("fluent"));
        let header = tr(&scoped, Msg::GALLERY_TF_DISABLED);
        let caption = tr(&scoped, Msg::GALLERY_TF_CAPTION_FLUENT_DISABLED);
        let control = kit.text_box(
            ui_widgets_fluent::TextBoxParams::new(Id::str("fluent/text-field-off"), "Ada Lovelace")
                .header(&header)
                .disabled(true),
            &mut scoped,
        );
        fcol = fcol.child(tf_block_fluent(2, caption, scoped.element(control)));
    }
    row = row.child(column(
        Page::TextField,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(fcol),
        COL_TALL_H,
        cx,
    ));
    // ── The glass column: a live field, its search variant and a still one.
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let mut glass_col = Container::new(Id::of(("body", "glass")), NodeKind::Column).gap(14.0);
    let placeholder = tr(cx, Msg::GALLERY_TF_PLACEHOLDER);
    let caption = tr(cx, Msg::GALLERY_TF_CAPTION_GLASS);
    let field = GlassTextField::new(
        GlassTextFieldParams::new(glass_text_field_id(), glass, TF_W)
            .placeholder(&placeholder)
            .leading(sf_glyph("magnifyingglass")),
        cx,
        &pack,
    );
    glass_col = glass_col.child(Box::new(captioned_row(
        glass_text_field_id().with("cap"),
        cx.element(Box::new(field)),
        caption,
    )));
    let search_placeholder = tr(cx, Msg::GALLERY_TF_SEARCH);
    let caption = tr(cx, Msg::GALLERY_TF_CAPTION_GLASS_SEARCH);
    let search = GlassTextField::new(
        GlassTextFieldParams::search(glass_search_field_id(), "", TF_W)
            .placeholder(&search_placeholder)
            .leading(sf_glyph("magnifyingglass"))
            .trailing(sf_glyph("xmark_circle_fill")),
        cx,
        &pack,
    );
    glass_col = glass_col.child(Box::new(captioned_row(
        glass_search_field_id().with("cap"),
        cx.element(Box::new(search)),
        caption,
    )));
    let caption = tr(cx, Msg::GALLERY_TF_CAPTION_GLASS_TEXT);
    let still = GlassTextField::new(
        GlassTextFieldParams::new(glass_still_field_id(), "Ada Lovelace", TF_W)
            .leading(sf_glyph("person_crop_circle"))
            .disabled(true),
        cx,
        &pack,
    );
    glass_col = glass_col.child(Box::new(captioned_row(
        glass_still_field_id().with("cap"),
        cx.element(Box::new(still)),
        caption,
    )));
    let caption = tr(cx, Msg::GALLERY_TF_CAPTION_GLASS_SECURE);
    let secure_placeholder = tr(cx, Msg::GALLERY_TF_SECURE_PLACEHOLDER);
    let secret = GlassTextField::new(
        GlassTextFieldParams::new(glass_secure_field_id(), secure, TF_W)
            .placeholder(&secure_placeholder)
            .leading(sf_glyph("lock"))
            .secure(true),
        cx,
        &pack,
    );
    glass_col = glass_col.child(Box::new(captioned_row(
        glass_secure_field_id().with("cap"),
        cx.element(Box::new(secret)),
        caption,
    )));
    row = row.child(column(
        Page::TextField,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        Box::new(glass_col),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
