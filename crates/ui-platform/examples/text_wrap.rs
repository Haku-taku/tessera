//! Wrapped text: no glyph may paint outside the text node that laid it out.
//!
//! Layout measures with `TextPass::measure` and paint breaks with the same function, so a
//! glyph outside its box means the two disagreed about the wrap width or the line count.
//!
//! Run: `cargo run -p ui-platform --example text_wrap --features system-fonts`

use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeData, NodeKind};
use ui_platform::present::OffscreenTarget;
use ui_platform::{App, AppLoop};

const WIDTH: u32 = 420;
const HEIGHT: u32 = 480;

const LATIN: &str = "The quick brown fox jumps over the lazy dog while the compositor waits";
const CJK: &str = "中文换行测试需要一个足够长的句子来触发断行否则看不到效果";

struct Demo;

fn text(
    id: &'static str,
    body: &str,
    size: f32,
    width: f32,
    max_lines: Option<u32>,
    color: Color,
) -> Decl {
    Decl {
        id: Id::str(id),
        kind: NodeKind::Text,
        shapes: Vec::new(),
        data: NodeData::Text {
            ink: color,
            block: std::rc::Rc::new(ui_core::TextBlock {
                runs: vec![ui_core::TextRun::new(body, ui_core::TextStyle::new(size))],
                align: ui_core::TextAlign::Start,
                max_lines,
                line_gap: 0.0,
                indent: 0.0,
                hanging: 0.0,
                atoms: Vec::new(),
                input_base: None,
                line_numbers: None,
                structures: Vec::new(),
            }),
            caret: None,
            selection: None,
            composing: None,
            nowrap: false,
        },
        size_class: SizeClass {
            // `fixed`, not `hug`: a hug's max is infinite and the column would stretch it.
            width: AxisSize::fixed(width),
            height: AxisSize::hug(size, size * 1.4),
            grow_x: Grow::No,
            grow_y: Grow::No,
        },
        ink_margin: Insets::ZERO,
        box_model: Default::default(),
        offset: ui_core::geom::Point::new(0.0, 0.0),
        cursor: None,
        sense: ui_core::Sense::NONE,
        disabled: false,
        grid: Default::default(),
        placement: Default::default(),
        env: None,
        children: Vec::new(),
    }
}

impl App for Demo {
    fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let decl = {
            Decl::container(Id::str("root"), NodeKind::Column)
                .background(Color::from_rgba8(20, 22, 28, 255), Corners::ZERO)
                .padding(Insets::all(12.0))
                .gap(8.0)
                .child(text("latin", LATIN, 15.0, 170.0, None, Color::WHITE))
                .child(text(
                    "cjk",
                    CJK,
                    15.0,
                    170.0,
                    None,
                    Color::from_rgba8(255, 220, 150, 255),
                ))
                .child(text(
                    "wide",
                    "This one has room to spare",
                    15.0,
                    380.0,
                    None,
                    Color::from_rgba8(150, 200, 255, 255),
                ))
                // Capped at two lines: the third must be replaced by an ellipsis.
                .child(text(
                    "capped",
                    LATIN,
                    15.0,
                    170.0,
                    Some(2),
                    Color::from_rgba8(255, 170, 200, 255),
                ))
                // Two styles in one paragraph: rich text is just more runs.
                .child(rich())
                // Centred: the line is placed by its own width, not the box.
                .child({
                    let mut d = text(
                        "centred",
                        "centred line",
                        15.0,
                        240.0,
                        None,
                        Color::from_rgba8(180, 255, 200, 255),
                    );
                    if let NodeData::Text { block, .. } = &mut d.data {
                        let block = std::rc::Rc::make_mut(block);
                        block.align = ui_core::TextAlign::Center;
                    }
                    d
                })
                .child({
                    let mut d = text(
                        "aligned",
                        "right aligned",
                        15.0,
                        240.0,
                        None,
                        Color::from_rgba8(200, 220, 255, 255),
                    );
                    if let NodeData::Text { block, .. } = &mut d.data {
                        let block = std::rc::Rc::make_mut(block);
                        block.align = ui_core::TextAlign::End;
                    }
                    d
                })
        };
        ui_platform::from_decl(decl)
    }
}

