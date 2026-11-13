// The switch page.

use crate::shell::*;

pub(crate) fn switch_page(
    styles: &[(&dyn StyleKit, &'static str, Msg); 3],
    on: bool,
    variant_on: [bool; 4],
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Switch.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    for (style, tag, title) in styles {
        // MD3 is the only style with the icon/disabled knobs today, so its column shows all
        // four spec variants; the other styles show their single toggle, as before.
        let body: Box<dyn Element> = if *tag == "md3" {
            md3_switch_variants(variant_on, cx)
        } else {
            let mut scoped = cx.at_scope(style_env(tag));
            let control = style.toggle(
                &ToggleSpec::new(switch_id(Page::Switch, tag), on, ""),
                &mut scoped,
            );
            Box::new(
                Container::new(Id::of(("body", *tag)), NodeKind::Column)
                    .gap(10.0)
                    .child(scoped.element(control))
                    .child_decl(label(
                        Id::of(("readout", *tag)),
                        format!("on = {on}"),
                        12.0,
                        muted(),
                        300.0,
                    )),
            )
        };
        // Four MD3 variant rows need the tall column; other styles keep the control height.
        let h = if *tag == "md3" {
            COL_TALL_H
        } else {
            COL_CTRL_H
        };
        row = row.child(column(Page::Switch, tag, *title, body, h, cx));
    }
    Box::new(row)
}

/// The stable id of the `k`-th MD3 switch variant on the Switch page.
pub(crate) fn md3_variant_id(k: usize) -> Id {
    col_id(Page::Switch, "md3").with(("variant", k))
}

pub(crate) fn md3_switch_variants(variant_on: [bool; 4], cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(8.0);
    for (k, (has_icon, disabled)) in [(true, false), (false, false), (true, true), (false, true)]
        .iter()
        .enumerate()
    {
        // Derive the control id from the column id so the demo can address it deterministically.
        let id = md3_variant_id(k);
        let mut params = ui_widgets_md3::Md3Options::default();
        params.has_icon = *has_icon;
        params.disabled = *disabled;
        // The pack resolves from the scope's appearance, so the same instance serves both themes.
        let style = Md3Kit::with_scope(ui_style::StyleScope::default(), params);
        // Seed was icon->off / plain->on; `variant_on` lets a click flip just this one.
        let show_on = variant_on[k];
        let mut scoped = cx.at_scope(Env::with(Some(root_env()), params));
        let control = style.toggle(&ToggleSpec::new(id, show_on, ""), &mut scoped);
        let caption = tf(
            &scoped,
            Msg::GALLERY_SWITCH_CAPTION,
            &[
                ("kind", arg(if *has_icon { "icon" } else { "plain" })),
                ("disabled", arg(if *disabled { "yes" } else { "no" })),
            ],
        );
        let row = Container::new(id.with("row"), NodeKind::Row)
            .gap(10.0)
            .child(scoped.element(control))
            .child_decl(label(id.with("cap"), caption, 11.0, muted(), 220.0));
        col = col.child(Box::new(row));
    }
    Box::new(col)
}
