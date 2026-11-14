//! Drag and drop through the framework: payloads out to the desktop, and in from it.
//!
//! Run: `cargo run -p ui-gallery --example dnd --release`
//!
//! The three cards on the left carry `text/plain` and ask to leave the app, so one can be
//! dragged into a text editor; the panel on the right accepts `text/*` and reports what it
//! took. Dropping a file from the desktop shows the same panel taking `text/uri-list`.

use ui_core::change::Change;
use ui_core::cx::Cx;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::SizeClass;
use ui_core::{Decl, DragData, Id, NodeKind, Weight};
use ui_platform::view::{App, AppLoop};
use ui_platform::window::SurfaceSpec;
use ui_platform::{Container, Element, SessionOptions};

use ui_host::Session;

const BACKDROP: Color = Color::from_rgba8(20, 22, 28, 255);
const CARD: Color = Color::from_rgba8(38, 41, 50, 255);
const CARD_OVER: Color = Color::from_rgba8(46, 62, 74, 255);
const INK: Color = Color::from_rgba8(235, 238, 245, 255);
const MUTED: Color = Color::from_rgba8(150, 156, 170, 255);
const ACCENT: Color = Color::from_rgba8(120, 190, 255, 255);
const BORDER: Color = Color::from_rgba8(64, 69, 80, 255);

/// The demo's own state: what arrived, and how our outgoing drags ended.
#[derive(Default)]
struct Dnd {
    log: Vec<String>,
}

/// The three payloads the left column offers.
const NAMES: [&str; 3] = ["alpha.txt", "beta.txt", "gamma.txt"];

fn source_id(name: &'static str) -> Id {
    Id::of(("source", name))
}

fn zone_id() -> Id {
    Id::str("drop-zone")
}

fn text(id: Id, body: impl Into<String>, size: f32, colour: Color, max_width: f32) -> Decl {
    ui_widgets::leaf(
        id,
        body.into(),
        size,
        colour,
        Weight::REGULAR,
        max_width,
        0.0,
        0.0,
    )
}

/// One draggable card: the payload rides the node the press starts on.
fn source(name: &'static str) -> Box<dyn Element> {
    let id = source_id(name);
    Box::new(
        Container::new(id, NodeKind::Row)
            .size(SizeClass::fixed(200.0, 44.0))
            .padding(Insets::all(12.0))
            .background(CARD, Corners::all(10.0))
            .drag_data(DragData::bytes("text/plain", name))
            // The same gesture also leaves the app: dropping into an editor works.
            .drag_out()
            .child_decl(text(id.with("label"), name, 14.0, INK, 176.0)),
    )
}

/// The drop target; its highlight reads the engine's own hover fact.
fn zone(cx: &mut Cx<'_>) -> Box<dyn Element> {
    let id = zone_id();
    let over = cx.drag_over_within(id) || cx.hovered_within(id);
    let (fill, border) = if over {
        (CARD_OVER, ACCENT)
    } else {
        (CARD, BORDER)
    };
    let mut column = Container::new(id, NodeKind::Column)
        .size(SizeClass::fixed(320.0, 240.0))
        .padding(Insets::all(18.0))
        .gap(10.0)
        .background(fill, Corners::all(14.0))
        .border(border, 2.0)
        // A pattern family takes its members; `text/plain` and `text/uri-list` both fit.
        .accept_drop(["text/*"]);
    column = column.child_decl(text(
        id.with("title"),
        if over {
            "Release to take it"
        } else {
            "Drop here"
        },
        18.0,
        if over { ACCENT } else { INK },
        284.0,
    ));
    column = column.child_decl(text(
        id.with("hint"),
        "text, files from the desktop, and anything else that speaks text/*",
        12.0,
        MUTED,
        284.0,
    ));
    Box::new(column)
}

impl App for Dnd {
    fn view(&mut self, cx: &mut Cx<'_>) -> Box<dyn Element> {
        let width = cx.width() as f32;
        let height = cx.height() as f32;
        let mut left = Container::new(Id::str("left"), NodeKind::Column)
            .gap(10.0)
            .size(SizeClass::fixed(200.0, height));
        left = left.child_decl(text(
            Id::str("left/title"),
            "Drag a card out",
            16.0,
            INK,
            200.0,
        ));
        left = left.child_decl(text(
            Id::str("left/hint"),
            "Drop one on the panel, or into another app that accepts text.",
            12.0,
            MUTED,
            200.0,
        ));
        for name in NAMES {
            left = left.child(source(name));
        }
        let recent = self
            .log
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let mut body = Container::new(Id::str("body"), NodeKind::Row)
            .size(SizeClass::fixed(width, height))
            .padding(Insets::all(24.0))
            .gap(24.0)
            .background(BACKDROP, Corners::ZERO)
            .child(Box::new(left))
            .child(zone(cx));
        body = body.child_decl(text(
            Id::str("body/log"),
            if recent.is_empty() {
                String::from("No drops yet.")
            } else {
                recent
            },
            13.0,
            MUTED,
            (width - 24.0 * 2.0 - 200.0 - 320.0 - 24.0 * 2.0).max(120.0),
        ));
        Box::new(body)
    }

    fn changed(&mut self, id: Id, change: Change) -> bool {
        match change {
            Change::DroppedOn(data) => {
                let what = data
                    .text()
                    .map(|text| text.lines().next().unwrap_or_default().to_owned())
                    .or_else(|| data.types().next().map(str::to_owned))
                    .unwrap_or_else(|| String::from("an empty payload"));
                self.log.insert(0, format!("took {what}"));
                true
            }
            Change::DragEntered { types } => {
                self.log.insert(0, format!("over: {}", types.join(", ")));
                true
            }
            // The source side: another app took the payload, or the gesture was cancelled.
            Change::Dropped(None) => {
                if let Some(name) = NAMES.iter().find(|name| source_id(name) == id) {
                    self.log.insert(0, format!("{name} left the app"));
                }
                true
            }
            Change::Cancelled if NAMES.iter().any(|name| source_id(name) == id) => {
                self.log.insert(0, String::from("drag cancelled"));
                true
            }
            _ => false,
        }
    }
}

fn main() {
    let mut lp = AppLoop::with_stack(Dnd::default(), &["Inter"]);
    // The engine ships no faces; the host supplies the one the demo's text asks for.
    lp.register_font(
        "Inter",
        include_bytes!("../../../assets/fonts/InterVariable.ttf")
            .as_slice()
            .into(),
    );
    let opts = SessionOptions {
        surface: SurfaceSpec::window(
            "dev.crabsnap.ui.dnd",
            "crabsnap-ui   drag and drop",
            (900, 560),
        ),
        uncapped: false,
        cpu_presenter: std::env::var("DND_CPU").is_ok(),
    };
    let mut session = match Session::open(lp, opts) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    while !session.finished() {
        if let Err(error) = session.tick() {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
