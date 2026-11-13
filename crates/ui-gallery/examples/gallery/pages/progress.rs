// The progress indicator page.

use ui_widgets_glass::{GlassProgress, GlassProgressParams};

use crate::shell::*;

/// The determinate values the page shows; the bar itself owns no state.
pub(crate) const PROGRESS_VALUES: [f32; 4] = [0.0, 0.25, 0.6, 1.0];

pub(crate) fn progress_id(k: usize) -> Id {
    Id::of(("progress", k))
}

/// The sweeping ring's id; it has no value, so it needs no index.
pub(crate) fn progress_ring_id() -> Id {
    Id::str("progress/ring")
}

/// The expressive bar and its sweeping twin.
pub(crate) fn progress_wavy_id() -> Id {
    Id::str("progress/wavy")
}

pub(crate) fn progress_wavy_sweep_id() -> Id {
    Id::str("progress/wavy-sweep")
}

/// The rings that show a value, one per entry in `PROGRESS_VALUES`.
pub(crate) fn progress_ring_value_id(k: usize) -> Id {
    Id::of(("progress/ring-value", k))
}

/// The glass column's rings: the four values at the default 20 dp, then two 40 dp ones.
pub(crate) fn progress_glass_ring_id(k: usize) -> Id {
    Id::of(("gprogress-ring", k))
}

pub(crate) fn progress_glass_big_id(k: usize) -> Id {
    Id::of(("gprogress-big", k))
}

/// The glass bars: one per value, then the sweeping one.
pub(crate) fn progress_glass_bar_id(k: usize) -> Id {
    Id::of(("gprogress-bar", k))
}

