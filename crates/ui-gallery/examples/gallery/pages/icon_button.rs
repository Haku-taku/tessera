// The icon button page: the four styles, the five sizes, the three widths and the toggles.

use crate::shell::*;

/// The four styles in the order the export lists them.
pub(crate) const ICON_BUTTON_STYLES: [ui_widgets_md3::IconButtonStyle; 4] = [
    ui_widgets_md3::IconButtonStyle::Standard,
    ui_widgets_md3::IconButtonStyle::Filled,
    ui_widgets_md3::IconButtonStyle::Tonal,
    ui_widgets_md3::IconButtonStyle::Outlined,
];

/// The five sizes, with the mark each one draws.
pub(crate) const ICON_BUTTON_SIZES: [ui_widgets_md3::IconButtonSize; 5] = [
    ui_widgets_md3::IconButtonSize::XSmall,
    ui_widgets_md3::IconButtonSize::Small,
    ui_widgets_md3::IconButtonSize::Medium,
    ui_widgets_md3::IconButtonSize::Large,
    ui_widgets_md3::IconButtonSize::XLarge,
];

/// The three widths, all at the medium size.
pub(crate) const ICON_BUTTON_WIDTHS: [ui_widgets_md3::IconButtonWidth; 3] = [
    ui_widgets_md3::IconButtonWidth::Narrow,
    ui_widgets_md3::IconButtonWidth::Default,
    ui_widgets_md3::IconButtonWidth::Wide,
];

pub(crate) fn icon_button_style_id(k: usize) -> Id {
    Id::of(("icon-button/style", k))
}

pub(crate) fn icon_button_size_id(k: usize) -> Id {
    Id::of(("icon-button/size", k))
}

pub(crate) fn icon_button_width_id(k: usize) -> Id {
    Id::of(("icon-button/width", k))
}

pub(crate) fn icon_button_toggle_id(k: usize) -> Id {
    Id::of(("icon-button/toggle", k))
}

pub(crate) fn icon_button_disabled_id() -> Id {
    Id::str("icon-button/disabled")
}

/// One sample per row, built from `params` and laid out in `row_id`.
fn sample_row(
    row_id: Id,
    items: Vec<ui_widgets_md3::IconButtonParams>,
    scoped: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let appearance = ui_style::StyleScope::of(scoped.env()).appearance;
    let pack = ui_widgets_md3::pack_of(appearance);
    let mut row = Container::new(row_id, NodeKind::Row)
        .gap(16.0)
        .cross_align(0.5);
    for params in items {
        let button = ui_widgets_md3::Md3IconButton::new_with(params, scoped, pack);
        row = row.child(scoped.element(Box::new(button)));
    }
    Box::new(row)
}

/// A caption line above a sample row.
fn caption(id: Id, text: String) -> Decl {
    label(id, text, 11.0, muted(), COL_W - 24.0)
}

