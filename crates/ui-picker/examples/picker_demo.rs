//! A window that opens the file picker: a dialog surface per pack, and the in-window fallback.
//!
//! Run in a nested session: `cargo run --release -p ui-picker --example picker_demo`.
//! Knobs: `PICKER_DEMO_START=/some/dir` (default: the home place), `PICKER_DEMO_SECONDS=n`
//! (default: run until the windows close) for a scripted bound, `PICKER_DEMO_MODELESS=1` for
//! a dialog that does not hold the parent,
//! `PICKER_DEMO_INLINE=1` to draw the in-window fallback without asking the host for a
//! dialog, `PICKER_DEMO_OPEN=0|1|2` (MD3, Fluent, Glass) to open one without a click,
//! `PICKER_DEMO_DARK=0|1` to seed the appearance instead of the system's preference, and
//! `PICKER_DEMO_MODE=open|open-files|folder|folders|save|save-files` to pick the variant.
//! `PICKER_DEMO_EXTRAS=1` adds a caller place and two extra controls, which the platform
//! dialogs do not offer. `PICKER_DEMO_LOG=debug` prints the platform's own steps to the
//! console, which is how a freeze is pinned to the call it is stuck in.
//!
//! What only the screen can show: the dialog sits above the parent and holds its input while
//! open; the rows' icons and thumbnails; the sidebar disappearing when the dialog is narrowed.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{CursorShape, Decl, Id, ModalTarget, NodeKind, Sense};
use ui_design::{Appearance, CommonColor, CommonMetric};
use ui_i18n::{Args, Language, LanguageChoice, Msg};
use ui_platform::chooser::{
    ChoiceOption, ChooserChoice, ChooserError, ChooserMode, ChooserPlace, ChooserRequest,
    ChooserSelection, FileTypeFilter,
};
use ui_platform::host::{Host, OpenOptions};
use ui_platform::places::Place;
use ui_platform::view::App;
use ui_platform::window::SurfaceSpec;
use ui_platform::{Container, Element, from_decl};
use ui_picker::{
    Chooser, MIN_SIZE, MenuStyleOf, PackOf, Picker, PickerOptions, PickerOutcome, PickerState,
};
use ui_style::{Env, MotionName, StyleScope};
use ui_widgets::{Button, Cx, Pressable, Text};

const APP_ID: &str = "dev.crabsnap.ui.picker-demo";
const PARENT_SIZE: (u32, u32) = (760, 520);
const DIALOG_SIZE: (u32, u32) = (900, 600);
/// The inset the in-window fallback leaves around itself, so it reads as a modal rather than
/// as the page.
const FALLBACK_INSET: f32 = 48.0;

/// What the page and the turn handler share.
#[derive(Default)]
struct Shared {
    /// The pack index the next picker opens with; `None` means nothing was asked for.
    pack: Option<usize>,
    /// The appearance both the page and the next picker draw in.
    dark: bool,
    /// The last answer, for the page to show. One type for both implementations, which is
    /// what the demo is here to check.
    outcome: Option<Result<ChooserSelection, ChooserError>>,
}

type SharedHandle = Rc<RefCell<Shared>>;

/// Print the platform's warnings: a surface that is retired says why, so a demo that stops
/// answering leaves a line in the console instead of only a frozen window.
struct ConsoleLog;

impl log::Log for ConsoleLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{}: {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

fn install_logging() {
    static LOG: ConsoleLog = ConsoleLog;
    let _ = log::set_logger(&LOG);
    // `PICKER_DEMO_LOG=debug` names the step a freeze happened in; warnings are the default.
    let level = match std::env::var("PICKER_DEMO_LOG").as_deref() {
        Ok("debug") => log::LevelFilter::Debug,
        Ok("info") => log::LevelFilter::Info,
        Ok("off") => log::LevelFilter::Off,
        _ => log::LevelFilter::Warn,
    };
    log::set_max_level(level);
}

/// The drawn picker's answer in the contract's terms, so the readout has one shape.
fn outcome_result(outcome: PickerOutcome) -> Result<ChooserSelection, ChooserError> {
    match outcome {
        PickerOutcome::Selected(selection) => Ok(selection),
        PickerOutcome::Cancelled => Err(ChooserError::Cancelled),
    }
}
type Embedded = Rc<RefCell<Option<PickerState>>>;

struct Demo {
    shared: SharedHandle,
    /// The picker shown in this window when the host refused a dialog surface.
    embedded: Embedded,
    packs: [PackOf; 3],
    /// Each pack's engine-menu material, so a dialog's context menus follow its pack.
    menu_styles: [MenuStyleOf; 3],
    env: Arc<Env>,
}

fn pack_button(index: usize) -> Id {
    Id::of(("demo/pack", index))
}

fn appearance_button(dark: bool) -> Id {
    Id::of(("demo/appearance", dark))
}

impl Demo {
    fn appearance(&self) -> Appearance {
        if self.shared.borrow().dark {
            Appearance::Dark
        } else {
            Appearance::Light
        }
    }

