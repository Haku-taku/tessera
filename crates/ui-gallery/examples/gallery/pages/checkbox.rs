// The checkbox page.

use crate::shell::*;

/// How many checkbox variants the Checkbox page shows, and their build-time knobs
/// (`error`, `disabled`). The checked state is live, one flag per box.
pub(crate) const CHECKBOXES: usize = 6;
pub(crate) const CHECKBOX_VARIANTS: [(bool, bool); CHECKBOXES] = [
    (false, false),
    (false, false),
    (true, false),
    (true, false),
    (false, true),
    (false, true),
];

pub(crate) fn checkbox_id(k: usize) -> Id {
    Id::of(("checkbox", k))
}

/// The Fluent column's boxes; each one's value lives in the app.
pub(crate) fn fluent_checkbox_id(k: usize) -> Id {
    Id::of(("fcheckbox", k))
}

/// The three group buttons that write the whole MD3 column at once; a click on a mixed MD3
/// box settles it to checked, as material-web does, so the group is how mixed comes back.
pub(crate) fn checkbox_group_id(tag: &'static str) -> Id {
    Id::of(("checkbox-group", tag))
}

pub(crate) const CHECKBOX_GROUP: [(&str, Msg); 3] = [
    ("off", Msg::GALLERY_CHECKBOX_ALL_OFF),
    ("mixed", Msg::GALLERY_CHECKBOX_ALL_MIXED),
    ("on", Msg::GALLERY_CHECKBOX_ALL_ON),
];

/// The MD3 checkbox's variants. `error` and `disabled` are build-time knobs, so each box is
/// its own instance; the checked state is live and a click flips just that box.
pub(crate) fn checkbox_page(
    checked: [ui_widgets_md3::CheckState; CHECKBOXES],
    fchecked: [ui_widgets_fluent::CheckState; 3],
    ftri: ui_widgets_fluent::CheckState,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Checkbox.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(6.0);
    for (k, (error, disabled)) in CHECKBOX_VARIANTS.iter().enumerate() {
        let id = checkbox_id(k);
        let mut params = ui_widgets_md3::Md3Options::default();
        params.disabled = *disabled;
        // The pack resolves from the scope's appearance, so one instance serves both themes.
        let style = Md3Kit::with_scope(ui_style::StyleScope::default(), params);
        let mut scoped = cx.at_scope(Env::with(Some(root_env()), params));
        let control = style.checkbox_tri(id, checked[k], *error, &mut scoped);
        let caption = tf(
            &scoped,
            Msg::GALLERY_CHECKBOX_CAPTION,
            &[
                (
                    "state",
                    arg(match checked[k] {
                        ui_widgets_md3::CheckState::Off => "off",
                        ui_widgets_md3::CheckState::On => "on",
                        ui_widgets_md3::CheckState::Mixed => "mixed",
                    }),
                ),
                ("error", arg(if *error { "yes" } else { "no" })),
                ("disabled", arg(if *disabled { "yes" } else { "no" })),
            ],
        );
        let line = Container::new(id.with("row"), NodeKind::Row)
            .gap(10.0)
            .child(scoped.element(control))
            .child_decl(label(id.with("cap"), caption, 12.0, muted(), 220.0));
        col = col.child(Box::new(line));
    }
    let mut group = Container::new(Id::str("checkbox/group"), NodeKind::Row).gap(8.0);
    let group_style = Md3Kit::new();
    for (tag, msg) in CHECKBOX_GROUP {
        // The label is read through `cx` before the scoped context takes its mutable borrow.
        let label = tr(cx, msg);
        let mut scoped = cx.at_scope(style_env("md3"));
        let control = group_style.button(
            ui_widgets_md3::ButtonParams::new(checkbox_group_id(tag), label)
                .style(ui_widgets_md3::ButtonStyle::Outlined)
                .size(ui_widgets_md3::ButtonSize::Small),
            &mut scoped,
        );
        group = group.child(scoped.element(control));
    }
    col = col.child(Box::new(group));
    // MD3 defines no size or shape variants for the checkbox: the export has one geometry.
    col = col.child_decl(label(
        Id::str("checkbox/note"),
        tr(cx, Msg::GALLERY_CHECKBOX_NOTE),
        11.0,
        muted(),
        320.0,
    ));
    row = row.child(column(
        Page::Checkbox,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_TALL_H,
        cx,
    ));
    // WinUI's three value states; each box's value belongs to the app, as the template's does.
    let kit = ui_widgets_fluent::FluentKit::new();
    let mut fcol = Container::new(Id::of(("body", "fluent")), NodeKind::Column).gap(6.0);
    let labels = [
        Msg::GALLERY_CHECKBOX_LABEL_OFF,
        Msg::GALLERY_CHECKBOX_LABEL_ON,
        Msg::GALLERY_CHECKBOX_LABEL_MIXED,
    ];
    for (k, state) in fchecked.iter().enumerate() {
        let id = fluent_checkbox_id(k);
        let mut scoped = cx.at_scope(style_env("fluent"));
        let text = tr(&scoped, labels[k]);
        let params = ui_widgets_fluent::CheckBoxParams::new(id, text, *state);
        let control = kit.checkbox(params, &mut scoped);
        let caption = tf(
            &scoped,
            Msg::GALLERY_CHECKBOX_CAPTION,
            &[
                (
                    "state",
                    arg(match state {
                        ui_widgets_fluent::CheckState::Unchecked => "off",
                        ui_widgets_fluent::CheckState::Checked => "on",
                        ui_widgets_fluent::CheckState::Indeterminate => "mixed",
                    }),
                ),
                ("error", arg("no")),
                ("disabled", arg("no")),
            ],
        );
        let line = Container::new(id.with("row"), NodeKind::Row)
            .gap(10.0)
            .child(scoped.element(control))
            .child_decl(label(id.with("cap"), caption, 12.0, muted(), 220.0));
        fcol = fcol.child(Box::new(line));
    }
    let mut scoped = cx.at_scope(style_env("fluent"));
    let text = tr(&scoped, Msg::GALLERY_CHECKBOX_LABEL_ON);
    let params = ui_widgets_fluent::CheckBoxParams::new(
        fluent_checkbox_id(3),
        text,
        ui_widgets_fluent::CheckState::Checked,
    )
    .disabled();
    let control = kit.checkbox(params, &mut scoped);
    fcol = fcol.child(scoped.element(control));
    // `IsThreeState`: the only way a click can reach the indeterminate value.
    let mut scoped = cx.at_scope(style_env("fluent"));
    let text = tr(&scoped, Msg::GALLERY_CHECKBOX_LABEL_TRI);
    let params =
        ui_widgets_fluent::CheckBoxParams::new(fluent_checkbox_id(4), text, ftri).three_state();
    let control = kit.checkbox(params, &mut scoped);
    fcol = fcol.child(scoped.element(control));
    row = row.child(column(
        Page::Checkbox,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(fcol),
        COL_TALL_H,
        cx,
    ));
    row = row.child(pending_column(
        Page::Checkbox,
        "glass",
        COL_W,
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
