// The three design-system pages and their row registry.

use crate::shell::*;

/// Ids of the row nodes a system page emitted, so the selftest can look them up by value.
#[derive(Default)]
pub(crate) struct SystemRows {
    pub(crate) common: std::cell::RefCell<Vec<Id>>,
    pub(crate) dims: std::cell::RefCell<Vec<Id>>,
    pub(crate) source: std::cell::RefCell<Vec<Id>>,
}

thread_local! {
    pub(crate) static SYSTEM_ROWS: SystemRows = SystemRows::default();
}

pub(crate) fn record_row(kind: &str, id: Id) {
    SYSTEM_ROWS.with(|r| match kind {
        "common" => r.common.borrow_mut().push(id),
        "dims" => r.dims.borrow_mut().push(id),
        _ => r.source.borrow_mut().push(id),
    });
}

/// A design-system page: one system shown as colours, dimension tables, and provenance.
/// The token names, hex values and source labels stay as written: they are diagnostics.
pub(crate) fn system_page(
    id: ui_design::SystemId,
    page: Page,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    use ui_design::Context;
    let ctx = Context::new(ui_design::Appearance::Light, ui_design::Platform::Ios);
    let dims = ui_design::dimensions_of(id, ctx);
    let mut row = Container::new(Id::of(("bodyrow", page.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);

    let system = arg(id.name());
    let colours_title = tf(
        cx,
        Msg::GALLERY_SYSTEM_COLOURS,
        &[("system", system.clone())],
    );
    let dimensions_title = tf(cx, Msg::GALLERY_SYSTEM_DIMENSIONS, &[("system", system)]);
    row = row.child(column_text(
        page,
        "colours",
        colours_title,
        colours_column(id, page, ctx, cx),
        COL_W,
        COL_TALL_H,
    ));
    row = row.child(column_text(
        page,
        "dimensions",
        dimensions_title,
        dimensions_column(id, &dims, page),
        COL_W,
        COL_TALL_H,
    ));
    row = row.child(column(
        page,
        "source",
        Msg::GALLERY_SYSTEM_PROVENANCE,
        source_column(id, ctx, page),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}

pub(crate) fn rgb_of(c: ui_design::Rgba) -> Color {
    let a = c.argb();
    Color::from_rgba8((a >> 16) as u8, (a >> 8) as u8, a as u8, (a >> 24) as u8)
}

/// Colour column: the common roles, then what the native path adds, each with provenance.
pub(crate) fn colours_column(
    id: ui_design::SystemId,
    page: Page,
    ctx: ui_design::Context,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    use ui_design::{CommonColor, Palette, Resolution, StyleSystem};
    let (common_pairs, extra): (
        Vec<(
            CommonColor,
            Resolution<ui_design::Resolved<ui_design::Rgba>>,
        )>,
        Vec<(&'static str, &'static str)>,
    ) = match id {
        ui_design::SystemId::Md3 => {
            let s = ui_design::Md3Scheme::baseline(ctx);
            let pairs = CommonColor::ALL
                .iter()
                .map(|r| (*r, s.common_colour(*r)))
                .collect();
            (
                pairs,
                vec![
                    ("primary_container", "md3"),
                    ("on_primary_container", "md3"),
                    ("surface_container_high", "md3"),
                    ("outline_variant", "md3"),
                    ("on_primary_fixed", "md3"),
                ],
            )
        }
        ui_design::SystemId::Fluent => {
            let s = ui_design::FluentSystem::new(ctx);
            let pairs = CommonColor::ALL
                .iter()
                .map(|r| (*r, s.common_colour(*r)))
                .collect();
            (
                pairs,
                vec![
                    ("colorBrandBackground2", "fluent"),
                    ("colorNeutralStroke1", "fluent"),
                    ("colorPaletteRedBackground3", "fluent"),
                    ("colorPaletteGreenBackground3", "fluent"),
                    ("colorStrokeFocus2", "fluent"),
                ],
            )
        }
        ui_design::SystemId::Hig => {
            let s = ui_design::HigSystem::new(ctx);
            let pairs = CommonColor::ALL
                .iter()
                .map(|r| (*r, s.common_colour(*r)))
                .collect();
            (
                pairs,
                vec![
                    ("systemRed", "hig"),
                    ("systemGreen", "hig"),
                    ("systemOrange", "hig"),
                    ("systemBlue", "hig"),
                    ("systemMint", "hig"),
                ],
            )
        }
    };

    let mut col = Decl::container(Id::of(("sys", "colours")), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(3.0);

    col = col.child(label(
        Id::of(("sys", "common", page.tag())),
        tr(cx, Msg::GALLERY_SYSTEM_COMMON_ROLES),
        10.0,
        Color::from_rgba8(150, 190, 240, 255),
        COL_W - 24.0,
    ));
    for (i, (role, res)) in common_pairs.into_iter().enumerate() {
        let (fill, note) = match res {
            Resolution::Value(r) => (
                rgb_of(r.value),
                format!("#{:06X} · {}", r.value.argb() & 0xffffff, r.source.label()),
            ),
            Resolution::Absent(_) => (Color::from_rgba8(60, 64, 74, 255), "host".to_string()),
            Resolution::Unmapped => (Color::from_rgba8(120, 50, 50, 255), "n/a".to_string()),
        };
        let tag: &'static str = match res {
            Resolution::Value(_) => "cmono",
            Resolution::Absent(_) => "chost",
            Resolution::Unmapped => "cnone",
        };
        let row_id = Id::of(("syscommon", i, page.tag(), tag));
        record_row("common", row_id);
        col = col.child(
            Decl::container(row_id, NodeKind::Row)
                .size(SizeClass::fixed(COL_W - 24.0, 18.0))
                .gap(6.0)
                .child(
                    Decl::container(row_id.with("chip"), NodeKind::Column)
                        .size(SizeClass::fixed(30.0, 14.0))
                        .background(fill, Corners::all(3.0))
                        .border(Color::from_rgba8(255, 255, 255, 30), 1.0),
                )
                .child(label(
                    row_id.with("name"),
                    role.name().to_string(),
                    9.0,
                    Color::from_rgba8(206, 214, 228, 255),
                    180.0,
                ))
                .child(label(
                    row_id.with("val"),
                    note,
                    9.0,
                    Color::from_rgba8(150, 158, 176, 255),
                    100.0,
                )),
        );
    }

    col = col.child(label(
        Id::of(("sys", "native", page.tag())),
        tr(cx, Msg::GALLERY_SYSTEM_NATIVE_EXTRAS),
        10.0,
        Color::from_rgba8(150, 190, 240, 255),
        COL_W - 24.0,
    ));
    for (i, (name, which)) in extra.into_iter().enumerate() {
        let c = match which {
            "md3" => ui_design::Md3Scheme::baseline(ctx).native_colour(name),
            "fluent" => ui_design::FluentSystem::new(ctx).native_colour(name),
            _ => ui_design::HigSystem::new(ctx).native_colour(name),
        };
        let (fill, note) = match c {
            Resolution::Value(v) => (rgb_of(v), format!("#{:06X}", v.argb() & 0xffffff)),
            _ => (Color::from_rgba8(60, 64, 74, 255), "host".to_string()),
        };
        let row_id = Id::of(("sysnative", i, page.tag()));
        record_row("common", row_id);
        col = col.child(
            Decl::container(row_id, NodeKind::Row)
                .size(SizeClass::fixed(COL_W - 24.0, 18.0))
                .gap(6.0)
                .child(
                    Decl::container(row_id.with("chip"), NodeKind::Column)
                        .size(SizeClass::fixed(30.0, 14.0))
                        .background(fill, Corners::all(3.0))
                        .border(Color::from_rgba8(255, 255, 255, 30), 1.0),
                )
                .child(label(
                    row_id.with("name"),
                    name.to_string(),
                    9.0,
                    Color::from_rgba8(206, 214, 228, 255),
                    180.0,
                ))
                .child(label(
                    row_id.with("val"),
                    note,
                    9.0,
                    Color::from_rgba8(150, 158, 176, 255),
                    100.0,
                )),
        );
    }
    if id == ui_design::SystemId::Hig {
        col = col.child(materials_strip(page));
    }
    from_decl(col)
}

/// Apple's standard content materials over text, thinnest to thickest.
///
/// The ordering is the whole contract ("thicker = more opaque = better contrast for text"),
/// so a chip shows its own blur and tint over the lines it must keep readable. The page runs
/// dark, which is why the dark context is the one that matches it.
fn materials_strip(page: Page) -> Decl {
    const CHIP: f32 = 44.0;
    const GAP: f32 = 3.0;
    const WIDTH: f32 = COL_W - 24.0;
    const HEIGHT: f32 = 64.0;
    const NAMES: [&str; 6] = [
        "ultraThinMaterial",
        "thinMaterial",
        "regularMaterial",
        "thickMaterial",
        "ultraThickMaterial",
        "bar",
    ];
    let context = ui_design::Context::new(ui_design::Appearance::Light, ui_design::Platform::Ios);
    let hig = ui_design::HigSystem::new(context);
    let band = NAMES.len() as f32 * CHIP + (NAMES.len() - 1) as f32 * GAP;

    // The ground is the material names themselves over a dark card: a name under a chip is
    // that chip's own label seen through it, and pale tints only read against something dark.
    let mut lines =
        Decl::container(Id::of(("sys", "matlines", page.tag())), NodeKind::Column).gap(3.0);
    for (i, text) in [
        "ultraThinMaterial · thinMaterial · regularMaterial",
        "thickMaterial · ultraThickMaterial · bar",
    ]
    .into_iter()
    .enumerate()
    {
        lines = lines.child(label(
            Id::of(("sys", "matline", i, page.tag())),
            text.to_string(),
            10.0,
            Color::from_rgba8(226, 232, 240, 255),
            WIDTH,
        ));
    }
    let ground = Decl::container(Id::of(("sys", "matground")), NodeKind::Stack)
        .size(SizeClass::fixed(WIDTH, HEIGHT))
        .stack_align(ui_core::layout::OverlayAlign::Center)
        .background(Color::from_rgba8(28, 30, 36, 255), Corners::all(8.0))
        .child(lines);

    // Only the right band is covered, so the same lines stay sharp beside it: the blur is
    // the claim, and nothing else in the page would show it.
    let mut chips = Decl::container(Id::of(("sys", "matchips", page.tag())), NodeKind::Row)
        .size(SizeClass::fixed(band, HEIGHT))
        .gap(GAP);
    for (i, name) in NAMES.iter().enumerate() {
        let spec = hig_material(&hig, name);
        let mut chip = Decl::container(Id::of(("sysmat", i, page.tag())), NodeKind::Box)
            .size(SizeClass::fixed(CHIP, HEIGHT))
            .background(rgb_of(spec.tint), Corners::all(8.0))
            .border(Color::from_rgba8(0, 0, 0, 36), 1.0);
        chip = chip.backdrop_blur(spec.sigma);
        chips = chips.child(chip);
    }

    Decl::container(Id::of(("sys", "materials", page.tag())), NodeKind::Stack)
        .size(SizeClass::fixed(WIDTH, HEIGHT))
        .stack_align(ui_core::layout::OverlayAlign::End)
        .child(ground)
        .child(chips)
}

/// A material the HIG system must answer for; a missing one is a broken table, not a look.
fn hig_material(system: &ui_design::HigSystem, name: &str) -> ui_design::MaterialSpec {
    use ui_design::StyleSystem;
    system
        .material(name)
        .value()
        .unwrap_or_else(|| panic!("HIG has no material {name}"))
        .value
}

/// Dimension column: radii, spacing, type and motion as readable rows.
pub(crate) fn dimensions_column(
    id: ui_design::SystemId,
    dims: &ui_design::DimensionTokens,
    page: Page,
) -> Box<dyn Element> {
    let mut col = Decl::container(Id::of(("sys", "dims")), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(2.0);
    let mut n = 0usize;
    let mut add = |col: Decl, text: String, size: f32, colour: Color| -> Decl {
        let row = Id::of(("sysdim", n, page.tag()));
        record_row("dims", row);
        n += 1;
        col.child(label(row, text, size, colour, COL_W - 24.0))
    };

    let head = Color::from_rgba8(150, 190, 240, 255);
    let body = Color::from_rgba8(206, 214, 228, 255);

    col = add(
        col,
        format!("radius  ({})", dims.radius.curvature.name()),
        10.0,
        head,
    );
    let radii: Vec<String> = dims
        .radius
        .steps
        .iter()
        .map(|s| {
            format!(
                "{}:{}",
                s.name,
                if s.points < 0.0 {
                    "full".into()
                } else {
                    format!("{}", s.points)
                }
            )
        })
        .collect();
    col = add(col, radii.join("  "), 9.0, body);
    col = add(
        col,
        format!("cut corners: {}", dims.radius.supports_cut),
        9.0,
        body,
    );

    col = add(
        col,
        format!("spacing  (base {}pt)", dims.spacing.base_points),
        10.0,
        head,
    );
    let spac: Vec<String> = dims
        .spacing
        .steps
        .iter()
        .map(|s| format!("{}:{}", s.name, s.multiple * dims.spacing.base_points))
        .collect();
    col = add(col, spac.join("  "), 9.0, body);
    if !dims.spacing.nested.is_empty() {
        col = add(col, format!("nested: {:?}", dims.spacing.nested), 9.0, body);
    }

    col = add(
        col,
        format!("type  (scalable: {})", dims.typography.scalable),
        10.0,
        head,
    );
    for s in dims.typography.steps {
        col = add(
            col,
            format!(
                "{}  {}sp / {}  w{}",
                s.name, s.size, s.line_height, s.weight
            ),
            9.0,
            body,
        );
    }

    col = add(col, "motion".to_string(), 10.0, head);
    let durs: Vec<String> = dims
        .motion
        .durations
        .iter()
        .map(|d| format!("{}:{}", d.name, d.ms))
        .collect();
    col = add(col, durs.join("  "), 9.0, body);
    let curves: Vec<String> = dims
        .motion
        .curves
        .iter()
        .map(|(n, _)| n.to_string())
        .collect();
    col = add(col, curves.join("  "), 9.0, body);

    col = add(
        col,
        format!(
            "hit target  {} / {} (min)",
            dims.hit_target.default_points, dims.hit_target.minimum_points
        ),
        9.0,
        body,
    );
    let _ = id;
    from_decl(col)
}

/// Provenance column: every scale's source label, so the page answers "why believe this?".
pub(crate) fn source_column(
    id: ui_design::SystemId,
    ctx: ui_design::Context,
    page: Page,
) -> Box<dyn Element> {
    use ui_design::StyleSystem;
    let dims = ui_design::dimensions_of(id, ctx);
    let system: Box<dyn StyleSystem> = match id {
        ui_design::SystemId::Md3 => Box::new(ui_design::Md3Scheme::baseline(ctx)),
        ui_design::SystemId::Fluent => Box::new(ui_design::FluentSystem::new(ctx)),
        ui_design::SystemId::Hig => Box::new(ui_design::HigSystem::new(ctx)),
    };
    let mut col = Decl::container(Id::of(("sys", "source")), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(4.0);
    let mut n = 0usize;
    let mut add = |col: Decl, a: String, b: String| -> Decl {
        let row = Id::of(("syssrc", n, page.tag()));
        record_row("source", row);
        n += 1;
        col.child(
            Decl::container(row, NodeKind::Column)
                .size(SizeClass::fixed(COL_W - 24.0, 30.0))
                .gap(1.0)
                .child(label(
                    row.with("a"),
                    a,
                    9.5,
                    Color::from_rgba8(206, 214, 228, 255),
                    COL_W - 24.0,
                ))
                .child(label(
                    row.with("b"),
                    b,
                    8.5,
                    Color::from_rgba8(146, 156, 176, 255),
                    COL_W - 24.0,
                )),
        )
    };

    for (label, src) in [
        ("radius", dims.radius.source),
        ("spacing", dims.spacing.source),
        ("type", dims.typography.source),
        ("motion", dims.motion.source),
        ("hit target", dims.hit_target.source),
        ("colour", system.provenance("accent")),
    ] {
        col = add(
            col,
            format!("{label}  [{}]", src.label()),
            src.detail().to_string(),
        );
    }
    let caps = system.caps();
    col = add(
        col,
        "capabilities".to_string(),
        format!(
            "tones:{} containers:{} states:{} palettes:{} depth:{} materials:{} glass:{} cut:{} motion:{}",
            caps.arbitrary_tones,
            caps.containers,
            caps.named_state_tokens,
            caps.shared_palettes,
            caps.elevated_depth,
            caps.materials,
            caps.glass,
            caps.cut_corners,
            caps.motion_tokens
        ),
    );
    from_decl(col)
}
