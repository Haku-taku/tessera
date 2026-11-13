// The buttons page.

use ui_widgets_glass::{GlassButton, GlassButtonParams, GlassButtonStyle, HigOptions};

use crate::pages::glyphs::sf_glyph;
use crate::shell::*;

/// How many toggle buttons the Buttons page shows; the app owns each one's `selected`.
pub(crate) const BTN_TOGGLES: usize = 2;

pub(crate) fn btn_toggle_id(k: usize) -> Id {
    Id::of(("btntoggle", k))
}

/// Tall enough for the split button's two sample rows under the button families; the shared
/// `COL_TALL_H` leaves the last rows outside the panel.
pub(crate) const COL_BUTTONS_H: f32 = 1400.0;

/// The split button the app drives, and the one sample per size below it.
pub(crate) fn btn_split_id() -> Id {
    Id::of(("btnsplit",))
}

pub(crate) fn btn_split_sample_id(k: usize) -> Id {
    Id::of(("btnsplit", "sample", k))
}

/// The glass icon buttons: filled, prominent and transparent, in that order.
pub(crate) fn glass_btn_id(k: usize) -> Id {
    Id::of(("btnglass", k))
}

/// The glass labelled capsules.
pub(crate) fn glass_label_btn_id(k: usize) -> Id {
    Id::of(("btnglasslabel", k))
}

/// How many glass buttons report: three icon buttons and two capsules.
pub(crate) const GLASS_ICON_BTNS: usize = 3;
pub(crate) const GLASS_LABEL_BTNS: usize = 2;

/// One button sample: everything that can only be chosen when it is built.
pub(crate) struct BtnSample {
    id: Id,
    label: Msg,
    style: ui_widgets_md3::ButtonStyle,
    size: ui_widgets_md3::ButtonSize,
    /// `None` is a plain button; `Some` makes it a toggle in that state.
    toggle: Option<bool>,
    icon: bool,
    disabled: bool,
}

impl BtnSample {
    fn new(
        id: Id,
        label: Msg,
        style: ui_widgets_md3::ButtonStyle,
        size: ui_widgets_md3::ButtonSize,
    ) -> Self {
        Self {
            id,
            label,
            style,
            size,
            toggle: None,
            icon: false,
            disabled: false,
        }
    }

    fn icon(mut self) -> Self {
        self.icon = true;
        self
    }

    fn toggle(mut self, on: bool) -> Self {
        self.toggle = Some(on);
        self
    }

    fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// A caption above a row of buttons.
pub(crate) fn button_line(
    id: Id,
    caption: Msg,
    samples: &[BtnSample],
    style: &Md3Kit,
    cx: &mut Cx<'_, '_>,
) -> Container {
    // `captioned_row` owns `id.with("row")`; this inner row needs a salt of its own.
    let mut row = Container::new(id.with("btns"), NodeKind::Row).gap(8.0);
    for s in samples {
        let env = if s.disabled {
            disabled_env()
        } else {
            style_env("md3")
        };
        let mut scoped = cx.at_scope(env);
        let spec = ui_widgets_md3::ButtonParams::new(s.id, tr(&scoped, s.label))
            .style(s.style)
            .size(s.size);
        let spec = match s.icon.then_some(ms("add")) {
            Some(mark) => spec.icon(mark),
            None => spec,
        };
        let control = match s.toggle {
            Some(on) => style.toggle_button(spec, on, &mut scoped),
            None => style.button(spec, &mut scoped),
        };
        row = row.child(scoped.element(control));
    }
    captioned_row(id, Box::new(row), tr(cx, caption))
}

/// The button's construction-time variants: style, size, icon, selection and disabled state.
pub(crate) fn buttons_page(
    toggled: [bool; BTN_TOGGLES],
    split_open: bool,
    split_presses: u32,
    glass_press: u32,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    use ui_widgets_md3::{ButtonSize as S, ButtonStyle as T};
    let mut row = Container::new(Id::of(("bodyrow", Page::Buttons.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let bid = |tag: &'static str| Id::of(("btn", Page::Buttons.tag(), tag));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(10.0);
    col = col.child(Box::new(button_line(
        bid("styles"),
        Msg::GALLERY_BUTTONS_CAPTION_STYLES,
        &[
            BtnSample::new(bid("filled"), Msg::GALLERY_BTN_FILLED, T::Filled, S::Small),
            BtnSample::new(bid("tonal"), Msg::GALLERY_BTN_TONAL, T::Tonal, S::Small),
            BtnSample::new(
                bid("elevated"),
                Msg::GALLERY_BTN_ELEVATED,
                T::Elevated,
                S::Small,
            ),
        ],
        &style,
        cx,
    )));
    col = col.child(Box::new(button_line(
        bid("styles2"),
        Msg::GALLERY_BUTTONS_CAPTION_STYLES2,
        &[
            BtnSample::new(
                bid("outlined"),
                Msg::GALLERY_BTN_OUTLINED,
                T::Outlined,
                S::Small,
            ),
            BtnSample::new(bid("text"), Msg::GALLERY_BTN_TEXT, T::Text, S::Small),
        ],
        &style,
        cx,
    )));
    col = col.child(Box::new(button_line(
        bid("sizes"),
        Msg::GALLERY_BUTTONS_CAPTION_SIZES,
        &[
            BtnSample::new(bid("xs"), Msg::GALLERY_BTN_XS, T::Filled, S::XSmall),
            BtnSample::new(bid("sm"), Msg::GALLERY_BTN_SMALL, T::Filled, S::Small),
            BtnSample::new(bid("md"), Msg::GALLERY_BTN_MEDIUM, T::Filled, S::Medium),
        ],
        &style,
        cx,
    )));
    col = col.child(Box::new(button_line(
        bid("sizes2"),
        Msg::GALLERY_BUTTONS_CAPTION_SIZES2,
        &[
            BtnSample::new(bid("lg"), Msg::GALLERY_BTN_LARGE, T::Filled, S::Large),
            BtnSample::new(bid("xl"), Msg::GALLERY_BTN_XL, T::Filled, S::XLarge),
        ],
        &style,
        cx,
    )));
    col = col.child(Box::new(button_line(
        bid("icons"),
        Msg::GALLERY_BUTTONS_CAPTION_ICONS,
        &[
            BtnSample::new(
                bid("icon-filled"),
                Msg::GALLERY_BTN_FILLED,
                T::Filled,
                S::Small,
            )
            .icon(),
            BtnSample::new(
                bid("icon-tonal"),
                Msg::GALLERY_BTN_TONAL,
                T::Tonal,
                S::Small,
            )
            .icon(),
            BtnSample::new(
                bid("icon-outlined"),
                Msg::GALLERY_BTN_OUTLINED,
                T::Outlined,
                S::Small,
            )
            .icon(),
        ],
        &style,
        cx,
    )));
    col = col.child(Box::new(button_line(
        bid("toggles"),
        Msg::GALLERY_BUTTONS_CAPTION_TOGGLES,
        &[
            BtnSample::new(
                btn_toggle_id(0),
                Msg::GALLERY_BTN_TOGGLE_A,
                T::Filled,
                S::Medium,
            )
            .toggle(toggled[0]),
            BtnSample::new(
                btn_toggle_id(1),
                Msg::GALLERY_BTN_TOGGLE_B,
                T::Outlined,
                S::Medium,
            )
            .toggle(toggled[1]),
        ],
        &style,
        cx,
    )));
    let undo = tr(cx, Msg::GALLERY_BTN_UNDO);
    let redo = tr(cx, Msg::GALLERY_BTN_REDO);
    let copy = tr(cx, Msg::GALLERY_BTN_COPY);
    let group_caption = tr(cx, Msg::GALLERY_BUTTONS_CAPTION_GROUP);
    let mut scoped = cx.at_scope(style_env("md3"));
    let group = style.button_group(
        ui_widgets_md3::ButtonGroupParams::new(
            bid("group-a"),
            vec![
                ui_widgets_md3::ButtonParams::new(bid("group-a1"), undo),
                ui_widgets_md3::ButtonParams::new(bid("group-a2"), redo),
                ui_widgets_md3::ButtonParams::new(bid("group-a3"), copy),
            ],
        ),
        &mut scoped,
    );
    col = col.child(Box::new(captioned_row(
        bid("group"),
        scoped.element(group),
        group_caption,
    )));
    col = col.child(Box::new(button_line(
        bid("disabled"),
        Msg::GALLERY_BUTTONS_CAPTION_DISABLED,
        &[
            BtnSample::new(
                bid("dis-filled"),
                Msg::GALLERY_BTN_FILLED,
                T::Filled,
                S::Small,
            )
            .disabled(),
            BtnSample::new(bid("dis-tonal"), Msg::GALLERY_BTN_TONAL, T::Tonal, S::Small).disabled(),
            BtnSample::new(
                bid("dis-elevated"),
                Msg::GALLERY_BTN_ELEVATED,
                T::Elevated,
                S::Small,
            )
            .disabled(),
        ],
        &style,
        cx,
    )));
    col = col.child(Box::new(button_line(
        bid("disabled2"),
        Msg::GALLERY_BUTTONS_CAPTION_DISABLED2,
        &[
            BtnSample::new(
                bid("dis-outlined"),
                Msg::GALLERY_BTN_OUTLINED,
                T::Outlined,
                S::Small,
            )
            .disabled(),
            BtnSample::new(bid("dis-text"), Msg::GALLERY_BTN_TEXT, T::Text, S::Small).disabled(),
        ],
        &style,
        cx,
    )));
    // The split button: the trailing half owns the menu, the leading half the action.
    let split_label = tr(cx, Msg::GALLERY_SPLIT_BUTTON_LABEL);
    let split_caption = tr(cx, Msg::GALLERY_SPLIT_BUTTON_CAPTION);
    let split_sizes_caption = tr(cx, Msg::GALLERY_SPLIT_BUTTON_SIZES_CAPTION);
    let split_readout = tf(
        cx,
        Msg::GALLERY_SPLIT_BUTTON_READOUT,
        &[
            ("open", arg(format!("{split_open}"))),
            ("count", arg(split_presses.to_string())),
        ],
    );
    let pack = ui_widgets_md3::pack_of(ui_style::StyleScope::of(cx.env()).appearance);
    let mut split_scope = cx.at_scope(style_env("md3"));
    let split = ui_widgets_md3::Md3SplitButton::new_with(
        ui_widgets_md3::SplitButtonParams::new(btn_split_id(), split_label.clone())
            .leading(ui_widgets::IconSource::glyph(
                "share",
                ui_widgets::IconSet::Outlined,
            ))
            .selected(split_open),
        &mut split_scope,
        pack,
    );
    col = col.child(Box::new(captioned_row(
        bid("split"),
        split_scope.element(Box::new(split)),
        split_caption,
    )));
    col = col.child_decl(label(
        Id::str("buttons/split-readout"),
        split_readout,
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    // One per line: the xlarge pill alone is wider than the 380dp column's remaining space.
    let mut samples = Container::new(bid("split-sizes"), NodeKind::Column)
        .gap(8.0)
        .cross_align(0.0);
    for (k, size) in [
        ui_widgets_md3::SplitButtonSize::XSmall,
        ui_widgets_md3::SplitButtonSize::Small,
        ui_widgets_md3::SplitButtonSize::Medium,
        ui_widgets_md3::SplitButtonSize::Large,
        ui_widgets_md3::SplitButtonSize::XLarge,
    ]
    .into_iter()
    .enumerate()
    {
        let params =
            ui_widgets_md3::SplitButtonParams::new(btn_split_sample_id(k), split_label.clone())
                .size(size);
        let control = ui_widgets_md3::Md3SplitButton::new_with(params, &mut split_scope, pack);
        samples = samples.child(split_scope.element(Box::new(control)));
    }
    col = col.child(Box::new(captioned_row(
        bid("split-sizes-row"),
        Box::new(samples),
        split_sizes_caption,
    )));
    col = col.child_decl(label(
        Id::str("buttons/readout"),
        tf(
            cx,
            Msg::GALLERY_BUTTONS_READOUT,
            &[("sel", arg(format!("{toggled:?}")))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Buttons,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_BUTTONS_H,
        cx,
    ));
    // WinUI's three `<Style x:Key>` families: the same geometry, three brush tables.
    use ui_widgets_fluent::{ButtonParams as FParams, ButtonVariant as FVariant};
    let kit = ui_widgets_fluent::FluentKit::new();
    let fid = |tag: &'static str| Id::of(("fbtn", Page::Buttons.tag(), tag));
    let mut fcol = Container::new(Id::of(("body", "fluent")), NodeKind::Column).gap(10.0);
    let mut frow = Container::new(fid("styles"), NodeKind::Row).gap(8.0);
    for (tag, label, variant) in [
        ("default", Msg::GALLERY_BTN_DEFAULT, FVariant::Default),
        ("accent", Msg::GALLERY_BTN_ACCENT, FVariant::Accent),
        ("subtle", Msg::GALLERY_BTN_SUBTLE, FVariant::Subtle),
        ("textlink", Msg::GALLERY_BTN_TEXT_LINK, FVariant::TextLink),
    ] {
        let mut scoped = cx.at_scope(style_env("fluent"));
        let params = FParams::new(fid(tag), tr(&scoped, label)).variant(variant);
        let control = kit.button(params, &mut scoped);
        frow = frow.child(scoped.element(control));
    }
    fcol = fcol.child(Box::new(captioned_row(
        fid("styles"),
        Box::new(frow),
        tr(cx, Msg::GALLERY_BUTTONS_CAPTION_FLUENT_STYLES),
    )));
    let mut frow = Container::new(fid("disabled"), NodeKind::Row).gap(8.0);
    for (tag, label, variant) in [
        ("dis-default", Msg::GALLERY_BTN_DEFAULT, FVariant::Default),
        ("dis-accent", Msg::GALLERY_BTN_ACCENT, FVariant::Accent),
        (
            "dis-textlink",
            Msg::GALLERY_BTN_TEXT_LINK,
            FVariant::TextLink,
        ),
    ] {
        let mut scoped = cx.at_scope(style_env("fluent"));
        let params = FParams::new(fid(tag), tr(&scoped, label))
            .variant(variant)
            .disabled();
        let control = kit.button(params, &mut scoped);
        frow = frow.child(scoped.element(control));
    }
    fcol = fcol.child(Box::new(captioned_row(
        fid("disabled"),
        Box::new(frow),
        tr(cx, Msg::GALLERY_BUTTONS_CAPTION_FLUENT_DISABLED),
    )));
    row = row.child(column(
        Page::Buttons,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(fcol),
        COL_BUTTONS_H,
        cx,
    ));
    row = row.child(glass_buttons_column(glass_press, cx));
    Box::new(row)
}

/// The glass buttons: an icon capsule that inflates under the finger, and a labelled one.
fn glass_buttons_column(glass_press: u32, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut scoped = cx.at_scope(style_env("glass"));
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let gid = |tag: &'static str| Id::of(("btn", Page::Buttons.tag(), "glass", tag));

    let mut icons = Container::new(gid("styles").with("btns"), NodeKind::Row).gap(12.0);
    for (k, style) in [
        GlassButtonStyle::Filled,
        GlassButtonStyle::Prominent,
        GlassButtonStyle::Transparent,
    ]
    .into_iter()
    .enumerate()
    {
        let id = glass_btn_id(k);
        let params = GlassButtonParams::icon(id, sf_glyph("plus")).style(style);
        let mut node = scoped.at(id);
        let button = GlassButton::new_with(HigOptions::default(), params, &mut node, &pack);
        icons = icons.child(scoped.element(Box::new(button)));
    }

    let mut labels = Container::new(gid("labels").with("btns"), NodeKind::Row).gap(12.0);
    for (k, (msg, style)) in [
        (Msg::GALLERY_BTN_FILLED, GlassButtonStyle::Filled),
        (Msg::GALLERY_BTN_TONAL, GlassButtonStyle::Prominent),
    ]
    .into_iter()
    .enumerate()
    {
        let text = tr(&scoped, msg);
        let id = glass_label_btn_id(k);
        let params = GlassButtonParams::labelled(id, None, &text).style(style);
        let mut node = scoped.at(id);
        let button = GlassButton::new_with(HigOptions::default(), params, &mut node, &pack);
        labels = labels.child(scoped.element(Box::new(button)));
    }

    let disabled_id = gid("disabled");
    let disabled = {
        let params = GlassButtonParams::icon(disabled_id, sf_glyph("plus")).disabled();
        let mut node = scoped.at(disabled_id);
        GlassButton::new_with(HigOptions::default(), params, &mut node, &pack)
    };

    let readout = tf(
        &scoped,
        Msg::GALLERY_BUTTONS_GLASS_READOUT,
        &[("count", arg(format!("{glass_press}")))],
    );
    let body = Box::new(
        Container::new(Id::of(("body", "glass")), NodeKind::Column)
            .gap(10.0)
            .child(Box::new(captioned_row(
                gid("styles"),
                Box::new(icons),
                tr(&scoped, Msg::GALLERY_BUTTONS_CAPTION_GLASS_ICONS),
            )))
            .child(Box::new(captioned_row(
                gid("labels"),
                Box::new(labels),
                tr(&scoped, Msg::GALLERY_BUTTONS_CAPTION_GLASS_LABELS),
            )))
            .child(Box::new(captioned_row(
                gid("disabled"),
                scoped.element(Box::new(disabled)),
                tr(&scoped, Msg::GALLERY_BUTTONS_CAPTION_GLASS_DISABLED),
            )))
            .child_decl(label(gid("readout"), readout, 12.0, muted(), COL_W - 24.0)),
    );
    column(
        Page::Buttons,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        body,
        COL_BUTTONS_H,
        &scoped,
    )
}