    fn page(&self, cx: &mut Cx<'_, '_>, width: f32, height: f32) -> Box<dyn Element> {
        let pack = (self.packs[0])(self.appearance());
        let surface = pack.colour_or(
            CommonColor::Surface,
            Color::from_rgba8(24, 24, 28, 255),
        );
        let ink = pack.colour_or(CommonColor::OnSurface, Color::WHITE);
        let muted = pack.colour_or(CommonColor::TextMuted, Color::from_rgba8(170, 170, 180, 255));
        let radius = pack.metric_or(CommonMetric::Radius, 6.0);
        let title = pack.type_table.get(ui_style::TextRole::Title);
        let body = pack.type_table.get(ui_style::TextRole::Body);
        let caption = pack.type_table.get(ui_style::TextRole::Caption);
        let state_spec = pack.motion.get(MotionName::StateChange).spec;

        let mut children: Vec<Box<dyn Element>> = vec![
            from_decl(
                Text::new(
                    Id::str("demo/title"),
                    cx.t(Msg::PICKER_DEMO_TITLE).to_string(),
                    title.size,
                    ink,
                )
                .weight(title.weight)
                .resolve(),
            ),
            from_decl(
                Text::new(
                    Id::str("demo/hint"),
                    cx.t(Msg::PICKER_DEMO_HINT).to_string(),
                    body.size,
                    muted,
                )
                .max_width(width - 48.0)
                .resolve(),
            ),
        ];

        let mut packs: Vec<Box<dyn Element>> = Vec::new();
        for (index, label) in [
            Msg::GALLERY_STYLE_MD3,
            Msg::GALLERY_STYLE_FLUENT,
            Msg::GALLERY_STYLE_GLASS,
        ]
        .into_iter()
        .enumerate()
        {
            let id = pack_button(index);
            let state = cx.state(id);
            let lift = cx.at(id).animate(
                id.with("hover"),
                if state.hovered || state.pressed {
                    1.0
                } else {
                    0.0
                },
                state_spec,
            );
            // A bare `Decl` would paint and report nothing: the pack buttons are controls.
            let label = Button::new(id, cx.t(label).to_string())
                .resolve(state, pack, lift)
                .corners(Corners::all(radius));
            packs.push(cx.element(Box::new(Pressable::new(label))));
        }
        children.push(Box::new(
            Container::from_decl(Decl::container(Id::str("demo/packs"), NodeKind::Row).gap(8.0))
                .children(packs),
        ));

        let mut appearances: Vec<Box<dyn Element>> = Vec::new();
        for (dark, label) in [
            (false, Msg::GALLERY_APPEARANCE_LIGHT),
            (true, Msg::GALLERY_APPEARANCE_DARK),
        ] {
            let id = appearance_button(dark);
            let state = cx.state(id);
            let lift = cx.at(id).animate(
                id.with("hover"),
                if state.hovered || state.pressed {
                    1.0
                } else {
                    0.0
                },
                state_spec,
            );
            // The dialog is its own surface and cannot see this page's env, so the demo
            // states the appearance the next picker opens with.
            let button = Button::new(id, cx.t(label).to_string())
                .resolve(state, pack, lift)
                .corners(Corners::all(radius));
            appearances.push(cx.element(Box::new(Pressable::new(button))));
        }
        children.push(Box::new(
            Container::from_decl(
                Decl::container(Id::str("demo/appearances"), NodeKind::Row).gap(8.0),
            )
            .children(appearances),
        ));

        let readout = {
            let shared = self.shared.borrow();
            match &shared.outcome {
                Some(Ok(selection)) => {
                    let text = selection
                        .paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let mut args = Args::new();
                    args.set("path", text.as_str());
                    Some(cx.t_args(Msg::PICKER_DEMO_CHOSEN, &args).to_string())
                }
                Some(Err(ChooserError::Cancelled)) => {
                    Some(cx.t(Msg::PICKER_DEMO_CANCELLED).to_string())
                }
                Some(Err(error)) => {
                    let reason = error.to_string();
                    let mut args = Args::new();
                    args.set("reason", reason.as_str());
                    Some(cx.t_args(Msg::PICKER_DEMO_FAILED, &args).to_string())
                }
                None => None,
            }
        };
        if let Some(readout) = readout {
            children.push(from_decl(
                Text::new(Id::str("demo/readout"), readout, caption.size, ink)
                    .max_width(width - 48.0)
                    .resolve(),
            ));
        }
        // The instruction is only true where a dialog could not be opened; say so instead of
        // letting the reader wonder where the window went.
        if self.embedded.borrow().is_some() {
            children.push(from_decl(
                Text::new(
                    Id::str("demo/no-surface"),
                    cx.t(Msg::PICKER_DEMO_NO_SURFACE).to_string(),
                    caption.size,
                    muted,
                )
                .max_width(width - 48.0)
                .resolve(),
            ));
        }
        // A quiet strip keeps the page from collapsing to the top and marks the base surface.
        children.push(from_decl(
            Decl::container(Id::str("demo/fill"), NodeKind::Box).size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::Yes,
            }),
        ));