/// MD3 icon buttons: the axes side by side, and four toggles the app remembers.
pub(crate) fn icon_button_page(
    toggles: [bool; ICON_BUTTON_STYLES.len()],
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::IconButton.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(6.0);

    let styles: Vec<ui_widgets_md3::IconButtonParams> = ICON_BUTTON_STYLES
        .iter()
        .enumerate()
        .map(|(k, style)| {
            ui_widgets_md3::IconButtonParams::new(icon_button_style_id(k), ms("favorite"))
                .style(*style)
        })
        .collect();
    col = col
        .child_decl(caption(
            Id::str("icon-button/styles-caption"),
            tr(&scoped, Msg::GALLERY_ICON_BUTTON_STYLES),
        ))
        .child(sample_row(
            Id::str("icon-button/styles"),
            styles,
            &mut scoped,
        ));

    let sizes: Vec<ui_widgets_md3::IconButtonParams> = ICON_BUTTON_SIZES
        .iter()
        .enumerate()
        .map(|(k, size)| {
            ui_widgets_md3::IconButtonParams::new(icon_button_size_id(k), ms("bookmark"))
                .style(ui_widgets_md3::IconButtonStyle::Filled)
                .size(*size)
        })
        .collect();
    col = col
        .child_decl(caption(
            Id::str("icon-button/sizes-caption"),
            tr(&scoped, Msg::GALLERY_ICON_BUTTON_SIZES),
        ))
        .child(sample_row(Id::str("icon-button/sizes"), sizes, &mut scoped));

    let widths: Vec<ui_widgets_md3::IconButtonParams> = ICON_BUTTON_WIDTHS
        .iter()
        .enumerate()
        .map(|(k, width)| {
            ui_widgets_md3::IconButtonParams::new(icon_button_width_id(k), ms("star"))
                .style(ui_widgets_md3::IconButtonStyle::Tonal)
                .size(ui_widgets_md3::IconButtonSize::Medium)
                .width(*width)
        })
        .collect();
    col = col
        .child_decl(caption(
            Id::str("icon-button/widths-caption"),
            tr(&scoped, Msg::GALLERY_ICON_BUTTON_WIDTHS),
        ))
        .child(sample_row(
            Id::str("icon-button/widths"),
            widths,
            &mut scoped,
        ));

    let labels: Vec<String> = ICON_BUTTON_STYLES
        .iter()
        .enumerate()
        .filter(|(k, _)| toggles[*k])
        .map(|(_, style)| style_name(*style).to_string())
        .collect();
    let on = if labels.is_empty() {
        tr(&scoped, Msg::GALLERY_ICON_BUTTON_NONE)
    } else {
        labels.join(", ")
    };
    let readout = tf(
        &scoped,
        Msg::GALLERY_ICON_BUTTON_READOUT,
        &[("on", arg(on))],
    );
    col = col.child_decl(label(
        Id::str("icon-button/readout"),
        readout,
        12.0,
        muted(),
        COL_W - 24.0,
    ));

    row = row.child(column_w(
        Page::IconButton,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        800.0,
        520.0,
        &scoped,
    ));

    let mut side = Container::new(Id::of(("body", "md3-toggle")), NodeKind::Column).gap(6.0);
    let toggles_row: Vec<ui_widgets_md3::IconButtonParams> = ICON_BUTTON_STYLES
        .iter()
        .enumerate()
        .map(|(k, style)| {
            ui_widgets_md3::IconButtonParams::new(icon_button_toggle_id(k), ms("favorite"))
                .style(*style)
                .size(ui_widgets_md3::IconButtonSize::Medium)
                .selected(toggles[k])
        })
        .collect();
    side = side
        .child_decl(caption(
            Id::str("icon-button/toggles-caption"),
            tr(&scoped, Msg::GALLERY_ICON_BUTTON_TOGGLES),
        ))
        .child(sample_row(
            Id::str("icon-button/toggles"),
            toggles_row,
            &mut scoped,
        ));
    let disabled = ui_widgets_md3::IconButtonParams::new(icon_button_disabled_id(), ms("delete"))
        .style(ui_widgets_md3::IconButtonStyle::Filled)
        .size(ui_widgets_md3::IconButtonSize::Medium)
        .disabled(true);
    side = side
        .child_decl(caption(
            Id::str("icon-button/disabled-caption"),
            tr(&scoped, Msg::GALLERY_ICON_BUTTON_DISABLED),
        ))
        .child(sample_row(
            Id::str("icon-button/disabled"),
            vec![disabled],
            &mut scoped,
        ));
    row = row.child(column_w(
        Page::IconButton,
        "md3-toggle",
        Msg::GALLERY_ICON_BUTTON_TOGGLES_TITLE,
        Box::new(side),
        COL_W,
        520.0,
        &scoped,
    ));
    Box::new(row)
}

/// The style's name as the captions write it.
pub(crate) fn style_name(style: ui_widgets_md3::IconButtonStyle) -> &'static str {
    use ui_widgets_md3::IconButtonStyle;
    match style {
        IconButtonStyle::Standard => "standard",
        IconButtonStyle::Filled => "filled",
        IconButtonStyle::Tonal => "tonal",
        IconButtonStyle::Outlined => "outlined",
        IconButtonStyle::Base => "base",
    }
}
