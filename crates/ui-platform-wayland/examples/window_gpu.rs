//! Zero-copy window driven by `AppLoop`: real widget tree, real glyphs, dma-buf out.
//!
//! Run: `cargo run -p ui-platform-wayland --example window_gpu \
//!        --features "gpu,system-fonts"`

use std::time::Instant;

use ui_core::geom::{Color, Corners, Insets};
use ui_core::{AxisSize, Decl, Grow, Id, NodeData, NodeKind, SizeClass};
use ui_platform::window::SurfaceSpec;
use ui_platform::{App, AppLoop};
use ui_platform_wayland::{ColourPolicy, GpuWindow, GpuWindowConfig, WaylandWindow};

// Tahoma first: it ships a real Bold face, so the title's weight is visible.
// MiSans VF stays as the CJK fallback — it is a variable font whose weight lives on
// the `wght` axis, which this renderer does not apply yet.
const STACK: [&str; 2] = ["Tahoma", "MiSans VF"];

/// Fixed card width so the sliding offset stays predictable.
const CARD_W: f32 = 320.0;
/// Rows in the scrollable list, and the height each row and the viewport take.
const ROWS: usize = 6;
const ROW_H: f32 = 22.0;
const SCROLL_H: f32 = 66.0;

fn label(id: &'static str, body: String, size: f32, color: Color) -> Decl {
    Decl {
        id: Id::str(id),
        kind: NodeKind::Text,
        shapes: Vec::new(),
        data: NodeData::Text {
            ink: color,
            block: std::sync::Arc::new(ui_core::TextBlock {
                runs: vec![ui_core::TextRun::new(body, ui_core::TextStyle::new(size))],
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
            width: AxisSize::hug(0.0, CARD_W - 24.0),
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
        drag: None,
        drag_out: false,
        drop_types: None,
        children: Vec::new(),
    }
}

/// Same as [`label`], for ids that are not compile-time constants.
fn label_dyn(id: Id, body: String, size: f32, color: Color) -> Decl {
    let mut d = label("dyn", body, size, color);
    d.id = id;
    d
}

/// Same as [`label`], with a weight so the bold face must actually be reached.
fn bold(id: &'static str, body: String, size: f32, color: Color) -> Decl {
    let mut d = label(id, body, size, color);
    if let ui_core::NodeData::Text { block, .. } = &mut d.data
        && let Some(run) = std::sync::Arc::make_mut(block).runs.first_mut()
    {
        run.style.weight = ui_core::Weight::BOLD;
    }
    d
}

/// A paragraph with a first-line indent and extra line spacing.
fn paragraph(id: &'static str, body: String, size: f32, color: Color) -> Decl {
    let mut d = label(id, body, size, color);
    if let ui_core::NodeData::Text { block, .. } = &mut d.data {
        let block = std::sync::Arc::make_mut(block);
        block.indent = 24.0;
        block.line_gap = 6.0;
        d.size_class.height = ui_core::layout::AxisSize::hug(size, size * 8.0);
    }
    d
}

/// A widget tree whose card slides, so the GPU path is visibly animating.
struct Demo {
    phase: f32,
    /// Freeze the slide so hover and press can be exercised by hand.
    frozen: bool,
    frames: u32,
    /// Keyboard activations of the pill, so Tab + Enter is visible end to end.
    activated: u32,
    /// Wheel offset of the list inside the card; the app owns it, the view reads it.
    scroll: f32,
    /// Last focused node, so a focus change is visible in the log while testing.
    last_focused: Option<Id>,
    /// Engine-side animations; the framework advances it and reads the values.
    anim: ui_core::AnimStore,
    /// Horizontal drag of the card: committed offset, plus the drag's own baseline.
    drag_offset: f32,
    drag_from: Option<f32>,
    /// The frosted panel's own offset, so it can be dragged over the card.
    glass_offset: f32,
    glass_y: f32,
    /// The panel's offset when the drag began; `Started.from` is the *press point*.
    glass_drag_origin: Option<(f32, f32)>,
}

impl App for Demo {
    /// Enter or Space on the focused pill proves the keyboard path reaches the app.

    fn changed(&mut self, on: Id, change: ui_core::Change) -> bool {
        use ui_core::Change as C;
        // The wheel arrives as a value change on the scroller: the app owns the offset.
        if on == Id::str("scroller") {
            if let C::Value(v) = change {
                let max = ROWS as f32 * ROW_H - SCROLL_H;
                let next = (self.scroll + v as f32).clamp(0.0, max);
                if (next - self.scroll).abs() < 0.5 {
                    return false;
                }
                self.scroll = next;
                return true;
            }
            return false;
        }
        if on == Id::str("faded") {
            if matches!(change, C::Activated) {
                self.activated += 1;
                return true;
            }
            return false;
        }
        // The frosted panel moves in both axes, so it has its own branch.
        let glass = Id::str("glass");
        if on == glass {
            match change {
                // Snapshot the offset, not the press point: only the offset added to the
                // pointer's travel keeps the panel under the cursor.
                C::Activated => self.glass_drag_origin = Some((self.glass_offset, self.glass_y)),
                C::Moved { dx, dy, .. } => {
                    if let Some((x0, y0)) = self.glass_drag_origin {
                        self.glass_offset = x0 + dx as f32;
                        self.glass_y = y0 + dy as f32;
                    }
                }
                C::Selected(_) => self.glass_drag_origin = None,
                C::Cancelled => {
                    if let Some((x0, y0)) = self.glass_drag_origin {
                        self.glass_offset = x0;
                        self.glass_y = y0;
                    }
                    self.glass_drag_origin = None;
                }
                _ => return false,
            }
            return true;
        }
        if on != Id::str("card") {
            return false;
        }
        match change {
            // Remember where the card was when the drag began, so deltas stay absolute.
            C::Activated => {
                self.drag_from = Some(self.drag_offset);
                true
            }
            C::Moved { dx, .. } => {
                if let Some(from) = self.drag_from {
                    self.drag_offset = from + dx as f32;
                }
                true
            }
            C::Selected(_) => {
                // A drop keeps the offset.
                self.drag_from = None;
                true
            }
            C::Cancelled => {
                // A cancel restores the pre-drag one.
                if let Some(from) = self.drag_from {
                    self.drag_offset = from;
                }
                self.drag_from = None;
                true
            }
            _ => false,
        }
    }

    fn view(&mut self, info: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let decl = {
            if info.focused() != self.last_focused {
                println!(
                    "focus -> {:?}   (scroll {:.0})",
                    format!("{:?}", info.focused()),
                    self.scroll
                );
                self.last_focused = info.focused();
            }
            let travel = (info.width() as f32 - CARD_W).max(0.0);
            let base = if self.frozen {
                0.0
            } else {
                (self.phase.sin() * 0.5 + 0.5) * travel
            };
            // Paint offset, not a spacer: the card moves without reflowing, so its hit box
            // and its pixels stay in step even at speed.
            let offset = (base + self.drag_offset).clamp(0.0, travel);

            // Tab can land on the card too, so it needs a visible focus state as well.
            let card_focused = info.focused() == Some(Id::str("card"));
            // Hover fades the card instead of switching colour: a hard switch reads as a
            // different widget, a fade reads as the same widget reacting.
            let card_hovered = info.hovered_within(Id::str("card"));
            let hover = self.anim.animate(
                Id::str("card"),
                if card_hovered { 1.0 } else { 0.0 },
                ui_core::AnimSpec::new(0.18, ui_core::Easing::EaseOut),
                info.now(),
            );
            let base = if card_focused {
                Color::from_rgba8(220, 40, 40, 255)
            } else {
                Color::from_rgba8(60, 130, 220, 255)
            };
            let lifted = Color::from_rgba8(110, 175, 245, 255);
            // Grabbing while a drag is live, Grab when it can be grabbed: the shape must
            // follow the gesture state, which only the app knows.
            let card_cursor = if self.drag_from.is_some() {
                ui_core::CursorShape::Grabbing
            } else {
                ui_core::CursorShape::Grab
            };
            let mut card = Decl::container(Id::str("card"), NodeKind::Column)
                // The card subscribes to hover and drags; the router reads only this.
                .sense(ui_core::Sense::HOVER | ui_core::Sense::DRAG | ui_core::Sense::FOCUSABLE)
                .cursor(card_cursor)
                .offset(offset, 0.0)
                .background(base.lerp(lifted, hover), Corners::all(10.0).squircle())
                .border(
                    if card_focused {
                        Color::from_rgba8(255, 240, 120, 255)
                    } else {
                        Color::from_rgba8(210, 225, 245, 255)
                    },
                    2.0,
                )
                .padding(Insets {
                    left: 12.0,
                    right: 12.0,
                    top: 10.0,
                    bottom: 10.0,
                })
                .child(bold(
                    "title",
                    "GPU zero-copy + layer".into(),
                    18.0,
                    Color::WHITE,
                ))
                .child(bold(
                    "cjk",
                    "字形来自同一条流".into(),
                    14.0,
                    Color::from_rgba8(255, 235, 180, 255),
                ))
                .child(paragraph(
                    "para",
                    "首行缩进二十四点，行距也加宽，用来验证测量与绘制用的是同一套间距规则。".into(),
                    12.0,
                    Color::from_rgba8(225, 235, 250, 255),
                ))
                .child(label(
                    "rtl",
                    "مرحبا".into(),
                    15.0,
                    Color::from_rgba8(190, 255, 210, 255),
                ))
                .child(
                    // A faded group inside the card: exercises the GPU FBO layer path.
                    Decl::container(Id::str("faded"), NodeKind::Column)
                        // The faded group is a keyboard stop, exactly like the card.
                        .sense(ui_core::Sense::FOCUSABLE)
                        .opacity(0.55)
                        .background(
                            // Three states from the loop: pressed beats hovered beats idle.
                            if info.pressed() == Some(Id::str("faded")) {
                                Color::from_rgba8(255, 170, 90, 255)
                            } else if info.hovered() == Some(Id::str("faded")) {
                                Color::from_rgba8(255, 240, 170, 255)
                            } else {
                                Color::from_rgba8(255, 255, 255, 255)
                            },
                            Corners::all(6.0),
                        )
                        // Focus comes from the loop: click takes it, Escape releases it.
                        .border(
                            if info.focused() == Some(Id::str("faded")) {
                                Color::from_rgba8(20, 30, 50, 255)
                            } else {
                                Color::TRANSPARENT
                            },
                            2.0,
                        )
                        .padding(Insets {
                            left: 8.0,
                            right: 8.0,
                            top: 4.0,
                            bottom: 4.0,
                        })
                        .child(label(
                            "faded_label",
                            "gpu group opacity".into(),
                            13.0,
                            Color::from_rgba8(20, 30, 50, 255),
                        )),
                )
                .child(label(
                    "counter",
                    // Not the frame number: text that changes every draw makes every draw
                    // necessary, which hides whether on-demand redraw works.
                    if self.activated == 0 {
                        "idle unless input".to_string()
                    } else {
                        format!("activated {}", self.activated)
                    },
                    12.0,
                    Color::from_rgba8(225, 235, 250, 255),
                ));

            // A list taller than its viewport: the wheel must move it and the mask must cut it.
            let mut scroller = Decl::container(Id::str("scroller"), NodeKind::Column)
                .size(SizeClass::fixed(CARD_W - 24.0, SCROLL_H))
                .background(Color::from_rgba8(30, 60, 110, 255), Corners::all(6.0))
                .scrollable()
                .scroll_y(self.scroll);
            for i in 0..ROWS {
                // Each row needs its own id: the tree keys nodes by `Id`, so duplicates
                // silently collapse into one node and only the last text survives.
                scroller = scroller.child(
                    Decl::container(Id::str("row").with(i), NodeKind::Column)
                        .size(SizeClass::fixed(CARD_W - 40.0, ROW_H))
                        .child(label_dyn(
                            Id::str("row_label").with(i),
                            format!("scroll row {i}"),
                            13.0,
                            Color::from_rgba8(210, 225, 250, 255),
                        )),
                );
            }
            card = card.child(scroller);

            // A blurred group: the GPU effect passes must run against a real compositor.
            card = card.child(
                Decl::container(Id::str("glow"), NodeKind::Column)
                    .blur(8, 3.0)
                    .child(label(
                        "glow_label",
                        "gpu blur".into(),
                        22.0,
                        Color::from_rgba8(255, 205, 110, 255),
                    )),
            );

            // A floating frosted panel that can be dragged over the card. It blurs whatever
            // is already painted beneath it, which is exactly what a paint offset stresses:
            // the blurred backdrop must follow the panel, not stay where layout put it.
            // `Overlay`: the panel floats above the flow, so the card's sliding and any
            // sibling resizing cannot drag it around. A normal child would move with them.
            let glass = Decl::container(Id::str("glass"), NodeKind::Overlay)
                .sense(ui_core::Sense::HOVER | ui_core::Sense::DRAG | ui_core::Sense::FOCUSABLE)
                .cursor(if self.glass_drag_origin.is_some() {
                    ui_core::CursorShape::Grabbing
                } else {
                    ui_core::CursorShape::Grab
                })
                .offset(self.glass_offset, self.glass_y)
                .size(SizeClass::fixed(260.0, 120.0))
                .backdrop_blur(14.0)
                .background(
                    Color::from_rgba8(255, 255, 255, 90),
                    Corners::all(14.0).squircle(),
                )
                .border(Color::from_rgba8(255, 255, 255, 90), 1.0)
                .padding(Insets {
                    left: 14.0,
                    right: 14.0,
                    top: 10.0,
                    bottom: 10.0,
                })
                .child(bold(
                    "glass_title",
                    "frosted".into(),
                    16.0,
                    Color::from_rgba8(255, 255, 255, 240),
                ))
                .child(label(
                    "glass_hint",
                    "drag me".into(),
                    12.0,
                    Color::from_rgba8(235, 240, 255, 210),
                ));

            // `Stack` is a declared kind the layout engine does not implement, so the panel
            // rides on the card as a zero-size anchor displaced by a paint offset.
            // The panel is a sibling of the card, not a child: as a child it would take part
            // in the card's layout and squeeze it, which shows up as the card drifting while
            // dragged. As a sibling it only adds to the row's width.
            Decl::container(Id::str("root"), NodeKind::Row)
                .background(Color::from_rgba8(18, 20, 26, 255), Corners::ZERO)
                .child(card)
                .child(glass)
        };
        ui_platform::from_decl(decl)
    }
}

/// `wl_shm` presenter: the same scene through the CPU rasterizer.

/// A scripted pointer gesture for verifying drag without a synthetic-button tool.
///
/// The events go through the real `dispatch`, so this exercises the same routing a
/// compositor would drive; only the compositor's own delivery is skipped.
/// Panel position from `CRABSNAP_GLASS_AT=x,y`, for parking it against a chosen edge.
fn glass_at() -> Option<(f32, f32)> {
    let raw = std::env::var("CRABSNAP_GLASS_AT").ok()?;
    let (x, y) = raw.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn autodrag(frame: u32) -> Option<ui_core::Event> {
    let p = |x: f32, y: f32| ui_core::Event::PointerMoved {
        pos: ui_core::geom::Point::new(x, y),
    };
    // `CRABSNAP_DRAG_GLASS` targets the floating panel instead of the card, so the
    // two-axis drag can be verified without a mouse.
    // `CRABSNAP_AB` reproduces the hover-then-leave sequence that showed a mismatch.
    if std::env::var("CRABSNAP_AB").is_ok() {
        return match frame {
            5 => Some(p(400.0, 300.0)),
            600 => Some(p(1500.0, 900.0)),
            _ => None,
        };
    }
    // Drags the panel to the window's bottom edge, the case where the blur's band runs
    // past the frame and would otherwise read texels nothing ever wrote.
    if std::env::var("CRABSNAP_DRAG_LOW").is_ok() {
        // Press inside the floating frost panel (start it with GLASS_AT=340,190) and pull it
        // down to the window's bottom edge.
        let start = (450.0_f32, 250.0_f32);
        let end = (450.0_f32, 1380.0_f32);
        return match frame {
            5 => Some(p(start.0, start.1)),
            6 => Some(ui_core::Event::PointerPressed {
                pos: ui_core::geom::Point::new(start.0, start.1),
                button: ui_core::PointerButton::Primary,
            }),
            f @ 7..=60 => {
                let t = (f - 6) as f32 / 54.0;
                Some(p(start.0, start.1 + (end.1 - start.1) * t))
            }
            61 => Some(ui_core::Event::PointerReleased {
                pos: ui_core::geom::Point::new(end.0, end.1),
                button: ui_core::PointerButton::Primary,
            }),
            _ => None,
        };
    }
    if std::env::var("CRABSNAP_DRAG_GLASS").is_ok() {
        return match frame {
            5 => Some(p(200.0, 180.0)),
            6 => Some(ui_core::Event::PointerPressed {
                pos: ui_core::geom::Point::new(200.0, 180.0),
                button: ui_core::PointerButton::Primary,
            }),
            f @ 7..=40 => Some(p(
                200.0 + (f - 6) as f32 * 8.0,
                180.0 + (f - 6) as f32 * 4.0,
            )),
            41 => Some(ui_core::Event::PointerReleased {
                pos: ui_core::geom::Point::new(200.0 + 34.0 * 8.0, 180.0 + 34.0 * 4.0),
                button: ui_core::PointerButton::Primary,
            }),
            _ => None,
        };
    }
    match frame {
        5 => Some(p(300.0, 400.0)),
        6 => Some(ui_core::Event::PointerPressed {
            pos: ui_core::geom::Point::new(300.0, 400.0),
            button: ui_core::PointerButton::Primary,
        }),
        f @ 7..=40 => Some(p(300.0 + (f - 6) as f32 * 18.0, 400.0)),
        41 => Some(ui_core::Event::PointerReleased {
            pos: ui_core::geom::Point::new(300.0 + 34.0 * 18.0, 400.0),
            button: ui_core::PointerButton::Primary,
        }),
        _ => None,
    }
}

fn run_cpu(mut win: WaylandWindow) {
    let mut app = AppLoop::with_stack(
        Demo {
            phase: 0.0,
            frozen: true,
            frames: 0,
            activated: 0,
            scroll: 0.0,
            last_focused: None,
            anim: ui_core::AnimStore::new(),
            drag_offset: 0.0,
            drag_from: None,
            // keeps it over the dark background so the demo looks unchanged.
            glass_offset: glass_at().map(|p| p.0).unwrap_or(40.0),
            // Probe position: straddles the card edge so the frost has detail under it.
            glass_y: glass_at().map(|p| p.1).unwrap_or(190.0),
            glass_drag_origin: None,
        },
        &STACK,
    );
    println!(
        "cpu presenter: logical {:?} scale {:.2}; text face: {}",
        win.logical_size(),
        win.scale(),
        app.text_pass().has_face()
    );

    let mut last_scale = 0.0f32;
    let mut needs_redraw = true;
    let mut last_frame = Instant::now();
    // Runs until the compositor closes the window; a background timer made captures
    // race the shutdown and silently photograph the desktop instead.
    while !win.should_close() {
        if win.logical_size().0 == 0 || win.logical_size().1 == 0 {
            win.wait();
            continue;
        }
        if (win.scale() - last_scale).abs() > 0.001 {
            last_scale = win.scale();
            app.set_scale(last_scale);
            needs_redraw = true;
        }
        for ev in win.take_events() {
            needs_redraw |= app.dispatch(ev);
        }
        if !needs_redraw {
            win.wait();
            continue;
        }
        if !win.can_draw() {
            win.wait_for_frame();
            if !win.can_draw() {
                win.pump();
                continue;
            }
        }
        let pointer = win.pointer();
        app.app_mut().frames += 1;
        let now = Instant::now();
        let dt = (now - last_frame).as_secs_f32().min(0.1);
        last_frame = now;
        let outcome = app.frame_with_dt(&mut win, pointer, dt);
        win.set_cursor(outcome.cursor);
        // Keep the loop awake while something moves; otherwise wait for input.
        needs_redraw = outcome.animating;
    }
    println!("cpu presenter done: {} frames", app.app_mut().frames);
}

fn main() {
    let mut win = match WaylandWindow::new(SurfaceSpec::window(
        "dev.crabsnap.ui.gpu",
        "crabsnap-ui   gpu zero-copy",
        (520, 300),
    )) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("cannot open a Wayland window: {e}");
            std::process::exit(1);
        }
    };
    // One scene, two presenters: the backend is a flag, not a separate example.
    if std::env::var("CRABSNAP_CPU").is_ok() {
        run_cpu(win);
        return;
    }

    // Default to the GPU that drives a connected output: rendering on the other
    // device gives the compositor a buffer it imports but shows as black.
    let mut gpu = match GpuWindow::open(
        &mut win,
        GpuWindowConfig {
            render_node: std::env::args().nth(1),
            format: match std::env::var("CRABSNAP_FORMAT").as_deref() {
                Ok("10bit") => ui_core::colour::BufferFormat::Rgb10a2,
                Ok("f16") => ui_core::colour::BufferFormat::Rgba16Float,
                _ => ui_core::colour::BufferFormat::Bgra8Srgb,
            },
            colour: if std::env::var("CRABSNAP_SDR").is_ok() {
                ColourPolicy::Sdr
            } else {
                ColourPolicy::Auto
            },
        },
    ) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("GPU presenter unavailable: {e}");
            std::process::exit(1);
        }
    };
    println!("GPU: {} ({})", gpu.renderer_name(), gpu.node());
    match gpu.colour().hdr() {
        Some(surface) => println!(
            "colour: HDR, reference {} nits, {:?} buffers",
            surface.reference_nits, surface.format
        ),
        None => println!("colour: SDR, {:?} buffers", gpu.format()),
    }
    let mut app = AppLoop::with_stack(
        Demo {
            phase: 0.0,
            frozen: std::env::var("CRABSNAP_STATIC").is_ok(),
            frames: 0,
            activated: 0,
            scroll: 0.0,
            last_focused: None,
            anim: ui_core::AnimStore::new(),
            drag_offset: 0.0,
            drag_from: None,
            glass_offset: glass_at().map(|p| p.0).unwrap_or(40.0),
            // Probe position: straddles the card edge so the frost has detail under it.
            glass_y: glass_at().map(|p| p.1).unwrap_or(190.0),
            glass_drag_origin: None,
        },
        &STACK,
    );
    println!("text face: {}", app.text_pass().has_face());

    // Buffer format is a flag so 8-bit and 10-bit can be compared on the same scene.
    println!(
        "buffer format: {:?} fourcc {:#010x}",
        gpu.format(),
        gpu.fourcc()
    );
    let mut frames = 0u32;
    let mut presented: Option<(u32, u32, f32)> = None;
    // Starts true: the first frame must draw whatever the initial layout produces.
    let mut needs_redraw = true;
    let mut outcome_animating = false;
    let mut last_frame = Instant::now();

    // Runs until the compositor closes the window; a background timer made captures
    // race the shutdown and silently photograph the desktop instead.
    while !win.should_close() {
        if let Err(e) = win.pump_checked() {
            eprintln!("protocol error: {e}");
            break;
        }
        let (lw, lh) = win.logical_size();
        let scale = win.scale();
        if lw == 0 || lh == 0 {
            win.wait();
            continue;
        }
        // A resize invalidates the presented buffer, so it repaints even when idle.
        if presented != Some((lw, lh, scale)) {
            needs_redraw = true;
        }

        // Same pipeline as the CPU backend: build once, then hand the resolved list
        // (text already turned into glyphs) to the GPU renderer.
        app.set_scale(scale);
        // Input first: while idle it is the only thing that can ask for a repaint.
        if (std::env::var("CRABSNAP_AUTODRAG").is_ok()
            || std::env::var("CRABSNAP_DRAG_GLASS").is_ok()
            || std::env::var("CRABSNAP_AB").is_ok())
            && let Some(ev) = autodrag(app.app_mut().frames)
        {
            needs_redraw |= app.dispatch(ev);
        }
        for ev in win.take_events() {
            needs_redraw |= app.dispatch(ev);
        }
        // Two things keep drawing without input: the slide (app-driven) and any
        // animation still in flight. Everything else waits.
        let sliding = !app.app_mut().frozen;
        // A scripted gesture needs frames to advance even with no real input.
        let scripting = (std::env::var("CRABSNAP_AUTODRAG").is_ok()
            || std::env::var("CRABSNAP_DRAG_GLASS").is_ok()
            || std::env::var("CRABSNAP_AB").is_ok()
            || std::env::var("CRABSNAP_DRAG_LOW").is_ok())
            && app.app_mut().frames < 75;
        if !needs_redraw && !outcome_animating && !sliding && !scripting {
            // Idle: block until the seat or the compositor wakes us, instead of spinning.
            win.wait();
            continue;
        }
        app.app_mut().frames += 1;
        if !app.app_mut().frozen {
            app.app_mut().phase += 0.12;
        }
        // Feed real time: `build` without it would leave every animation at frame 0.
        let now = Instant::now();
        let dt = (now - last_frame).as_secs_f32().min(0.1);
        last_frame = now;
        let outcome = app.build_with_dt((lw, lh), win.pointer(), dt);
        outcome_animating = outcome.animating;
        win.set_cursor(outcome.cursor);

        if let Err(e) = gpu.sync_atlas(&app.text_pass()) {
            eprintln!("atlas upload failed: {e}");
            break;
        }

        match gpu.present(&mut win, (lw, lh), app.draw_list(), &outcome.damage, &[]) {
            Ok(done) => {
                if let Some(size) = done.resized {
                    println!("targets rebuilt at {size:?} (scale {})", done.scale);
                }
            }
            Err(e) => {
                eprintln!("present failed: {e}");
                break;
            }
        }
        presented = Some((lw, lh, scale));

        if std::env::var("DUMP_GPU").is_ok() && frames == 30 {
            match gpu.dump_ppm(
                std::path::Path::new("/tmp/gpu_frame.ppm"),
                std::path::Path::new("/tmp/gpu_layer.ppm"),
            ) {
                Ok(layer) => {
                    println!("dumped /tmp/gpu_frame.ppm");
                    if let Some((w, h)) = layer {
                        println!("dumped /tmp/gpu_layer.ppm ({w}x{h})");
                    }
                }
                Err(e) => eprintln!("dump failed: {e}"),
            }
        }
        // `CRABSNAP_ALWAYS_REDRAW` keeps the loop painting instead of blocking on input,
        // which is the only way to observe how repeated frames accumulate.
        needs_redraw = std::env::var("CRABSNAP_ALWAYS_REDRAW").is_ok();
        if std::env::var("CRABSNAP_COLOUR_INFO").is_ok() && frames == 0 {
            match win.preferred_luminances() {
                Some((min, max, reference)) => {
                    println!("compositor preferred: min {min} max {max} reference {reference} nits")
                }
                None => println!("compositor preferred: unavailable"),
            }
        }

        frames += 1;
        if frames.is_multiple_of(60) {
            println!(
                "frames {frames}, releases {}, inflight {}, glyphs {}, unresolved {}",
                win.dmabuf_releases(),
                win.dmabuf_inflight(),
                app.draw_list().prims.glyphs.len(),
                app.text_pass().unresolved()
            );
        }
        win.wait_for_frame();
    }

    println!(
        "done: {frames} frames, {} dma-buf releases, {} inflight",
        win.dmabuf_releases(),
        win.dmabuf_inflight()
    );
}
