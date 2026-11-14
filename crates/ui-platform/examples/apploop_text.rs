//! Renders a text-bearing widget tree through `AppLoop` and dumps a PPM.
//!
//! This is the check that text actually reaches the screen through the pipeline
//! rather than through a hand-rolled example.

use std::io::Write;

use ui_core::geom::Corners;
use ui_core::{AxisSize, Grow, NodeData, SizeClass};
use ui_core::{Color, Decl, Id, NodeKind};
use ui_platform::present::OffscreenTarget;
use ui_platform::{App, AppLoop};

const W: u32 = 560;
const H: u32 = 240;

fn text_decl(id: &'static str, text: &str, size: f32, color: Color) -> Decl {
    Decl {
        id: Id::str(id),
        kind: NodeKind::Text,
        shapes: Vec::new(),
        data: NodeData::Text {
            ink: color,
            block: std::sync::Arc::new(ui_core::TextBlock {
                runs: vec![ui_core::TextRun::new(text, ui_core::TextStyle::new(size))],
                align: ui_core::TextAlign::Start,
                max_lines: None,
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
            width: AxisSize::hug(0.0, 520.0),
            height: AxisSize::hug(size, size * 1.4),
            grow_x: Grow::No,
            grow_y: Grow::No,
        },
        ink_margin: ui_core::geom::Insets::ZERO,
        box_model: Default::default(),
        offset: ui_core::geom::Point::new(0.0, 0.0),
        cursor: None,
        sense: ui_core::Sense::NONE,
        disabled: false,
        grid: Default::default(),
        placement: Default::default(),
        env: None,
        drag: None,
        drag_out: false,
        drop_types: None,
        children: Vec::new(),
    }
}

struct Demo;

impl App for Demo {
    fn view(&mut self, _info: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let decl = {
            Decl::container(Id::str("root"), NodeKind::Column)
                .background(Color::from_rgba8(24, 26, 32, 255), Corners::ZERO)
                .child(text_decl(
                    "t1",
                    "crabsnap: real glyphs through AppLoop",
                    22.0,
                    Color::from_rgba8(235, 238, 245, 255),
                ))
                .child(text_decl(
                    "t2",
                    "Subpixel positions keep the baseline steady.",
                    16.0,
                    Color::from_rgba8(150, 200, 255, 255),
                ))
                .child(text_decl(
                    "t3",
                    "中日韩字形走整像素相位",
                    16.0,
                    Color::from_rgba8(255, 210, 140, 255),
                ))
                .child(text_decl(
                    "t4",
                    "مرحبا بالعالم",
                    18.0,
                    Color::from_rgba8(160, 240, 180, 255),
                ))
        };
        ui_platform::from_decl(decl)
    }
}
fn write_ppm(path: &str, pixels: &ui_paint::Pixels) {
    let mut f = std::fs::File::create(path).expect("create ppm");
    write!(f, "P6\n{} {}\n255\n", pixels.width, pixels.height).expect("header");
    for px in pixels.data.as_chunks::<4>().0 {
        // `Pixels` stores premultiplied BGRA; PPM wants straight RGB.
        let a = px[3] as u32;
        let unmul = |c: u8| {
            if a == 0 {
                0
            } else {
                ((c as u32 * 255) / a).min(255) as u8
            }
        };
        let _ = f.write_all(&[unmul(px[2]), unmul(px[1]), unmul(px[0])]);
    }
}

fn app_with_stack(stack: &[&str]) -> AppLoop<Demo> {
    AppLoop::with_stack(Demo, stack)
}

fn main() {
    // Explicit stack: coverage must not depend on which families fontconfig happens
    // to enumerate, and some listed families have no loadable font behind them.
    let stack = ["MiSans VF", "Tahoma"];
    let mut app = app_with_stack(&stack);
    let mut target = OffscreenTarget::new(W, H);

    // Two frames: the first pays for atlas rasterization, the second proves the
    // resulting glyphs are what gets drawn once the atlas is uploaded.
    for _ in 0..2 {
        let outcome = app.frame(&mut target, None);
        println!(
            "frame: nodes={} batches={} damage={:?} glyphs={} text={} unresolved={} atlas_v={}",
            outcome.nodes,
            app.draw_list().plan_batches().len(),
            outcome.damage.is_none(),
            app.draw_list().prims.glyphs.len(),
            app.draw_list().prims.texts.len(),
            app.text_pass().unresolved(),
            app.text_pass().version(),
        );
    }

    write_ppm("apploop_text.ppm", &target.pixels);
    println!("wrote apploop_text.ppm");
}
