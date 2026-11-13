//! M2 acceptance: the same scene drawn through widgets + the legacy pack must produce
//! the same pixels as the hand-written literals. The hand-written side re-expresses the
//! example's inline scene; the widget side reads only the pack.

use std::cell::RefCell;
use std::rc::Rc;

use ui_core::geom::{Color, Corners, Insets, Point};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeData, NodeKind, TextAlign, TextBlock, TextRun, TextStyle};
use ui_platform::{App, AppLoop, OffscreenTarget};
use ui_style::{Style, legacy_pack};
use ui_widgets::{Button, Card, List, Panel};

const CARD_W: f32 = 320.0;
const ROWS: usize = 6;
const ROW_H: f32 = 22.0;
const SCROLL_H: f32 = 66.0;
const STACK: [&str; 1] = ["sans-serif"];

/// Records the `Decl` each frame into a shared cell so the test can compare it later.
#[derive(Clone)]
struct Recorder(Rc<RefCell<Option<Decl>>>);

impl Recorder {
    fn record(&self, d: Decl) {
        *self.0.borrow_mut() = Some(d);
    }
    fn take(&self) -> Decl {
        self.0.borrow_mut().take().expect("view ran")
    }
}

struct WidgetApp {
    pack: Style,
    rec: Recorder,
}

impl App for WidgetApp {
    fn view(&mut self, info: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let decl = {
            // The state query reads from the build context; parity builds one per frame.
            let mut store = ui_core::AnimStore::new();
            let mut core = ui_core::cx::Cx::new(
                &mut store,
                info.frame(),
                info.width(),
                info.height(),
                info.now(),
                info.dt(),
                1.0,
                info.pointer(),
            );
            core.route(ui_core::cx::Routing {
                hovered: info.hovered(),
                pressed: info.pressed(),
                focused: info.focused(),
                hovered_path: info.hovered_path().to_vec(),
                ..ui_core::cx::Routing::default()
            });
            let cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
            let pack = &self.pack;
            let button = Button::new(Id::str("faded"), "gpu group opacity");
            let list = List {
                id: Id::str("scroller"),
                width: CARD_W - 24.0,
                height: SCROLL_H,
                row_height: ROW_H,
                rows: (0..ROWS).map(|i| format!("scroll row {i}")).collect(),
                pack,
            };
            let panel = Panel {
                id: Id::str("glass"),
                size: (260.0, 120.0),
                offset: (40.0, 190.0),
                dragging: false,
                title: "frosted".into(),
                hint: "drag me".into(),
                title_ink: Some(Color::from_rgba8(255, 255, 255, 240)),
                hint_ink: Some(Color::from_rgba8(235, 240, 255, 210)),
                pack,
            };
            let card = Card {
                id: Id::str("card"),
                title: Some("GPU zero-copy + layer".into()),
                title_ink: Some(Color::WHITE),
                pack,
            };
            let decl = Decl::container(Id::str("root"), NodeKind::Row)
                .background(Color::from_rgba8(18, 20, 26, 255), Corners::ZERO)
                .child(card.resolve(vec![
                    button.resolve(cx.state(button.id), pack, 0.0),
                    list.resolve(),
                ]))
                .child(panel.resolve());
            self.rec.record(decl.clone());
            decl
        };
        ui_platform::from_decl(decl)
    }
}
struct LiteralApp {
    rec: Recorder,
}

impl App for LiteralApp {
    fn view(&mut self, _info: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let decl = hand_written();
        self.rec.record(decl.clone());
        ui_platform::from_decl(decl)
    }
}

