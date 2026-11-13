// The radio page.

use crate::shell::*;

/// How many options the demo radio group has, plus two standalone disabled samples.
pub(crate) const RADIOS: usize = 3;

pub(crate) fn radio_id(k: usize) -> Id {
    Id::of(("radio", k))
}

pub(crate) fn radio_extra_id(k: usize) -> Id {
    Id::of(("radio-extra", k))
}

/// The Fluent column's own group; its selection is the app's, like the MD3 one.
pub(crate) fn fluent_radio_id(k: usize) -> Id {
    Id::of(("fradio", k))
}

/// The MD3 radio: a group whose selection the app owns, plus the two disabled appearances.
pub(crate) fn radio_page(sel: usize, fsel: usize, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Radio.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(6.0);
    for k in 0..RADIOS {
        let id = radio_id(k);
        let caption = tf(
            cx,
            Msg::GALLERY_RADIO_OPTION,
            &[
                ("letter", arg(((b'A' + k as u8) as char).to_string())),
                ("selected", arg(if k == sel { "yes" } else { "no" })),
            ],
        );
        let mut scoped = cx.at_scope(style_env("md3"));
        let control = style.radio(id, k == sel, &mut scoped);
        let line = Container::new(id.with("row"), NodeKind::Row)
            .gap(10.0)
            .child(scoped.element(control))
            .child_decl(label(id.with("cap"), caption, 12.0, muted(), 220.0));
        col = col.child(Box::new(line));
    }
    // Disabled samples are their own instances: `disabled` is a build-time knob here.
    let mut disabled_params = ui_widgets_md3::Md3Options::default();
    disabled_params.disabled = true;
    let disabled_style = Md3Kit::with_scope(ui_style::StyleScope::default(), disabled_params);
    for (k, on) in [(0usize, true), (1, false)] {
        let id = radio_extra_id(k);
        let caption = tf(
            cx,
            Msg::GALLERY_RADIO_DISABLED,
            &[("selected", arg(if on { "yes" } else { "no" }))],
        );
        let mut scoped = cx.at_scope(Env::with(Some(root_env()), disabled_params));
        let control = disabled_style.radio(id, on, &mut scoped);
        let line = Container::new(id.with("row"), NodeKind::Row)
            .gap(10.0)
            .child(scoped.element(control))
            .child_decl(label(id.with("cap"), caption, 12.0, muted(), 220.0));
        col = col.child(Box::new(line));
    }
    col = col.child_decl(label(
        Id::str("radio/note"),
        tr(cx, Msg::GALLERY_RADIO_NOTE),
        11.0,
        muted(),
        320.0,
    ));
    row = row.child(column(
        Page::Radio,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_TALL_H,
        cx,
    ));
    // WinUI's radio group: the app owns which option is on, the control only reports.
    let kit = ui_widgets_fluent::FluentKit::new();
    let mut fcol = Container::new(Id::of(("body", "fluent")), NodeKind::Column).gap(6.0);
    for k in 0..RADIOS {
        let id = fluent_radio_id(k);
        let mut scoped = cx.at_scope(style_env("fluent"));
        let text = tf(
            &scoped,
            Msg::GALLERY_RADIO_OPTION,
            &[
                ("letter", arg(((b'A' + k as u8) as char).to_string())),
                ("selected", arg(if k == fsel { "yes" } else { "no" })),
            ],
        );
        let params = ui_widgets_fluent::RadioParams::new(id, text, k == fsel);
        let control = kit.radio(params, &mut scoped);
        let caption = tf(
            &scoped,
            Msg::GALLERY_RADIO_OPTION,
            &[
                ("letter", arg(((b'A' + k as u8) as char).to_string())),
                ("selected", arg(if k == fsel { "yes" } else { "no" })),
            ],
        );
        let line = Container::new(id.with("row"), NodeKind::Row)
            .gap(10.0)
            .child(scoped.element(control))
            .child_decl(label(id.with("cap"), caption, 12.0, muted(), 220.0));
        fcol = fcol.child(Box::new(line));
    }
    for (k, on) in [(0usize, true), (1, false)] {
        let id = Id::of(("fradio-extra", k));
        let mut scoped = cx.at_scope(style_env("fluent"));
        let text = tf(
            &scoped,
            Msg::GALLERY_RADIO_OPTION,
            &[
                ("letter", arg(((b'X' + k as u8) as char).to_string())),
                ("selected", arg(if on { "yes" } else { "no" })),
            ],
        );
        let params = ui_widgets_fluent::RadioParams::new(id, text, on).disabled();
        let control = kit.radio(params, &mut scoped);
        fcol = fcol.child(scoped.element(control));
    }
    row = row.child(column(
        Page::Radio,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(fcol),
        COL_TALL_H,
        cx,
    ));
    row = row.child(pending_column(Page::Radio, "glass", COL_W, COL_TALL_H, cx));
    Box::new(row)
}