        Box::new(
            Container::from_decl(
                Decl::container(Id::str("demo/page"), NodeKind::Column)
                    .size(SizeClass::fixed(width, height))
                    .background(surface, Corners::ZERO)
                    .padding(Insets::all(24.0))
                    .gap(12.0),
            )
            .children(children),
        )
    }
}

impl App for Demo {
    /// The page and its in-window dialog draw with the first pack, so their menus do too.
    fn menu_style(&self) -> Option<ui_widgets::MenuStyle> {
        Some((self.menu_styles[0])(self.appearance()))
    }

    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        let width = cx.width() as f32;
        let height = cx.height() as f32;
        let mut finished = None;
        // The modal is built first: a field's Enter can answer during the build, and this
        // frame is the one that must show the result instead of the closed modal.
        let modal = {
            let mut slot = self.embedded.borrow_mut();
            slot.as_mut().map(|state| {
                let scrim = (self.packs[0])(self.appearance())
                    .colour_or(CommonColor::Scrim, Color::from_rgba8(0, 0, 0, 120));
                let inner = state.view(
                    cx,
                    (width - 2.0 * FALLBACK_INSET).max(360.0),
                    (height - 2.0 * FALLBACK_INSET).max(240.0),
                );
                finished = state.take_outcome();
                // A global modal is placed on the window rect and centres its content with
                // growers, so the scrim is this node's own fill and the inset is these
                // spacers — a fixed-size child would sit at the window's top-left instead.
                Container::from_decl(
                    Decl::container(Id::str("demo/modal"), NodeKind::Column)
                        .size(SizeClass::hug(0.0, 0.0))
                        .modal(ModalTarget::Global)
                        .cross_align(0.0)
                        .cross_pos(0.5)
                        .sense(Sense::CLICK)
                        .cursor(CursorShape::Arrow)
                        .background(scrim, Corners::ZERO),
                )
                .child(from_decl(grow_spacer(Id::str("demo/modal-pad-top"))))
                .child(inner)
                .child(from_decl(grow_spacer(Id::str("demo/modal-pad-bottom"))))
            })
        };
        let modal = if let Some(outcome) = finished {
            self.shared.borrow_mut().outcome = Some(outcome_result(outcome));
            *self.embedded.borrow_mut() = None;
            None
        } else {
            modal
        };
        let page = {
            let mut widgets = Cx::new(cx, self.env.clone());
            self.page(&mut widgets, width, height)
        };
        let mut stack = Container::new(Id::str("demo/stack"), NodeKind::Stack)
            .size(SizeClass::fixed(width, height))
            .child(page);
        if let Some(modal) = modal {
            stack = stack.child(Box::new(modal));
        }
        Box::new(stack)
    }

    fn changed(&mut self, id: Id, change: ui_core::Change) -> bool {
        if matches!(change, ui_core::Change::Activated)
            && let Some(index) = (0..3).find(|index| pack_button(*index) == id)
        {
            self.shared.borrow_mut().pack = Some(index);
            return true;
        }
        if matches!(change, ui_core::Change::Activated)
            && (appearance_button(false) == id || appearance_button(true) == id)
        {
            self.shared.borrow_mut().dark = appearance_button(true) == id;
            return true;
        }
        let (outcome, dirty) = {
            let mut slot = self.embedded.borrow_mut();
            match slot.as_mut() {
                Some(state) => {
                    let outcome = state.changed(id, change);
                    let dirty = state.take_dirty();
                    (outcome, dirty)
                }
                None => (None, false),
            }
        };
        if let Some(outcome) = outcome {
            self.shared.borrow_mut().outcome = Some(outcome_result(outcome));
            *self.embedded.borrow_mut() = None;
            return true;
        }
        dirty
    }

    fn shortcut(&mut self, id: ui_core::ShortcutId, held: ui_core::Modifiers) -> bool {
        let (outcome, dirty) = {
            let mut slot = self.embedded.borrow_mut();
            match slot.as_mut() {
                Some(state) => {
                    let outcome = state.shortcut(id, held);
                    let dirty = state.take_dirty();
                    (outcome, dirty)
                }
                None => (None, false),
            }
        };
        if let Some(outcome) = outcome {
            self.shared.borrow_mut().outcome = Some(outcome_result(outcome));
            *self.embedded.borrow_mut() = None;
            return true;
        }
        dirty
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("picker_demo: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    install_logging();
    let backend = ui_host::connect()?;
    let mut host = Host::new(Box::new(backend));
    let listings = host
        .listings()
        .ok_or("this host has no directory listing service")?;
    let searches = host
        .searches()
        .ok_or("this host has no recursive search service")?;
    let watches = host
        .watches()
        .ok_or("this host has no directory watch service")?;
    let places = host.places().to_vec();
    let devices = host.devices().to_vec();
    let volumes = host.volumes();
    let thumbnails = host.thumbnails();
    let file_icons = host.file_icons();
    let file_operations = host.file_operations();
    // The host's handle follows whichever surface it opened last, so one obtained now is
    // still the right owner once the dialog's own window exists.
    let shell_menu = host.shell_menu();
    // The platform keeps the preferred-UI-language list; the environment is only how a POSIX
    // session states it, and a Windows launch from the shell states nothing.
    let env = language_env(&host.languages());
    use ui_platform::ThemeWatcher as _;
    // The page offers the switch, but the system's own preference is what the first frame
    // opens in; `PICKER_DEMO_DARK=0|1` seeds it for a screenshot without a click.
    let dark = match std::env::var("PICKER_DEMO_DARK").ok().as_deref() {
        Some("0") => false,
        Some("1") => true,
        _ => ui_host::ThemeWatch::new().preference().is_dark(false),
    };
    let packs: [PackOf; 3] = [
        ui_widgets_md3::pack_of,
        ui_widgets_fluent::pack_of,
        ui_widgets_glass::pack_of,
    ];
    let menu_styles: [MenuStyleOf; 3] = [
        ui_widgets_md3::menu_style_of,
        ui_widgets_fluent::menu_style_of,
        ui_widgets_glass::menu_style_of,
    ];
    let start = start_directory(&places);
    let shared: SharedHandle = Rc::new(RefCell::new(Shared {
        dark,
        ..Shared::default()
    }));
    // `PICKER_DEMO_OPEN=0|1|2` opens a pack without a click, so a scripted run can screenshot
    // the dialog.
    if let Ok(value) = std::env::var("PICKER_DEMO_OPEN") {
        match value.parse::<usize>() {
            Ok(index) if index < 3 => shared.borrow_mut().pack = Some(index),
            _ => eprintln!("picker_demo: PICKER_DEMO_OPEN must be 0, 1 or 2, not {value:?}"),
        }
    }
    let embedded: Embedded = Rc::new(RefCell::new(None));

    let demo = Demo {
        shared: Rc::clone(&shared),
        embedded: Rc::clone(&embedded),
        packs,
        menu_styles,
        env: Arc::clone(&env),
    };
    let parent = host.open(
        SurfaceSpec::window(APP_ID, "crabsnap-ui   picker demo", PARENT_SIZE),
        demo,
        OpenOptions {
            shortcuts: Picker::shortcuts(),
            ..surface_options()
        },
    )?;

    let title = dialog_title(&env);
    let mode = demo_mode();
    let request = demo_request(mode, &start, &title);
    let modal = std::env::var("PICKER_DEMO_MODELESS").as_deref() != Ok("1");
    // The in-window fallback is what a host without dialog surfaces sees; asking for it
    // directly is the only way to exercise that path where the host can open a dialog.
    let inline = std::env::var("PICKER_DEMO_INLINE").as_deref() == Ok("1");
    host.on_turn(move |ctx, _loop| {
        let Some(index) = shared.borrow_mut().pack.take() else {
            return;
        };
        // A dialog is its own surface: it reads the appearance from the env it is given,
        // not from the window that opened it, so this turn states the current one.
        let appearance = if shared.borrow().dark {
            Appearance::Dark
        } else {
            Appearance::Light
        };
        let scoped = Env::with_scope(Some(Arc::clone(&env)), StyleScope::new(appearance), ());
        let options = PickerOptions::new(
            listings.clone(),
            start.clone(),
            packs[index],
            menu_styles[index],
            scoped,
        )
            .places(places.clone())
            .devices(devices.clone())
            .volumes(volumes.clone())
            .thumbnails(thumbnails.clone())
            .file_icons(file_icons.clone())
            .searches(searches.clone())
            .watches(watches.clone())
            // One request for both paths: the drawn fallback must show what the native
            // dialog would have been asked for, not a second configuration.
            .for_request(&request);
        // Dragging a row writes through the host's own service; a host without one leaves
        // the rows with no payload, which the dialog shows by refusing the drag.
        let options = match file_operations.clone() {
            Some(operations) => options.file_operations(operations),
            None => options,
        };
        let options = match shell_menu.clone() {
            Some(menu) => options.shell_menu(menu),
            None => options,
        };
        let spec = SurfaceSpec::dialog(APP_ID, title.clone(), DIALOG_SIZE)
            .with_modal(modal)
            .with_min_size(MIN_SIZE);
        // PICKER_DEMO_NATIVE=1 goes through the portable request instead of constructing
        // the picker directly: the native dialog answers when the platform has one, and the
        // same request falls back to this same drawn picker when it does not.
        if std::env::var("PICKER_DEMO_NATIVE").as_deref() == Ok("1") {
            let slot = Rc::clone(&shared);
            let opened = Chooser::new(request.clone(), Some(options)).open(
                ctx,
                parent,
                spec,
                OpenOptions {
                    shortcuts: Vec::new(),
                    ..surface_options()
                },
                move |answer| {
                    slot.borrow_mut().outcome = Some(answer.result);
                },
            );
            if let Err(error) = opened {
                shared.borrow_mut().outcome = Some(Err(error));
            }
            return;
        }
        // The fallback needs a second state, so the options are shared rather than moved.
        let fallback = options.clone();
        let answer = Rc::clone(&shared);
        let picker = Picker::new(options, move |outcome| {
            answer.borrow_mut().outcome = Some(match outcome {
                PickerOutcome::Selected(selection) => Ok(selection),
                PickerOutcome::Cancelled => Err(ChooserError::Cancelled),
            });
        });
        let refusal = if inline {
            Some("in-window mode was requested".to_string())
        } else {
            ctx.open_dialog(
                parent,
                spec,
                Box::new(picker),
                OpenOptions {
                    shortcuts: Picker::shortcuts(),
                    ..surface_options()
                },
            )
            .err()
        };
        if let Some(reason) = refusal {
            eprintln!("picker_demo: drawing the picker in this window ({reason})");
            *embedded.borrow_mut() = Some(PickerState::new(fallback));
        }
    });

    // Without a bound the demo runs until its windows close, which is what an interactive
    // look needs: a scripted run is the one that sets `PICKER_DEMO_SECONDS`.
    let deadline = std::env::var("PICKER_DEMO_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .map(|seconds| Instant::now() + Duration::from_secs_f32(seconds));
    let started = Instant::now();
    let stats = host.run_until(deadline)?;
    println!(
        "picker_demo: {} turn(s), {} frame(s) over {:.1} s",
        stats.turns,
        stats.frames,
        started.elapsed().as_secs_f32()
    );
    Ok(())
}

/// The faces the picker draws with: text, and the Material Symbols the rows' glyphs name.
fn surface_options() -> OpenOptions {
    const STACK: &[&str] = &[
        "Inter",
        "Noto Sans SC",
        "Noto Sans CJK SC",
        "MiSans VF",
        "HarmonyOS Sans SC",
    ];
    OpenOptions {
        fonts: vec![
            ui_platform::host::FontFace {
                name: "Inter".to_string(),
                data: include_bytes!("../../assets/fonts/InterVariable.ttf").as_slice().into(),
            },
            ui_platform::host::FontFace {
                name: "Material Symbols Outlined".to_string(),
                data: include_bytes!("../../assets/fonts/MaterialSymbolsOutlined.ttf").as_slice().into(),
            },
        ],
        stack: STACK.iter().map(|family| (*family).to_string()).collect(),
        ..OpenOptions::default()
    }
}

fn language_env(system: &[String]) -> Arc<Env> {
    match Language::builtin(LanguageChoice::Auto, system) {
        Ok(language) => Env::with(Some(Env::root()), language),
        Err(error) => {
            eprintln!("picker_demo: no built-in catalog ({error}); the dialog stays English");
            Env::root()
        }
    }
}

/// The dialog's caption, resolved from the catalogue rather than written twice.
fn dialog_title(env: &Env) -> String {
    env.find::<Arc<Language>>()
        .map(|language| language.catalog().text(Msg::PICKER_OPEN).to_string())
        .unwrap_or_else(|| "Open".to_string())
}

/// A zero-basis grower: a modal's content is centred on the window by two of these.
fn grow_spacer(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::Yes,
    })
}

