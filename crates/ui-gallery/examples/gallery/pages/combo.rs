// The Fluent combo box page.

use ui_widgets_fluent::{ComboBoxItem, ComboBoxParams};

use crate::app::Gallery;
use crate::shell::*;

pub(crate) const COMBO_ITEMS: usize = 5;

pub(crate) const COMBO_LABELS: [Msg; COMBO_ITEMS] = [
    Msg::GALLERY_COMBO_ITEM_0,
    Msg::GALLERY_COMBO_ITEM_1,
    Msg::GALLERY_COMBO_ITEM_2,
    Msg::GALLERY_COMBO_ITEM_3,
    Msg::GALLERY_COMBO_ITEM_4,
];

pub(crate) fn combo_id() -> Id {
    Id::of("combo")
}

pub(crate) fn combo_item_id(j: usize) -> Id {
    combo_id().with(("item", j))
}

pub(crate) fn combo_placeholder_id() -> Id {
    Id::of(("combo", "placeholder"))
}

pub(crate) fn combo_disabled_id() -> Id {
    Id::of(("combo", "disabled"))
}

/// The field's width, so the panel has a width to match rather than the page's.
const COMBO_W: f32 = 260.0;

/// One live field with its header, plus the placeholder and disabled states side by side.
pub(crate) fn combo_page(g: &Gallery, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut body = Container::new(Id::of(("combo", "body")), NodeKind::Column).gap(12.0);
    body = body.child_decl(label(
        Id::of(("combo", "caption")),
        tr(cx, Msg::GALLERY_COMBO_CAPTION),
        12.0,
        muted(),
        700.0,
    ));

    let labels: [String; COMBO_ITEMS] = std::array::from_fn(|j| tr(cx, COMBO_LABELS[j]));
    let items: Vec<ComboBoxItem> = (0..COMBO_ITEMS)
        .map(|j| ComboBoxItem::new(combo_item_id(j), labels[j].clone()))
        .collect();

    // A row keeps each field at its own width; a column would stretch them across the page.
    let mut states = Container::new(Id::of(("combo", "states")), NodeKind::Row).gap(24.0);
    let mut scoped = cx.at_scope(style_env("fluent"));
    let params = ComboBoxParams::new(combo_placeholder_id(), &items)
        .placeholder(tr(&scoped, Msg::GALLERY_COMBO_PLACEHOLDER))
        .width(COMBO_W);
    let control = g.fluent.combo_box(params, &mut scoped);
    states = states.child(scoped.element(control));
    let mut scoped = cx.at_scope(style_env("fluent"));
    let params = ComboBoxParams::new(combo_disabled_id(), &items)
        .selected(Some(3))
        .disabled()
        .width(COMBO_W);
    let control = g.fluent.combo_box(params, &mut scoped);
    states = states.child(scoped.element(control));
    body = body.child(Box::new(states));

    // The live field comes last: its panel unfolds into the empty half of the column
    // instead of covering the two static samples.
    let mut scoped = cx.at_scope(style_env("fluent"));
    let params = ComboBoxParams::new(combo_id(), &items)
        .header(tr(&scoped, Msg::GALLERY_COMBO_HEADER))
        .selected(g.combo_sel)
        .open(g.combo_open)
        .width(COMBO_W);
    let control = g.fluent.combo_box(params, &mut scoped);
    body = body.child(scoped.element(control));

    let readout = match g.combo_sel {
        Some(j) => tf(
            cx,
            Msg::GALLERY_COMBO_READOUT_PICKED,
            &[("item", arg(labels[j].clone()))],
        ),
        None => tr(cx, Msg::GALLERY_COMBO_READOUT_NONE),
    };
    body = body.child_decl(label(
        Id::of(("combo", "readout")),
        readout,
        12.0,
        muted(),
        700.0,
    ));

    let mut row = Container::new(Id::of(("bodyrow", Page::Combo.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    row = row.child(column_w(
        Page::Combo,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(body),
        760.0,
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