/// One paragraph, two styles: a heading run followed by a body run.
fn rich() -> Decl {
    let mut d = text("rich", "", 15.0, 300.0, None, Color::WHITE);
    if let NodeData::Text { block, .. } = &mut d.data {
        let block = std::rc::Rc::make_mut(block);
        block.runs = vec![
            ui_core::TextRun::new("Rich ", ui_core::TextStyle::new(20.0)),
            ui_core::TextRun::new(
                "text with two styles in one paragraph, wrapping as one.",
                ui_core::TextStyle::new(13.0),
            ),
        ];
    }
    d
}

fn contains(rect: Rect, x: f32, y: f32, slack: f32) -> bool {
    x >= rect.min_x() - slack
        && x <= rect.max_x() + slack
        && y >= rect.min_y() - slack
        && y <= rect.max_y() + slack
}

fn main() {
    // `with_stack` wires layout to the same pass the painter uses; `new` would fall back
    // to the monospace estimate and the two would disagree about line breaks.
    let mut app = AppLoop::with_stack(Demo, &["MiSans VF", "Tahoma"]);
    let mut target = OffscreenTarget::new(WIDTH, HEIGHT);
    app.frame(&mut target, None);

    let boxes: Vec<(Id, Rect)> = app
        .tree()
        .walk()
        .into_iter()
        .filter_map(|nid| app.tree().get(nid))
        .filter(|n| matches!(n.data, NodeData::Text { .. }))
        .map(|n| (n.id, n.rect))
        .collect();

    for (id, rect) in &boxes {
        println!(
            "{id:?} rect {:.0},{:.0} {:.0}x{:.0}",
            rect.origin.x, rect.origin.y, rect.size.w, rect.size.h
        );
    }

    let mut stray = 0;
    for glyph in &app.draw_list().prims.glyphs {
        let cx = glyph.rect.origin.x + glyph.rect.size.w * 0.5;
        let cy = glyph.rect.origin.y + glyph.rect.size.h * 0.5;
        if !boxes.iter().any(|(_, r)| contains(*r, cx, cy, 1.0)) {
            stray += 1;
        }
    }
    let mut too_short = 0;
    for (id, rect) in &boxes {
        let node = app
            .tree()
            .walk()
            .into_iter()
            .filter_map(|nid| app.tree().get(nid))
            .find(|n| n.id == *id)
            .expect("node");
        let (block, wrap) = match &node.data {
            NodeData::Text { block, .. } => (block.clone(), node.wrap_width),
            _ => continue,
        };
        let m = app.text_pass_mut().measure(&block, wrap);
        if rect.size.h + 0.5 < m.h {
            too_short += 1;
        }
        println!(
            "{id:?} wrap {:.1} box_h {:.1} text_h {:.1}  {}",
            wrap,
            rect.size.h,
            m.h,
            if rect.size.h + 0.5 >= m.h {
                "OK"
            } else {
                "TOO SHORT"
            }
        );
    }

    println!(
        "glyphs {} stray {stray}",
        app.draw_list().prims.glyphs.len()
    );

    if std::env::var("DUMP_WRAP").is_ok() {
        use std::io::Write;
        let px = &target.pixels;
        let mut f = std::fs::File::create("/tmp/text_wrap.ppm").expect("ppm");
        write!(f, "P6\n{} {}\n255\n", WIDTH, HEIGHT).expect("hdr");
        for p in px.data.as_chunks::<4>().0 {
            let _ = f.write_all(&[p[2], p[1], p[0]]);
        }
        println!("dumped /tmp/text_wrap.ppm");
    }

    if app.draw_list().prims.glyphs.is_empty() {
        eprintln!("no glyphs resolved");
        std::process::exit(1);
    }
    if too_short > 0 {
        eprintln!("\n{too_short} text box(es) shorter than their own text");
        std::process::exit(1);
    }
    if stray > 0 {
        eprintln!("\n{stray} glyph(s) paint outside every text box");
        std::process::exit(1);
    }
    println!("\nwrapped text stays inside its layout boxes");
}