/// MD3 linear progress: four determinate values, the ends included.
pub(crate) fn progress_page(cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Progress.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(16.0);
    for (k, value) in PROGRESS_VALUES.iter().enumerate() {
        let id = progress_id(k);
        let control = style.progress_indicator(id, *value, COL_W - 24.0, &mut scoped);
        let caption = tf(
            &scoped,
            Msg::GALLERY_PROGRESS_CAPTION,
            &[("percent", arg(format!("{:.0}", value * 100.0)))],
        );
        col = col.child(Box::new(captioned_row(
            id,
            scoped.element(control),
            caption,
        )));
    }
    // ── No value: the bar sweeps the Android pair of bars until the app has one.
    let sweep_id = progress_id(PROGRESS_VALUES.len());
    let control = style.progress_indicator_indeterminate(sweep_id, COL_W - 24.0, &mut scoped);
    col = col.child(Box::new(captioned_row(
        sweep_id,
        scoped.element(control),
        tr(&scoped, Msg::GALLERY_PROGRESS_MD3_INDETERMINATE),
    )));
    // ── The expressive form: a wave over 60% of the track, then its two sweeping waves.
    let wavy_id = progress_wavy_id();
    let control = style.progress_indicator_wavy(wavy_id, 0.6, COL_W - 24.0, &mut scoped);
    col = col.child(Box::new(captioned_row(
        wavy_id,
        scoped.element(control),
        tr(&scoped, Msg::GALLERY_PROGRESS_WAVY),
    )));
    let wavy_sweep_id = progress_wavy_sweep_id();
    let control =
        style.progress_indicator_wavy_indeterminate(wavy_sweep_id, COL_W - 24.0, &mut scoped);
    col = col.child(Box::new(captioned_row(
        wavy_sweep_id,
        scoped.element(control),
        tr(&scoped, Msg::GALLERY_PROGRESS_WAVY_INDETERMINATE),
    )));

    // ── The ring form with the same values: a track, then an arc from twelve o'clock.
    let mut rings = Container::new(Id::str("progress/rings"), NodeKind::Row).gap(16.0);
    for (k, value) in PROGRESS_VALUES.iter().enumerate() {
        let id = progress_ring_value_id(k);
        let control = style.progress_ring(id, 40.0, *value, &mut scoped);
        let caption = tf(
            &scoped,
            Msg::GALLERY_PROGRESS_CAPTION,
            &[("percent", arg(format!("{:.0}", value * 100.0)))],
        );
        rings = rings.child(Box::new(
            Container::new(id.with("cell"), NodeKind::Column)
                .gap(4.0)
                .child(scoped.element(control))
                .child_decl(label(id.with("cap"), caption, 11.0, muted(), 40.0)),
        ));
    }
    col = col.child(Box::new(rings));

    // ── And the same ring with no value: an arc that grows and travels on its own clock.
    let ring_id = progress_ring_id();
    let control = style.progress_ring_indeterminate(ring_id, 40.0, &mut scoped);
    col = col.child(Box::new(captioned_row(
        ring_id,
        scoped.element(control),
        tr(&scoped, Msg::GALLERY_PROGRESS_MD3_RING_INDETERMINATE),
    )));
    col = col.child_decl(label(
        Id::str("progress/note"),
        tr(&scoped, Msg::GALLERY_PROGRESS_NOTE),
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Progress,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_TALL_H,
        &scoped,
    ));

    // WinUI's bar owns its clock while indeterminate; the app supplies only the value.
    let kit = ui_widgets_fluent::FluentKit::new();
    let mut fcol = Container::new(Id::of(("body", "fluent")), NodeKind::Column).gap(14.0);
    for (k, value) in PROGRESS_VALUES.iter().enumerate() {
        let id = Id::of(("fprogress", k));
        let mut scoped = cx.at_scope(style_env("fluent"));
        let caption = tf(
            &scoped,
            Msg::GALLERY_PROGRESS_CAPTION,
            &[("percent", arg(format!("{:.0}", value * 100.0)))],
        );
        let control = kit.progress_bar(
            ui_widgets_fluent::ProgressBarParams::new(id, *value),
            &mut scoped,
        );
        fcol = fcol.child(Box::new(captioned_row(
            id,
            scoped.element(control),
            caption,
        )));
    }
    for (tag, caption, params) in [
        (
            "error",
            Msg::GALLERY_PROGRESS_ERROR,
            ui_widgets_fluent::ProgressBarParams::new(Id::str("fprogress/error"), 0.6)
                .status(ui_widgets_fluent::ProgressStatus::Error),
        ),
        (
            "paused",
            Msg::GALLERY_PROGRESS_PAUSED,
            ui_widgets_fluent::ProgressBarParams::new(Id::str("fprogress/paused"), 0.6)
                .status(ui_widgets_fluent::ProgressStatus::Paused),
        ),
        (
            "indeterminate",
            Msg::GALLERY_PROGRESS_INDETERMINATE,
            ui_widgets_fluent::ProgressBarParams::new(Id::str("fprogress/sweep"), 0.0)
                .indeterminate(true),
        ),
    ] {
        let mut scoped = cx.at_scope(style_env("fluent"));
        let control = kit.progress_bar(params, &mut scoped);
        let text = tr(&scoped, caption);
        fcol = fcol.child(Box::new(captioned_row(
            Id::of(("fprogress-row", tag)),
            scoped.element(control),
            text,
        )));
    }
    // WinUI's ring: the same four values, the spinning state, and one that is inactive.
    let mut rings = Container::new(Id::str("fring/row"), NodeKind::Row)
        .gap(18.0)
        .cross_align(0.0)
        .cross_pos(0.5);
    for (k, value) in PROGRESS_VALUES.iter().enumerate() {
        let mut scoped = cx.at_scope(style_env("fluent"));
        let control = kit.progress_ring(
            ui_widgets_fluent::ProgressRingParams::new(Id::of(("fring", k)), *value),
            &mut scoped,
        );
        rings = rings.child(scoped.element(control));
    }
    for params in [
        ui_widgets_fluent::ProgressRingParams::new(Id::str("fring/spin"), 0.0).indeterminate(true),
        ui_widgets_fluent::ProgressRingParams::new(Id::str("fring/off"), 0.6).active(false),
    ] {
        let mut scoped = cx.at_scope(style_env("fluent"));
        let control = kit.progress_ring(params, &mut scoped);
        rings = rings.child(scoped.element(control));
    }
    let mut ring_block = Container::new(Id::str("fring/block"), NodeKind::Column).gap(6.0);
    let rings_caption = {
        let scoped = cx.at_scope(style_env("fluent"));
        tr(&scoped, Msg::GALLERY_PROGRESS_FLUENT_RINGS)
    };
    ring_block = ring_block.child(Box::new(rings)).child_decl(label(
        Id::str("fring/cap"),
        rings_caption,
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    fcol = fcol.child(Box::new(ring_block));
    let scoped = cx.at_scope(style_env("fluent"));
    fcol = fcol.child_decl(label(
        Id::str("fprogress/note"),
        tr(&scoped, Msg::GALLERY_PROGRESS_FLUENT_NOTE),
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Progress,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(fcol),
        COL_TALL_H,
        cx,
    ));
    // ── The glass column: the reference's 20 dp ring and 4 dp bar, determinate and sweeping.
    let gid = |tag: &'static str| Id::of(("progressglass", Page::Progress.tag(), tag));
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let mut glass_col = Container::new(Id::of(("body", "glass")), NodeKind::Column).gap(14.0);
    let mut rings = Container::new(gid("rings"), NodeKind::Row)
        .gap(18.0)
        .cross_align(0.0)
        .cross_pos(0.5);
    for (k, value) in PROGRESS_VALUES.iter().enumerate() {
        let mut scoped = cx.at_scope(style_env("glass"));
        let control = GlassProgress::new(
            GlassProgressParams::ring(progress_glass_ring_id(k), Some(*value)),
            &mut scoped,
            pack,
        );
        rings = rings.child(scoped.element(Box::new(control)));
    }
    // A larger ring shows the stroke and the halo the 20 dp default hides.
    for (k, value) in [Some(0.35f32), None].into_iter().enumerate() {
        let mut scoped = cx.at_scope(style_env("glass"));
        let control = GlassProgress::new(
            GlassProgressParams::ring(progress_glass_big_id(k), value)
                .size(40.0)
                .stroke(4.0),
            &mut scoped,
            pack,
        );
        rings = rings.child(scoped.element(Box::new(control)));
    }
    glass_col = glass_col.child(Box::new(captioned_row(
        gid("rings-cap"),
        Box::new(rings),
        tr(cx, Msg::GALLERY_PROGRESS_GLASS_RINGS),
    )));
    let mut bars = Container::new(gid("bars"), NodeKind::Column).gap(16.0);
    for (k, value) in PROGRESS_VALUES.iter().enumerate() {
        let mut scoped = cx.at_scope(style_env("glass"));
        let control = GlassProgress::new(
            GlassProgressParams::bar(progress_glass_bar_id(k), Some(*value)),
            &mut scoped,
            pack,
        );
        bars = bars.child(scoped.element(Box::new(control)));
    }
    {
        let mut scoped = cx.at_scope(style_env("glass"));
        let sweep = GlassProgress::new(
            GlassProgressParams::bar(progress_glass_bar_id(PROGRESS_VALUES.len()), None),
            &mut scoped,
            pack,
        );
        bars = bars.child(scoped.element(Box::new(sweep)));
    }
    glass_col = glass_col.child(Box::new(captioned_row(
        gid("bars-cap"),
        Box::new(bars),
        tr(cx, Msg::GALLERY_PROGRESS_GLASS_BARS),
    )));
    glass_col = glass_col.child_decl(label(
        Id::str("progress/glass-note"),
        tr(cx, Msg::GALLERY_PROGRESS_GLASS_NOTE),
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Progress,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        Box::new(glass_col),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