/// The example's scene spelled with inline literals.
fn hand_written() -> Decl {
    let label = |id: Id, body: String, size: f32, color: Color, weight: ui_core::Weight| -> Decl {
        Decl {
            id,
            kind: NodeKind::Text,
            shapes: Vec::new(),
            data: NodeData::Text {
                ink: color,
                block: std::rc::Rc::new(TextBlock {
                    runs: vec![TextRun::new(body, TextStyle::new(size).weight(weight))],
                    align: TextAlign::Start,
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
                width: AxisSize::hug(0.0, f32::INFINITY),
                height: AxisSize::hug(size, size * 1.4),
                grow_x: Grow::No,
                grow_y: Grow::No,
            },
            ink_margin: Insets::ZERO,
            box_model: Default::default(),
            offset: Point::new(0.0, 0.0),
            cursor: None,
            sense: ui_core::Sense::NONE,
            disabled: false,
            grid: Default::default(),
            placement: Default::default(),
            env: None,
            children: Vec::new(),
        }
    };
    let pill = Decl::container(Id::str("faded"), NodeKind::Column)
        .background(Color::from_rgba8(255, 255, 255, 255), Corners::all(6.0))
        .border(Color::TRANSPARENT, 2.0)
        .padding(Insets {
            left: 8.0,
            right: 8.0,
            top: 4.0,
            bottom: 4.0,
        })
        .cursor(ui_core::CursorShape::PointingHand)
        .child(label(
            Id::str("faded_label"),
            "gpu group opacity".into(),
            13.0,
            Color::from_rgba8(20, 30, 50, 255),
            ui_core::Weight::REGULAR,
        ));
    let mut scroller = Decl::container(Id::str("scroller"), NodeKind::Column)
        .size(SizeClass::fixed(CARD_W - 24.0, SCROLL_H))
        .background(Color::from_rgba8(30, 60, 110, 255), Corners::all(6.0))
        .scrollable()
        .scroll_y(0.0);
    for i in 0..ROWS {
        scroller = scroller.child(
            Decl::container(Id::str("row").with(i), NodeKind::Column)
                .size(SizeClass::fixed(CARD_W - 40.0, ROW_H))
                .child(label(
                    Id::str("row_label").with(i),
                    format!("scroll row {i}"),
                    13.0,
                    Color::from_rgba8(210, 225, 250, 255),
                    ui_core::Weight::REGULAR,
                )),
        );
    }
    let card = Decl::container(Id::str("card"), NodeKind::Column)
        .background(
            Color::from_rgba8(60, 130, 220, 255),
            Corners::all(10.0).squircle(),
        )
        .border(Color::from_rgba8(210, 225, 245, 255), 2.0)
        .padding(Insets {
            left: 12.0,
            right: 12.0,
            top: 10.0,
            bottom: 10.0,
        })
        .child(label(
            Id::str("card_title"),
            "GPU zero-copy + layer".into(),
            18.0,
            Color::WHITE,
            ui_core::Weight::BOLD,
        ))
        .child(pill)
        .child(scroller);
    let glass = Decl::container(Id::str("glass"), NodeKind::Overlay)
        .cursor(ui_core::CursorShape::Grab)
        .offset(40.0, 190.0)
        .size(SizeClass::fixed(260.0, 120.0))
        // The pack's overlay level: the floating panel's own elevation.
        .elevation(3)
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
        .child(label(
            Id::str("glass_title"),
            "frosted".into(),
            16.0,
            Color::from_rgba8(255, 255, 255, 240),
            ui_core::Weight::BOLD,
        ))
        .child(label(
            Id::str("glass_hint"),
            "drag me".into(),
            12.0,
            Color::from_rgba8(235, 240, 255, 210),
            ui_core::Weight::REGULAR,
        ));
    Decl::container(Id::str("root"), NodeKind::Row)
        .background(Color::from_rgba8(18, 20, 26, 255), Corners::ZERO)
        .child(card)
        .child(glass)
}

/// One frame through the CPU rasterizer: the resulting pixels plus the recorded tree.
fn draw<A: App + DeclOwner>(app: A) -> (Vec<u8>, Decl) {
    let mut target = OffscreenTarget::new(900, 600);
    let mut lp = AppLoop::with_stack(app, &STACK);
    lp.frame(&mut target, None);
    let pixels = target.pixels.as_bytes().to_vec();
    // The loop owns the app; the recorder handle was cloned out before it moved in.
    let decl = lp.app_mut().rec().take();
    (pixels, decl)
}

/// Both apps expose their recorder so the harness can read the produced tree back.
trait DeclOwner {
    fn rec(&self) -> Recorder;
}

impl DeclOwner for LiteralApp {
    fn rec(&self) -> Recorder {
        self.rec.clone()
    }
}

impl DeclOwner for WidgetApp {
    fn rec(&self) -> Recorder {
        self.rec.clone()
    }
}

#[test]
fn widgets_and_hand_written_literals_draw_identically() {
    let (lit_pixels, lit_tree) = draw(LiteralApp {
        rec: Recorder(Rc::new(RefCell::new(None))),
    });
    let (wid_pixels, wid_tree) = draw(WidgetApp {
        pack: legacy_pack(),
        rec: Recorder(Rc::new(RefCell::new(None))),
    });
    assert_eq!(
        lit_pixels, wid_pixels,
        "the widget path must not repaint anything the literals did not (tree:\n{lit_tree:#?}\n{wid_tree:#?})"
    );
}