fn start_directory(places: &[Place]) -> PathBuf {
    if let Some(start) = std::env::var_os("PICKER_DEMO_START") {
        return PathBuf::from(start);
    }
    places
        .first()
        .map(|place| place.path.clone())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The variant `PICKER_DEMO_MODE` names, `OpenFile` when unset or misspelled.
fn demo_mode() -> ChooserMode {
    match std::env::var("PICKER_DEMO_MODE").as_deref() {
        Ok("open-files") => ChooserMode::OpenFiles,
        Ok("folder") => ChooserMode::SelectFolder,
        Ok("folders") => ChooserMode::SelectFolders,
        Ok("save") => ChooserMode::SaveFile,
        Ok("save-files") => ChooserMode::SaveFiles,
        Ok("open") | Ok("") | Err(_) => ChooserMode::OpenFile,
        Ok(other) => {
            eprintln!(
                "picker_demo: PICKER_DEMO_MODE must be open, open-files, folder, folders, save or save-files, not {other:?}"
            );
            ChooserMode::OpenFile
        }
    }
}

/// The one request both implementations run, so a variant is checked on the same terms.
fn demo_request(mode: ChooserMode, start: &Path, title: &str) -> ChooserRequest {
    let mut request = match mode {
        ChooserMode::OpenFile => ChooserRequest::open_file(start),
        ChooserMode::OpenFiles => ChooserRequest::open_files(start),
        ChooserMode::SelectFolder => ChooserRequest::select_folder(start),
        ChooserMode::SelectFolders => ChooserRequest::select_folders(start),
        ChooserMode::SaveFile => ChooserRequest::save_file(start.join("untitled.png")),
        ChooserMode::SaveFiles => ChooserRequest::save_files(
            start,
            vec!["untitled.png".to_string(), "notes.txt".to_string()],
        ),
    };
    request = request.title(title);
    if !mode.takes_directories() {
        request = request
            .filter(FileTypeFilter::all_files())
            // The second entry exercises the type menu and the MIME half of a filter.
            .filter(
                FileTypeFilter::new("Images", vec!["*.png".to_string(), "*.jpg".to_string()])
                    .mime_types(vec!["image/*".to_string()]),
            );
    }
    if std::env::var("PICKER_DEMO_EXTRAS").as_deref() == Ok("1") {
        request = request
            .place(ChooserPlace::new("Demo", start))
            .choice(ChooserChoice::select(
                "encoding",
                "Encoding",
                vec![
                    ChoiceOption::new("utf8", "Unicode (UTF-8)"),
                    ChoiceOption::new("latin1", "Western"),
                ],
                "utf8",
            ))
            .choice(ChooserChoice::boolean("keep", "Keep original", true));
    }
    request
}
