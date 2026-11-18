//! The loop's control flow against a fake backend: no compositor, no window, no sleeping.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ui_core::clipboard::ClipboardContent;
use ui_core::draw::ImageEntry;
use ui_core::shortcut::{KeyChord, ShortcutId};
use ui_core::{Decl, Id, NodeKind};
use ui_platform::clipboard::{ClipboardCopy, ClipboardKind};
use ui_platform::host::{
    Backend, Effect, Engine, Host, OpenOptions, PresentationOptions, SurfaceHandle, SurfaceStep,
};
use ui_platform::hotkeys::{GlobalHotkeyError, GlobalHotkeys};
use ui_platform::ipc::{Channel, Reply};
use ui_platform::thumbnails::{
    ThumbnailPicture, ThumbnailRequest, ThumbnailState, Thumbnailer, Thumbnails,
};
use ui_platform::{
    FileIcon, FileIconProvider, FileIconRequest, FileIconState, FileIcons, FileKind,
};
use ui_platform::window::SurfaceSpec;
use ui_platform::{App, Element};

struct Blank;

impl App for Blank {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        ui_platform::from_decl(Decl::container(Id::str("root"), NodeKind::Box))
    }
}

/// One scripted step: draw, publish a copy, close, fail, or run a modal pump.
#[derive(Clone)]
enum Act {
    Idle,
    Draw,
    Copy(&'static str, u32),
    Close,
    Fail(&'static str),
    /// Enter a modal gesture: pump the rest of the loop, recording what it reached.
    Modal(Rc<Cell<usize>>, Rc<RefCell<Option<(bool, usize)>>>),
}

struct Scripted {
    plan: VecDeque<Act>,
    steps: Rc<Cell<usize>>,
    requested: Rc<Cell<usize>>,
    deadline: Option<Instant>,
    closed: bool,
    copies: Vec<ClipboardCopy>,
    images: Rc<RefCell<Vec<u64>>>,
}

impl SurfaceHandle for Scripted {
    fn add_image(&mut self, image: &ImageEntry) -> Result<(), String> {
        self.images.borrow_mut().push(image.id);
        Ok(())
    }

    fn step(&mut self, _engine: &mut Engine) -> Result<SurfaceStep, String> {
        self.steps.set(self.steps.get() + 1);
        assert!(
            self.steps.get() < 100,
            "the loop kept stepping a surface that had nothing left to do"
        );
        match self.plan.pop_front().unwrap_or(Act::Idle) {
            Act::Idle => {}
            Act::Draw => return Ok(SurfaceStep { drew: true }),
            Act::Copy(text, serial) => self.copies.push(ClipboardCopy {
                content: ClipboardContent::Text(text.into()),
                kind: ClipboardKind::Selection,
                serial,
            }),
            Act::Close => self.closed = true,
            Act::Fail(reason) => return Err(reason.to_string()),
            Act::Modal(other, seen) => {
                let pumped = ui_platform::modal::pump_others();
                *seen.borrow_mut() = Some((pumped, other.get()));
            }
        }
        Ok(SurfaceStep::default())
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        self.deadline
    }

    fn frame_requested(&mut self) {
        self.requested.set(self.requested.get() + 1);
    }

    fn is_closed(&self) -> bool {
        self.closed
    }

    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        std::mem::take(&mut self.copies)
    }
}

/// What the backend saw, shared so a test can read it after the loop's surfaces are gone.
#[derive(Default)]
struct Log {
    waits: Vec<Option<Duration>>,
    published: Vec<ClipboardCopy>,
    served: usize,
    flushes: usize,
    bound: Vec<(KeyChord, ShortcutId)>,
    unbound: Vec<ShortcutId>,
}

/// Hotkeys a test has "bound": presses are queued, and `poll` hands them over once.
struct ScriptedHotkeys {
    pressed: VecDeque<ShortcutId>,
    log: Rc<RefCell<Log>>,
}

impl GlobalHotkeys for ScriptedHotkeys {
    fn register(&mut self, chord: KeyChord, id: ShortcutId) -> Result<(), GlobalHotkeyError> {
        self.log.borrow_mut().bound.push((chord, id));
        Ok(())
    }

    fn unregister(&mut self, id: ShortcutId) {
        self.log.borrow_mut().unbound.push(id);
    }

    fn poll(&mut self) -> Vec<ShortcutId> {
        self.pressed.drain(..).collect()
    }
}

struct FakeBackend {
    prepared: VecDeque<Box<dyn SurfaceHandle>>,
    log: Rc<RefCell<Log>>,
    /// Stop the loop once this many waits have happened; the fake never blocks.
    stop_after: usize,
    hotkeys: Option<ScriptedHotkeys>,
    /// The platform's thumbnail source, when the test gives it one.
    thumbnailer: Option<Box<dyn Thumbnailer>>,
    /// The platform's file-icon source, when the test gives it one.
    file_icon_provider: Option<Box<dyn FileIconProvider>>,
}

impl Backend for FakeBackend {
    fn open(
        &mut self,
        _spec: SurfaceSpec,
        _options: PresentationOptions,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        match self.prepared.pop_front() {
            Some(surface) => Ok(surface),
            None => Err("the test prepared no surface".into()),
        }
    }

    fn set_clipboard_copy(&mut self, copy: ClipboardCopy) -> bool {
        self.log.borrow_mut().published.push(copy);
        true
    }

    fn serve_clipboard(&mut self) -> Result<(), String> {
        self.log.borrow_mut().served += 1;
        Ok(())
    }

    fn wait(&mut self, timeout: Option<Duration>) -> Result<(), String> {
        let mut log = self.log.borrow_mut();
        assert!(
            log.waits.len() < 1000,
            "the loop kept waiting with nothing that could stop it"
        );
        log.waits.push(timeout);
        Ok(())
    }

    fn flush(&mut self) {
        self.log.borrow_mut().flushes += 1;
    }

    fn should_quit(&self) -> bool {
        self.log.borrow().waits.len() > self.stop_after
    }

    fn listen(&mut self, _name: &str) -> Result<Box<dyn Channel>, String> {
        Ok(Box::new(Once { sent: false }))
    }

    fn thumbnails(&mut self) -> Option<Box<dyn Thumbnailer>> {
        self.thumbnailer.take()
    }

    fn file_icons(&mut self) -> Option<Box<dyn FileIconProvider>> {
        self.file_icon_provider.take()
    }

    fn hotkeys(&mut self) -> Option<&mut (dyn GlobalHotkeys + 'static)> {
        self.hotkeys
            .as_mut()
            .map(|hotkeys| hotkeys as &mut (dyn GlobalHotkeys + 'static))
    }
}

fn scripted(plan: &[Act]) -> (Scripted, Rc<Cell<usize>>) {
    let steps = Rc::new(Cell::new(0));
    (
        Scripted {
            plan: plan.iter().cloned().collect(),
            steps: Rc::clone(&steps),
            requested: Rc::new(Cell::new(0)),
            deadline: None,
            closed: false,
            copies: Vec::new(),
            images: Rc::new(RefCell::new(Vec::new())),
        },
        steps,
    )
}

/// A surface whose steps and textures a test can read after the loop is over.
fn watched(plan: &[Act]) -> (Scripted, Rc<Cell<usize>>, Rc<RefCell<Vec<u64>>>) {
    let (surface, steps) = scripted(plan);
    let images = Rc::clone(&surface.images);
    (surface, steps, images)
}

/// Says one thing once, so a handler runs without a socket.
struct Once {
    sent: bool,
}

impl Channel for Once {
    fn poll(&mut self, on_message: &mut dyn FnMut(&mut dyn Reply, &[u8])) {
        if !self.sent {
            self.sent = true;
            on_message(&mut NullReply, b"go");
        }
    }
}

struct NullReply;

impl Reply for NullReply {
    fn send(&mut self, _bytes: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

/// A host over a fake backend, plus what the backend saw.
fn host(surfaces: Vec<Scripted>, stop_after: usize) -> (Host, Rc<RefCell<Log>>) {
    host_with_hotkeys(surfaces, stop_after, None)
}

/// A host whose platform reports a thumbnail source.
fn host_with_thumbnailer(
    surfaces: Vec<Scripted>,
    thumbnailer: Box<dyn Thumbnailer>,
) -> (Host, Rc<RefCell<Log>>) {
    let log = Rc::new(RefCell::new(Log::default()));
    let backend = fake_backend(
        surfaces,
        Rc::clone(&log),
        usize::MAX,
        None,
        Some(thumbnailer),
    );
    (Host::new(Box::new(backend)), log)
}

/// A host whose platform reports an icon source.
fn host_with_icons(
    surfaces: Vec<Scripted>,
    provider: Box<dyn FileIconProvider>,
) -> (Host, Rc<RefCell<Log>>) {
    let log = Rc::new(RefCell::new(Log::default()));
    let mut backend = fake_backend(surfaces, Rc::clone(&log), usize::MAX, None, None);
    backend.file_icon_provider = Some(provider);
    (Host::new(Box::new(backend)), log)
}

fn fake_backend(
    surfaces: Vec<Scripted>,
    log: Rc<RefCell<Log>>,
    stop_after: usize,
    hotkeys: Option<ScriptedHotkeys>,
    thumbnailer: Option<Box<dyn Thumbnailer>>,
) -> FakeBackend {
    FakeBackend {
        prepared: surfaces
            .into_iter()
            .map(|surface| Box::new(surface) as Box<dyn SurfaceHandle>)
            .collect(),
        log,
        stop_after,
        hotkeys,
        thumbnailer,
        file_icon_provider: None,
    }
}

/// `pressed: None` is a platform without hotkeys at all.
fn host_with_hotkeys(
    surfaces: Vec<Scripted>,
    stop_after: usize,
    pressed: Option<Vec<ShortcutId>>,
) -> (Host, Rc<RefCell<Log>>) {
    let log = Rc::new(RefCell::new(Log::default()));
    let hotkeys = pressed.map(|pressed| ScriptedHotkeys {
        pressed: pressed.into(),
        log: Rc::clone(&log),
    });
    let backend = fake_backend(surfaces, Rc::clone(&log), stop_after, hotkeys, None);
    (Host::new(Box::new(backend)), log)
}

fn spec(name: &str) -> SurfaceSpec {
    SurfaceSpec::window("dev.crabsnap.test", name, (16, 16))
}

#[test]
fn every_surface_steps_once_per_wait_until_it_closes() {
    let (first, first_steps) = scripted(&[Act::Draw, Act::Idle, Act::Close]);
    let (second, second_steps) = scripted(&[Act::Idle, Act::Draw, Act::Close]);
    let (mut host, log) = host(vec![first, second], usize::MAX);
    host.open(spec("first"), Blank, OpenOptions::default())
        .expect("first surface");
    host.open(spec("second"), Blank, OpenOptions::default())
        .expect("second surface");

    host.run_until(None)
        .expect("the loop ends when both surfaces close");

    assert_eq!(first_steps.get(), 3, "one step per turn, no repeats");
    assert_eq!(second_steps.get(), 3);
    assert_eq!(host.surface_count(), 0);
    assert_eq!(log.borrow().waits.len(), 2, "the last turn closes the loop");
    assert_eq!(log.borrow().flushes, 3, "queued work leaves every turn");
    assert_eq!(log.borrow().served, 3, "the clipboard is served every turn");
}

/// A surface that cannot draw is dropped on its own, and the rest of the loop keeps running:
/// one lost swap chain must not take the tray, the hotkeys and every other window with it.
#[test]
fn a_failing_surface_is_retired_while_the_others_draw_on() {
    let (failing, _) = scripted(&[Act::Fail("the device was lost")]);
    let (survivor, survivor_steps) = scripted(&[Act::Draw, Act::Idle, Act::Close]);
    let (mut host, log) = host(vec![failing, survivor], usize::MAX);
    host.open(spec("failing"), Blank, OpenOptions::default())
        .expect("the surface that fails");
    host.open(spec("survivor"), Blank, OpenOptions::default())
        .expect("the surface that keeps drawing");

    host.run_until(None)
        .expect("a failed surface does not end the loop");

    assert_eq!(survivor_steps.get(), 3, "the survivor kept its own turns");
    assert_eq!(host.surface_count(), 0, "the failed surface was dropped");
    assert_eq!(
        log.borrow().served,
        3,
        "the loop turned on past the failure"
    );
}

/// A modal gesture borrows the thread's message loop, so the host's own turn is suspended
/// for its duration. The platform's timer pumps the rest of the process through it; without
/// that every other window holds its last frame until the gesture ends.
#[test]
fn a_modal_gesture_pumps_the_other_surfaces() {
    let (other, other_steps) = scripted(&[Act::Idle, Act::Idle, Act::Idle, Act::Close]);
    let seen = Rc::new(RefCell::new(None));
    let (gesture, _) = scripted(&[
        Act::Modal(Rc::clone(&other_steps), Rc::clone(&seen)),
        Act::Close,
    ]);
    let (mut host, _log) = host(vec![gesture, other], usize::MAX);
    host.open(spec("gesture"), Blank, OpenOptions::default())
        .expect("the surface that starts the gesture");
    host.open(spec("other"), Blank, OpenOptions::default())
        .expect("the surface that must keep drawing");

    host.run_until(None)
        .expect("the loop ends when both surfaces close");

    let (pumped, stepped) = seen.borrow().expect("the gesture's pump ran");
    assert!(pumped, "a surface's step must reach the rest of the loop");
    assert_eq!(
        stepped, 1,
        "the pump stepped the other surface while the gesture's step was on the stack"
    );
}

/// A surface opened during a turn draws before the loop blocks again.
///
/// The turn that opens it has already stepped the surfaces, and the surface's own setup
/// consumed the events that would have woken the loop: without another turn its first frame
/// waits for unrelated input — a pin canvas on screen but not drawn.
#[test]
fn a_surface_opened_during_a_turn_draws_before_the_loop_waits() {
    let (first, _) = scripted(&[Act::Idle, Act::Close]);
    let (second, second_steps) = scripted(&[Act::Draw, Act::Close]);
    let (mut host, log) = host(vec![first, second], usize::MAX);
    host.open(spec("first"), Blank, OpenOptions::default())
        .expect("the surface the loop starts with");

    let opened = Rc::new(Cell::new(false));
    let waits_before_first_frame = Rc::new(Cell::new(None::<usize>));
    {
        let opened = Rc::clone(&opened);
        let waits = Rc::clone(&waits_before_first_frame);
        let steps = Rc::clone(&second_steps);
        let log = Rc::clone(&log);
        host.on_turn(move |cx, _| {
            if steps.get() > 0 && waits.get().is_none() {
                waits.set(Some(log.borrow().waits.len()));
            }
            if opened.replace(true) {
                return;
            }
            cx.open(spec("second"), Box::new(Blank), OpenOptions::default())
                .expect("the turn opens the second surface");
        });
    }

    host.run_until(None)
        .expect("the loop ends when both surfaces close");

    assert_eq!(
        waits_before_first_frame.get(),
        Some(0),
        "the new surface drew in a turn that did not follow a wait"
    );
}

/// A texture added after another surface closed still reaches the surface it was opened on,
/// and asks that surface to build.
///
/// An id that indexes the live list would have shifted when the first surface went away, and
/// the second one — a pin canvas holding textures for as long as it lives — would be lost.
/// And a surface that took the texture without building keeps its old frame: the app already
/// put the node that draws it into its own state, so nothing else would wake it — a pin added
/// to a canvas that is already up would never appear.
#[test]
fn a_texture_reaches_the_surface_that_survived_and_makes_it_build() {
    let (first, _, first_images) = watched(&[Act::Close]);
    let (second, _, second_images) = watched(&[]);
    let second_builds = Rc::clone(&second.requested);
    let (mut host, _log) =
        host_with_hotkeys(vec![first, second], 0, Some(vec![ShortcutId("test/open")]));
    let held = Rc::new(RefCell::new(Vec::new()));
    {
        let held = Rc::clone(&held);
        host.on_hotkey(move |_, cx| {
            for name in ["first", "second"] {
                let id = cx
                    .open(spec(name), Box::new(Blank), OpenOptions::default())
                    .expect("the hotkey opens its surfaces");
                held.borrow_mut().push(id);
            }
            Effect::None
        });
    }
    let added = Rc::new(Cell::new(0));
    let builds = Rc::clone(&second_builds);
    host.on_turn(move |cx, _| {
        // One turn while both surfaces live, one after the first closed: enough to tell
        // a shifted id from a held one, without asking for a frame every turn.
        if added.replace(added.get() + 1) >= 2 {
            return;
        }
        let held = held.borrow();
        let Some(surviving) = held.get(1) else {
            return;
        };
        let image = ImageEntry {
            id: 7,
            width: 1,
            height: 1,
            source: ui_core::draw::ImageSource::Pixels(Arc::new(vec![0u8; 4])),
            colour: ui_core::colour::ContentColour::SDR,
            sampling: ui_core::draw::Sampling::Smooth,
        };
        let before = builds.get();
        cx.add_image(*surviving, image)
            .expect("the held id still names a live surface");
        cx.request_frame(*surviving)
            .expect("the held id still names a live surface");
        assert_eq!(
            builds.get(),
            before + 2,
            "a texture and a bare request each reach the surface"
        );
    });

    host.run().expect("the loop runs");

    assert_eq!(
        *first_images.borrow(),
        Vec::<u64>::new(),
        "the closed surface was handed nothing"
    );
    assert_eq!(
        *second_images.borrow(),
        vec![7, 7],
        "both turns reached the surface the ids were handed out for"
    );
    assert_eq!(
        second_builds.get(),
        5,
        "every turn's texture and request asked the surface to build, and the departure of the \
         other surface asked it once more: its opener reads what the closed surface answered"
    );
}

#[test]
fn the_clipboard_gets_each_copy_once_in_order() {
    let (first, _) = scripted(&[Act::Copy("a", 7), Act::Copy("b", 8), Act::Close]);
    let (mut host, log) = host(vec![first], usize::MAX);
    host.open(spec("first"), Blank, OpenOptions::default())
        .expect("surface");

    host.run_until(None).expect("the loop runs");

    let log = log.borrow();
    let texts: Vec<&str> = log
        .published
        .iter()
        .map(|copy| match &copy.content {
            ClipboardContent::Text(text) => text.as_str(),
            ClipboardContent::Bytes { .. } => "<bytes>",
            ClipboardContent::Files { .. } => "<files>",
        })
        .collect();
    assert_eq!(texts, ["a", "b"], "copies keep the order the build asked");
    assert_eq!(log.published[1].serial, 8, "each copy keeps its serial");
}

#[test]
fn the_nearest_deadline_bounds_the_wait() {
    let (mut first, _) = scripted(&[]);
    first.deadline = Some(Instant::now() + Duration::from_secs(10));
    let (mut second, _) = scripted(&[]);
    second.deadline = Some(Instant::now() + Duration::from_secs(20));
    let (mut host, log) = host(vec![first, second], 0);
    host.open(spec("first"), Blank, OpenOptions::default())
        .expect("first surface");
    host.open(spec("second"), Blank, OpenOptions::default())
        .expect("second surface");

    host.run()
        .expect("the loop runs one turn, then the fake quits");

    let waits = &log.borrow().waits;
    let first_wait = waits[0].expect("a deadline is not an unbounded wait");
    assert!(
        first_wait <= Duration::from_secs(10) && first_wait > Duration::from_secs(9),
        "the nearest deadline wins: {first_wait:?}"
    );
}

#[test]
fn a_run_deadline_clamps_a_later_surface_deadline() {
    let (mut surface, _) = scripted(&[]);
    surface.deadline = Some(Instant::now() + Duration::from_secs(60));
    let (mut host, log) = host(vec![surface], 0);
    host.open(spec("first"), Blank, OpenOptions::default())
        .expect("surface");

    host.run_until(Some(Instant::now() + Duration::from_secs(5)))
        .expect("the loop runs");

    let waits = &log.borrow().waits;
    let wait = waits[0].expect("the run's own deadline bounds the wait");
    assert!(
        wait <= Duration::from_secs(5),
        "the run deadline wins over a later surface: {wait:?}"
    );
}

#[test]
fn a_pressed_hotkey_reaches_the_handler_once_in_order() {
    let (mut host, _log) = host_with_hotkeys(
        Vec::new(),
        0,
        Some(vec![
            ShortcutId("crabsnap.region"),
            ShortcutId("crabsnap.pin"),
        ]),
    );
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    host.on_hotkey(move |id, cx| {
        sink.borrow_mut().push((id, cx.outputs().len()));
        Effect::None
    });

    host.run().expect("the fake quits after one wait");

    assert_eq!(
        seen.borrow().as_slice(),
        [
            (ShortcutId("crabsnap.region"), 0),
            (ShortcutId("crabsnap.pin"), 0)
        ],
        "each press is answered once, in the order the backend reported it"
    );
}

#[test]
fn binding_reaches_the_backend_and_an_absent_backend_says_unsupported() {
    let (mut host, log) = host_with_hotkeys(Vec::new(), 0, Some(Vec::new()));
    let chord = KeyChord::new(ui_core::event::Key::Print, ui_core::event::Modifiers::NONE);
    host.context()
        .bind_hotkey(chord, ShortcutId("crabsnap.region"))
        .expect("the fake backend binds anything");
    host.context().unbind_hotkey(ShortcutId("crabsnap.region"));
    assert_eq!(
        log.borrow().bound.as_slice(),
        [(chord, ShortcutId("crabsnap.region"))],
        "the chord reached the backend as written"
    );
    assert_eq!(
        log.borrow().unbound.as_slice(),
        [ShortcutId("crabsnap.region")],
        "the id is what a platform releases"
    );

    let (mut bare, _log) = host_with_hotkeys(Vec::new(), 0, None);
    assert_eq!(
        bare.context()
            .bind_hotkey(chord, ShortcutId("crabsnap.region")),
        Err(GlobalHotkeyError::Unsupported),
        "a platform without hotkeys refuses, rather than pretending"
    );
}

#[test]
fn a_hotkey_can_end_the_loop() {
    // The fake stops itself after a few turns: a broken quit must fail the assertion below,
    // not spin the test binary forever.
    let (mut host, log) = host_with_hotkeys(Vec::new(), 3, Some(vec![ShortcutId("quit")]));
    host.on_hotkey(|_, _| Effect::Quit);

    host.run().expect("the hotkey ends the loop");

    assert!(
        log.borrow().waits.is_empty(),
        "a quit in the turn is not followed by a sleep"
    );
}

#[test]
fn a_channel_handler_opens_a_surface_and_adds_its_texture() {
    let (first, first_steps) = scripted(&[Act::Close]);
    let (second, second_steps, second_images) = watched(&[Act::Close]);
    let (mut host, _log) = host(vec![first, second], usize::MAX);
    host.open(spec("first"), Blank, OpenOptions::default())
        .expect("the command's surface");

    host.listen("test", move |_reply, _bytes, cx| {
        let id = cx
            .open(spec("second"), Box::new(Blank), OpenOptions::default())
            .expect("the daemon opens its canvas");
        cx.add_image(
            id,
            ImageEntry {
                id: 7,
                width: 2,
                height: 2,
                source: ui_core::draw::ImageSource::Pixels(Arc::new(vec![0u8; 16])),
                colour: ui_core::colour::ContentColour::SDR,
                sampling: ui_core::draw::Sampling::Smooth,
            },
        )
        .expect("a texture reaches the surface the handler opened");
        Effect::None
    })
    .expect("the fake backend has a channel");

    host.run_until(None)
        .expect("the loop runs until both close");

    assert_eq!(first_steps.get(), 1, "the command's surface closes at once");
    assert_eq!(second_steps.get(), 1, "the opened surface was stepped");
    assert_eq!(*second_images.borrow(), [7], "the later texture arrived");
}

/// A finished job is not an event, so the loop has to ask every surface for a frame.
#[test]
fn a_finished_job_asks_every_surface_for_a_frame() {
    let (surface, _steps) = scripted(&[Act::Idle, Act::Idle, Act::Close]);
    let requested = Rc::clone(&surface.requested);
    let (mut host, _log) = host(vec![surface], usize::MAX);
    let jobs = host.jobs();
    let (sender, receiver) = std::sync::mpsc::channel();
    jobs.submit(move || {
        sender.send(()).expect("the test is listening");
    })
    .expect("submit");
    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("the job ran");
    let deadline = Instant::now() + Duration::from_secs(5);
    while jobs.completed() == 0 {
        assert!(Instant::now() < deadline, "the job never finished");
        std::thread::yield_now();
    }

    host.open(spec("first"), Blank, OpenOptions::default())
        .expect("the surface");
    host.run_until(None).expect("the surface closes");

    assert_eq!(requested.get(), 1, "one completion asks for one frame");
}

/// A thumbnail that arrives between turns is not an event either: its completion asks for the
/// frame that can show it, the same way a job's does.
#[test]
fn a_finished_thumbnail_asks_every_surface_for_a_frame() {
    struct Blocking {
        release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl Thumbnailer for Blocking {
        fn thumbnail(&self, request: &ThumbnailRequest) -> Option<ThumbnailPicture> {
            if let Ok(release) = self.release.lock() {
                let _ = release.recv_timeout(Duration::from_secs(5));
            }
            let side = request.size;
            Some(ThumbnailPicture {
                width: side,
                height: side,
                rgba: vec![0; side as usize * side as usize * 4],
            })
        }
    }

    let (surface, _steps) = scripted(&[Act::Idle, Act::Idle, Act::Close]);
    let requested = Rc::clone(&surface.requested);
    let (release, receiver) = std::sync::mpsc::channel();
    let provider = Box::new(Blocking {
        release: std::sync::Mutex::new(receiver),
    });
    let (mut host, _log) = host_with_thumbnailer(vec![surface], provider);
    let thumbnails: Thumbnails = host.thumbnails().expect("the fake has a thumbnail source");
    let request = ThumbnailRequest::new("/tmp/a.png", 64);
    assert!(matches!(thumbnails.get(&request), ThumbnailState::Loading));
    release.send(()).expect("the provider is listening");
    let deadline = Instant::now() + Duration::from_secs(5);
    while thumbnails.completed() == 0 {
        assert!(Instant::now() < deadline, "the thumbnail never finished");
        std::thread::yield_now();
    }

    host.open(spec("first"), Blank, OpenOptions::default())
        .expect("the surface");
    host.run_until(None).expect("the surface closes");

    assert_eq!(requested.get(), 1, "one completion asks for one frame");
}

/// An icon that arrives between turns asks for the frame that draws it, as a thumbnail does.
#[test]
fn a_finished_icon_asks_every_surface_for_a_frame() {
    struct Blocking {
        release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl FileIconProvider for Blocking {
        fn icon(&self, request: &FileIconRequest) -> Option<FileIcon> {
            if let Ok(release) = self.release.lock() {
                let _ = release.recv_timeout(Duration::from_secs(5));
            }
            let side = request.size;
            Some(FileIcon {
                identity: 1,
                base: ThumbnailPicture {
                    width: side,
                    height: side,
                    rgba: vec![0; side as usize * side as usize * 4],
                },
                badge: None,
            })
        }
    }

    let (surface, _steps) = scripted(&[Act::Idle, Act::Idle, Act::Close]);
    let requested = Rc::clone(&surface.requested);
    let (release, receiver) = std::sync::mpsc::channel();
    let provider = Box::new(Blocking {
        release: std::sync::Mutex::new(receiver),
    });
    let (mut host, _log) = host_with_icons(vec![surface], provider);
    let icons: FileIcons = host.file_icons().expect("the fake has an icon source");
    let request = FileIconRequest::new("/tmp/a.txt", FileKind::Text, false, 32);
    assert!(matches!(icons.get(&request), FileIconState::Loading));
    release.send(()).expect("the provider is listening");
    let deadline = Instant::now() + Duration::from_secs(5);
    while icons.completed() == 0 {
        assert!(Instant::now() < deadline, "the icon never finished");
        std::thread::yield_now();
    }

    host.open(spec("first"), Blank, OpenOptions::default())
        .expect("the surface");
    host.run_until(None).expect("the surface closes");

    assert_eq!(requested.get(), 1, "one completion asks for one frame");
}

// ---- panels on surfaces of their own ----

/// What the engine told the app was dismissed, frame by frame.
type Seen = Rc<RefCell<Vec<Option<ui_core::Id>>>>;

/// A page whose panel asks for a surface of its own.
struct PanelPage {
    seen: Seen,
}

impl App for PanelPage {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        self.seen.borrow_mut().push(cx.dismissed());
        use ui_core::layout::{Anchor, OverlayPlacement, Presentation, SizeClass};
        ui_platform::from_decl(
            Decl::container(Id::str("root"), NodeKind::Stack)
                .size(SizeClass::fixed(200.0, 200.0))
                .child(
                    Decl::container(Id::str("anchor"), NodeKind::Box)
                        .size(SizeClass::fixed(20.0, 20.0)),
                )
                .child(
                    Decl::container(Id::str("panel"), NodeKind::Box)
                        .size(SizeClass::fixed(100.0, 60.0))
                        .background(
                            ui_core::geom::Color::from_rgba8(255, 0, 0, 255),
                            ui_core::geom::Corners::ZERO,
                        )
                        .overlay(
                            OverlayPlacement::below(Anchor::Node(Id::str("anchor")))
                                .presentation(Presentation::Platform),
                        ),
                ),
        )
    }
}

/// What the panel machinery did, readable after the loop is over.
#[derive(Default)]
struct PanelLog {
    /// Owners of every rect the root's own lists carried, across all frames.
    parent_owners: RefCell<Vec<ui_core::NodeId>>,
    /// Panels the platform was asked for, with the request it got.
    opened: RefCell<Vec<(ui_core::NodeId, ui_platform::window::PopupSpec)>>,
    /// Panels whose surface presented a list.
    shown: RefCell<Vec<ui_core::NodeId>>,
    /// The engine's absolute panel rects, from the last frame the root built.
    rects: RefCell<Vec<(ui_core::NodeId, ui_core::geom::Rect)>>,
}

/// A scene target that records which nodes owned the rects it was handed.
#[derive(Default)]
struct OwnerTarget {
    owners: Vec<ui_core::NodeId>,
}

impl ui_platform::present::SceneTarget for OwnerTarget {
    fn size(&self) -> (u32, u32) {
        (400, 400)
    }

    fn present_scene(
        &mut self,
        scene: &ui_core::draw::DrawList,
        _text: &ui_platform::TextPass,
        _damage: &ui_core::Damage,
        _canvases: &[ui_platform::present::CanvasFrame],
    ) -> Result<ui_platform::present::PresentStats, String> {
        self.owners.extend(scene.owners.rects.iter().copied());
        Ok(ui_platform::present::PresentStats {
            draw_calls: 1,
            band: None,
        })
    }
}

/// The group's root: builds a real frame each step, then closes after `budget` steps.
struct PanelRoot {
    log: Rc<PanelLog>,
    budget: Cell<usize>,
    /// Whether this presentation can open a popup at all.
    popups: bool,
    /// Whether each panel's surface closes itself after its first frame, as a compositor
    /// taking the popup back does.
    close_panels: bool,
}

impl SurfaceHandle for PanelRoot {
    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        let mut target = OwnerTarget::default();
        engine.frame_scene(&mut target, None)?;
        self.log.parent_owners.borrow_mut().extend(target.owners);
        *self.log.rects.borrow_mut() = engine
            .platform_panels()
            .iter()
            .map(|panel| (panel.node, panel.rect))
            .collect();
        self.budget.set(self.budget.get() - 1);
        Ok(SurfaceStep { drew: true })
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        None
    }

    fn is_closed(&self) -> bool {
        self.budget.get() == 0
    }

    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        Vec::new()
    }

    fn open_popup(
        &mut self,
        node: ui_core::NodeId,
        spec: ui_platform::window::PopupSpec,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        if !self.popups {
            return Err("this presentation has no popup surfaces".into());
        }
        self.log.opened.borrow_mut().push((node, spec));
        Ok(Box::new(PanelSurface {
            node,
            log: Rc::clone(&self.log),
            closes: self.close_panels,
            done: Cell::new(false),
        }))
    }
}

/// One panel's surface: shows the part of the frame the engine painted for its node.
struct PanelSurface {
    node: ui_core::NodeId,
    log: Rc<PanelLog>,
    /// Close after the first frame, standing in for the compositor taking the popup back.
    closes: bool,
    done: Cell<bool>,
}

impl SurfaceHandle for PanelSurface {
    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        let mut target = OwnerTarget::default();
        engine.present_panel(self.node, &mut target)?;
        self.log.shown.borrow_mut().push(self.node);
        self.done.set(self.closes);
        Ok(SurfaceStep { drew: true })
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        None
    }

    fn is_closed(&self) -> bool {
        self.done.get()
    }

    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        Vec::new()
    }

    /// A panel inside this one hangs off this surface, as it does on a compositor.
    fn open_popup(
        &mut self,
        node: ui_core::NodeId,
        spec: ui_platform::window::PopupSpec,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        self.log.opened.borrow_mut().push((node, spec));
        Ok(Box::new(PanelSurface {
            node,
            log: Rc::clone(&self.log),
            closes: self.closes,
            done: Cell::new(false),
        }))
    }
}

/// Run `PanelPage` for one panel's worth of frames.
fn run_panels(popups: bool) -> (Rc<PanelLog>, Vec<ui_core::NodeId>) {
    let (log, parent, _) = run_panels_with(popups, false);
    (log, parent)
}

/// The same page, with panels that close themselves after one frame when `close_panels`.
fn run_panels_with(
    popups: bool,
    close_panels: bool,
) -> (Rc<PanelLog>, Vec<ui_core::NodeId>, Seen) {
    let log = Rc::new(PanelLog::default());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let root = PanelRoot {
        log: Rc::clone(&log),
        budget: Cell::new(3),
        popups,
        close_panels,
    };
    let backend = FakeBackend {
        prepared: VecDeque::from([Box::new(root) as Box<dyn SurfaceHandle>]),
        log: Rc::new(RefCell::new(Log::default())),
        stop_after: usize::MAX,
        hotkeys: None,
        thumbnailer: None,
        file_icon_provider: None,
    };
    let mut host = Host::new(Box::new(backend));
    host.open(
        spec("page"),
        PanelPage {
            seen: Rc::clone(&seen),
        },
        OpenOptions::default(),
    )
    .expect("the page opens");
    host.run_until(None).expect("the page closes");
    let parent = log.parent_owners.borrow().clone();
    (log, parent, seen)
}

/// A panel the platform closed is reported to the app as a dismissal, and stays closed for
/// as long as the app keeps declaring it — declaring it again is what opens it again.
#[test]
fn a_closed_panel_is_reported_as_dismissed_and_not_reopened() {
    let (log, _parent, seen) = run_panels_with(true, true);
    assert_eq!(
        log.opened.borrow().len(),
        1,
        "a panel the platform closed must not be opened again while it is still declared"
    );
    assert!(
        seen.borrow().contains(&Some(ui_core::Id::str("panel"))),
        "the app is told which panel the platform closed: {:?}",
        seen.borrow()
    );
}

/// A panel with a surface of its own is opened once, shown there, and kept out of the
/// parent's list; a platform that cannot open one hands it back to the parent.
#[test]
fn a_panel_gets_a_surface_of_its_own_or_falls_back_to_its_parent() {
    let (log, parent_owners) = run_panels(true);
    let opened = log.opened.borrow();
    assert_eq!(opened.len(), 1, "one surface for one panel: {opened:?}");
    let (panel, spec) = opened[0];
    assert_eq!(
        (spec.rect.size.w, spec.rect.size.h),
        (100.0, 60.0),
        "the surface takes the panel's laid-out size"
    );
    let shown = log.shown.borrow();
    assert!(
        !shown.is_empty() && shown.iter().all(|node| *node == panel),
        "the panel's surface showed that panel and nothing else: {shown:?}"
    );
    assert!(
        !parent_owners.contains(&panel),
        "the parent's surface must not paint the panel"
    );

    // The same page, built the same way, names the same nodes.
    let (log, parent_owners) = run_panels(false);
    assert!(
        log.opened.borrow().is_empty(),
        "a presentation with no popups opens none"
    );
    assert!(
        log.shown.borrow().is_empty(),
        "nothing was shown in a surface of its own"
    );
    assert!(
        parent_owners.contains(&panel),
        "with no surface to open, the parent paints the panel"
    );
}

/// A page with a panel on a surface of its own, recording the pointer each frame is built
/// with, in tree coordinates.
struct PointerPage {
    seen: Rc<RefCell<Vec<Option<ui_core::geom::Point>>>>,
}

impl App for PointerPage {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        self.seen.borrow_mut().push(cx.pointer());
        use ui_core::layout::{Anchor, OverlayPlacement, Presentation, SizeClass};
        ui_platform::from_decl(
            Decl::container(Id::str("root"), NodeKind::Stack)
                .size(SizeClass::fixed(200.0, 200.0))
                .child(
                    Decl::container(Id::str("anchor"), NodeKind::Box)
                        .size(SizeClass::fixed(20.0, 20.0)),
                )
                .child(
                    Decl::container(Id::str("panel"), NodeKind::Box)
                        .size(SizeClass::fixed(100.0, 60.0))
                        .overlay(
                            OverlayPlacement::below(Anchor::Node(Id::str("anchor")))
                                .presentation(Presentation::Platform),
                        ),
                ),
        )
    }
}

/// The panel's half of a crossing: it holds the pointer the window lost and hands the
/// arrival to the tree on its own intake, before the window builds the frame.
struct CrossingPanel {
    pointer: Cell<Option<ui_core::geom::Point>>,
    /// Hand the arrival to the tree once, on the first intake after the surface exists.
    arrive: Cell<bool>,
}

struct CrossingPanelSurface {
    node: ui_core::NodeId,
    side: Rc<CrossingPanel>,
}

impl SurfaceHandle for CrossingPanelSurface {
    fn intake(&mut self, engine: &mut Engine) -> Result<(), String> {
        if self.side.arrive.take()
            && let Some(pos) = self.side.pointer.get()
        {
            engine.dispatch(ui_core::Event::PointerMoved { pos });
        }
        Ok(())
    }

    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        let mut target = OwnerTarget::default();
        engine.present_panel(self.node, &mut target)?;
        Ok(SurfaceStep { drew: true })
    }

    fn pointer(&self) -> Option<ui_core::geom::Point> {
        self.side.pointer.get()
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        None
    }

    fn is_closed(&self) -> bool {
        false
    }

    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        Vec::new()
    }
}

/// The window's half: its queue has not been read yet, so it still reports the place the
/// pointer last stood on it, and its step dispatches the leave the compositor sends the
/// moment the pointer enters the popup.
struct CrossingWindow {
    pointer: Cell<Option<ui_core::geom::Point>>,
    /// Step on which the leave is dispatched, before that step's frame is built.
    leave_on: usize,
    steps: Cell<usize>,
    panel: Rc<CrossingPanel>,
}

impl SurfaceHandle for CrossingWindow {
    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        let step = self.steps.get();
        if step == self.leave_on {
            engine.dispatch(ui_core::Event::PointerLeft);
        }
        let mut target = OwnerTarget::default();
        engine.frame_scene(&mut target, None)?;
        self.steps.set(step + 1);
        Ok(SurfaceStep { drew: true })
    }

    fn pointer(&self) -> Option<ui_core::geom::Point> {
        self.pointer.get()
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        None
    }

    fn is_closed(&self) -> bool {
        // One turn past the crossing: the frame in question is already built.
        self.steps.get() > self.leave_on
    }

    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        Vec::new()
    }

    fn open_popup(
        &mut self,
        node: ui_core::NodeId,
        _spec: ui_platform::window::PopupSpec,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        Ok(Box::new(CrossingPanelSurface {
            node,
            side: Rc::clone(&self.panel),
        }))
    }
}

/// The frame built in the turn the pointer crosses from the window onto its popup sees the
/// pointer where it arrived. The window's leave and the popup's motion arrive in the same
/// batch on different queues, so the window both reports a stale place and erases the tree's
/// pointer before the frame is built.
#[test]
fn a_pointer_crossing_between_surfaces_reaches_the_frame_it_builds() {
    let at = ui_core::geom::Point::new(140.0, 90.0);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let panel = Rc::new(CrossingPanel {
        pointer: Cell::new(Some(at)),
        arrive: Cell::new(true),
    });
    let backend = FakeBackend {
        prepared: VecDeque::from([Box::new(CrossingWindow {
            // Where the pointer last stood on the window, which has not read its queue yet.
            pointer: Cell::new(Some(ui_core::geom::Point::new(900.0, 600.0))),
            leave_on: 1,
            steps: Cell::new(0),
            panel: Rc::clone(&panel),
        }) as Box<dyn SurfaceHandle>]),
        log: Rc::new(RefCell::new(Log::default())),
        stop_after: usize::MAX,
        hotkeys: None,
        thumbnailer: None,
        file_icon_provider: None,
    };
    let mut host = Host::new(Box::new(backend));
    host.open(
        spec("page"),
        PointerPage {
            seen: Rc::clone(&seen),
        },
        OpenOptions::default(),
    )
    .expect("the page opens");
    host.run_until(None).expect("the page closes");
    let frames = seen.borrow().clone();
    assert!(
        frames.len() >= 2,
        "the crossing frame was built: {frames:?}"
    );
    assert_eq!(
        frames[1],
        Some(at),
        "the frame built as the pointer entered the popup sees it there: {frames:?}"
    );
}

/// A page recording the pointer each frame is built with, and nothing else.
struct PlainPage {
    seen: Rc<RefCell<Vec<Option<ui_core::geom::Point>>>>,
}

impl App for PlainPage {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        self.seen.borrow_mut().push(cx.pointer());
        ui_platform::from_decl(Decl::container(Id::str("page"), NodeKind::Box).child(
            Decl::container(Id::str("pin"), NodeKind::Box)
                .size(ui_core::layout::SizeClass::fixed(80.0, 80.0))
                .sense(ui_core::Sense::HOVER),
        ))
    }
}

/// A window as the host loop sees it: the pointer it reports is the one its queue held
/// before this turn, and reading that queue in its own step is what ends the hover.
struct LeavingWindow {
    /// Where the pointer last stood before the window's queue was read.
    pointer: Cell<Option<ui_core::geom::Point>>,
    steps: Cell<usize>,
}

const INSIDE: ui_core::geom::Point = ui_core::geom::Point::new(40.0, 40.0);

impl SurfaceHandle for LeavingWindow {
    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        let step = self.steps.get();
        match step {
            0 => {
                engine.dispatch(ui_core::Event::PointerMoved { pos: INSIDE });
            }
            1 => {
                engine.dispatch(ui_core::Event::PointerLeft);
                self.pointer.set(None);
            }
            _ => {}
        }
        let mut target = OwnerTarget::default();
        let pointer = (step == 0).then_some(INSIDE);
        engine.frame_scene(&mut target, pointer)?;
        self.steps.set(step + 1);
        Ok(SurfaceStep { drew: true })
    }

    fn pointer(&self) -> Option<ui_core::geom::Point> {
        self.pointer.get()
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        None
    }

    fn is_closed(&self) -> bool {
        self.steps.get() > 1
    }

    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        Vec::new()
    }
}

/// The frame the leave builds sees no pointer, so the hover it paints is gone in that turn.
///
/// A window's queue is read by its own step; the place it reports before that is last
/// turn's. The root's answer was used as the frame's pointer hint, so the leave's frame was
/// hit-tested where the pointer no longer was: the hover came out unchanged, no damage was
/// asked for, and a pin kept its hover skin until the next pointer event anywhere.
#[test]
fn a_leave_ends_the_hover_in_the_frame_it_builds() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let backend = FakeBackend {
        prepared: VecDeque::from([Box::new(LeavingWindow {
            pointer: Cell::new(Some(INSIDE)),
            steps: Cell::new(0),
        }) as Box<dyn SurfaceHandle>]),
        log: Rc::new(RefCell::new(Log::default())),
        stop_after: usize::MAX,
        hotkeys: None,
        thumbnailer: None,
        file_icon_provider: None,
    };
    let mut host = Host::new(Box::new(backend));
    host.open(
        spec("page"),
        PlainPage {
            seen: Rc::clone(&seen),
        },
        OpenOptions::default(),
    )
    .expect("the page opens");
    host.run_until(None).expect("the page closes");
    let frames = seen.borrow().clone();
    assert_eq!(
        frames,
        vec![Some(INSIDE), None],
        "the leave's own frame is built with no pointer: {frames:?}"
    );
}

/// A page with a panel inside a panel, both asking for a surface of their own.
struct NestedPage;

impl App for NestedPage {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        use ui_core::layout::{Anchor, OverlayPlacement, Presentation, SizeClass};
        ui_platform::from_decl(
            Decl::container(Id::str("root"), NodeKind::Stack)
                .size(SizeClass::fixed(400.0, 400.0))
                .child(
                    Decl::container(Id::str("panel"), NodeKind::Box)
                        .size(SizeClass::fixed(100.0, 60.0))
                        .overlay(
                            OverlayPlacement::below(Anchor::Node(Id::str("root")))
                                .presentation(Presentation::Platform),
                        )
                        .child(
                            Decl::container(Id::str("inner"), NodeKind::Box)
                                .size(SizeClass::fixed(40.0, 20.0))
                                .overlay(
                                    OverlayPlacement::below(Anchor::Node(Id::str("panel")))
                                        .presentation(Presentation::Platform),
                                ),
                        ),
                ),
        )
    }
}

/// A panel inside another is asked of the outer panel's surface, and its request is
/// measured from that surface rather than from the window under it.
#[test]
fn a_panel_inside_a_panel_is_placed_against_the_outer_surface() {
    let log = Rc::new(PanelLog::default());
    let backend = FakeBackend {
        prepared: VecDeque::from([Box::new(PanelRoot {
            log: Rc::clone(&log),
            budget: Cell::new(3),
            popups: true,
            close_panels: false,
        }) as Box<dyn SurfaceHandle>]),
        log: Rc::new(RefCell::new(Log::default())),
        stop_after: usize::MAX,
        hotkeys: None,
        thumbnailer: None,
        file_icon_provider: None,
    };
    let mut host = Host::new(Box::new(backend));
    host.open(spec("page"), NestedPage, OpenOptions::default())
        .expect("the page opens");
    host.run_until(None).expect("the page closes");

    let opened = log.opened.borrow();
    assert_eq!(opened.len(), 2, "both panels get a surface: {opened:?}");
    let rects = log.rects.borrow();
    let engine_rect = |width: f32| {
        rects
            .iter()
            .find(|(_, rect)| rect.size.w == width)
            .map(|(_, rect)| *rect)
            .expect("both panels are recorded by the layout")
    };
    let outer = engine_rect(100.0);
    let inner = engine_rect(40.0);
    assert_ne!(
        outer.origin, inner.origin,
        "the two panels are placed apart, so the coordinates below can tell them apart"
    );
    let request = opened
        .iter()
        .find(|(_, request)| request.rect.size.w == 40.0)
        .map(|(_, request)| *request)
        .expect("the inner panel's surface was asked of the outer one");
    assert_eq!(
        (
            request.rect.origin.x - (inner.origin.x - outer.origin.x),
            request.rect.origin.y - (inner.origin.y - outer.origin.y),
        ),
        (0.0, 0.0),
        "a nested panel's request starts at the outer surface's own origin: {request:?}"
    );
}

/// A page with a panel inside a panel, recording the pointer each frame is built with.
struct NestedPointerPage {
    seen: Rc<RefCell<Vec<Option<ui_core::geom::Point>>>>,
}

impl App for NestedPointerPage {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        self.seen.borrow_mut().push(cx.pointer());
        let mut nested = NestedPage;
        App::view(&mut nested, cx)
    }
}

/// A root whose panels each claim the pointer, as a flyout's surface does while it holds
/// the grab; the panel inside the outer one claims a different place.
struct ClaimingRoot {
    budget: Cell<usize>,
    outer: ui_core::geom::Point,
    inner: ui_core::geom::Point,
}

impl SurfaceHandle for ClaimingRoot {
    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        let mut target = OwnerTarget::default();
        engine.frame_scene(&mut target, None)?;
        self.budget.set(self.budget.get() - 1);
        Ok(SurfaceStep { drew: true })
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        None
    }

    fn is_closed(&self) -> bool {
        self.budget.get() == 0
    }

    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        Vec::new()
    }

    fn open_popup(
        &mut self,
        node: ui_core::NodeId,
        _spec: ui_platform::window::PopupSpec,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        Ok(Box::new(ClaimingPanel {
            node,
            point: self.outer,
            child: self.inner,
        }))
    }
}

struct ClaimingPanel {
    node: ui_core::NodeId,
    point: ui_core::geom::Point,
    /// The place the panel opened inside this one claims.
    child: ui_core::geom::Point,
}

impl SurfaceHandle for ClaimingPanel {
    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        let mut target = OwnerTarget::default();
        engine.present_panel(self.node, &mut target)?;
        Ok(SurfaceStep { drew: true })
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        None
    }

    fn is_closed(&self) -> bool {
        false
    }

    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        Vec::new()
    }

    fn pointer(&self) -> Option<ui_core::geom::Point> {
        Some(self.point)
    }

    fn open_popup(
        &mut self,
        node: ui_core::NodeId,
        _spec: ui_platform::window::PopupSpec,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        Ok(Box::new(ClaimingPanel {
            node,
            point: self.child,
            child: self.child,
        }))
    }
}

/// The deepest panel that claims the pointer is the one the frame takes.
///
/// An open flyout holds the grab and keeps hearing the mouse while the panel it hangs off
/// stops; asking the panels in the order they opened took the outer panel's stale sample,
/// which froze the menu's hover and kept the flyout open. Nothing else asks two panels for
/// one pointer at once.
#[test]
fn the_deeper_panel_owns_the_shared_pointer() {
    let inner = ui_core::geom::Point::new(60.0, 40.0);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let backend = FakeBackend {
        prepared: VecDeque::from([Box::new(ClaimingRoot {
            budget: Cell::new(3),
            outer: ui_core::geom::Point::new(10.0, 10.0),
            inner,
        }) as Box<dyn SurfaceHandle>]),
        log: Rc::new(RefCell::new(Log::default())),
        stop_after: usize::MAX,
        hotkeys: None,
        thumbnailer: None,
        file_icon_provider: None,
    };
    let mut host = Host::new(Box::new(backend));
    host.open(
        spec("page"),
        NestedPointerPage {
            seen: Rc::clone(&seen),
        },
        OpenOptions::default(),
    )
    .expect("the page opens");
    host.run_until(None).expect("the page closes");
    let frames = seen.borrow().clone();
    assert!(
        frames.len() >= 2,
        "a frame was built with both panels bound: {frames:?}"
    );
    assert_eq!(
        frames.last().copied().flatten(),
        Some(inner),
        "the frame takes the pointer of the panel that holds the grab: {frames:?}"
    );
}

// A dialog is a surface of its own whose input the host arbitrates; these fixtures drive
// that rule without a compositor.
/// An app whose whole window is one clickable node, so a press always has a target.
struct Clickable;

impl App for Clickable {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        let (width, height) = (cx.width() as f32, cx.height() as f32);
        ui_platform::from_decl(
            Decl::container(Id::str("hit"), NodeKind::Box)
                .size(ui_core::layout::SizeClass::fixed(width, height))
                .sense(ui_core::Sense::CLICK),
        )
    }
}

/// A root that presses its own window each step and records whether the engine took it.
struct DialogRoot {
    pressed: Rc<RefCell<Vec<bool>>>,
    budget: Cell<usize>,
    /// How many steps the dialog it opens stays up; `usize::MAX` leaves it open.
    dialog_steps: usize,
    /// Set when the dialog's surface is dropped.
    dropped: Rc<Cell<bool>>,
}

impl SurfaceHandle for DialogRoot {
    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        // Build first: a press is hit-tested against the frame layout.
        let mut target = OwnerTarget::default();
        engine.frame_scene(&mut target, None)?;
        let pos = ui_core::geom::Point::new(5.0, 5.0);
        engine.dispatch(ui_core::Event::PointerReleased {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        // The release above is not news; this press is, unless a modal dialog holds it.
        let took = engine.dispatch(ui_core::Event::PointerPressed {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        self.pressed.borrow_mut().push(took);
        self.budget.set(self.budget.get().saturating_sub(1));
        Ok(SurfaceStep { drew: true })
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        None
    }

    fn is_closed(&self) -> bool {
        self.budget.get() == 0
    }

    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        Vec::new()
    }

    fn open_dialog(
        &mut self,
        spec: SurfaceSpec,
        _options: PresentationOptions,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        assert!(
            matches!(spec.role, ui_platform::window::SurfaceRole::Dialog { .. }),
            "a dialog is asked for as a dialog"
        );
        Ok(Box::new(DialogSurface {
            steps: 0,
            close_after: self.dialog_steps,
            dropped: Rc::clone(&self.dropped),
        }))
    }
}

/// A dialog's own surface: it closes after a scripted number of steps.
struct DialogSurface {
    steps: usize,
    close_after: usize,
    dropped: Rc<Cell<bool>>,
}

impl Drop for DialogSurface {
    fn drop(&mut self) {
        self.dropped.set(true);
    }
}

impl SurfaceHandle for DialogSurface {
    fn step(&mut self, _engine: &mut Engine) -> Result<SurfaceStep, String> {
        self.steps = self.steps.saturating_add(1);
        Ok(SurfaceStep::default())
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        None
    }

    fn is_closed(&self) -> bool {
        self.steps >= self.close_after
    }

    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        Vec::new()
    }
}

/// A host over one `DialogRoot`.
fn dialog_host(
    budget: usize,
    dialog_steps: usize,
    pressed: Rc<RefCell<Vec<bool>>>,
    dropped: Rc<Cell<bool>>,
) -> Host {
    let root = DialogRoot {
        pressed,
        budget: Cell::new(budget),
        dialog_steps,
        dropped,
    };
    let backend = FakeBackend {
        prepared: VecDeque::from([Box::new(root) as Box<dyn SurfaceHandle>]),
        log: Rc::new(RefCell::new(Log::default())),
        // A bound on the wait, so a surface that should have closed ends the loop instead
        // of spinning; the assertion after the run is what reports it.
        stop_after: 64,
        hotkeys: None,
        thumbnailer: None,
        file_icon_provider: None,
    };
    Host::new(Box::new(backend))
}

/// While a modal dialog is open the parent paints but takes nothing; when it closes, input
/// comes back. This is the host's own filter, not the platform's hint.
#[test]
fn a_modal_dialog_blocks_its_parent_until_it_closes() {
    let pressed = Rc::new(RefCell::new(Vec::new()));
    let dropped = Rc::new(Cell::new(false));
    let mut host = dialog_host(12, 3, Rc::clone(&pressed), Rc::clone(&dropped));
    let parent = host
        .open(spec("parent"), Clickable, OpenOptions::default())
        .expect("the parent opens");
    let mut turn = 0;
    host.on_turn(move |ctx, _| {
        turn += 1;
        if turn == 2 {
            ctx.open_dialog(
                parent,
                SurfaceSpec::dialog("dev.crabsnap.test", "dialog", (200, 120)),
                Box::new(Blank),
                OpenOptions::default(),
            )
            .expect("the dialog opens");
        }
    });
    host.run_until(None).expect("the parent closes");

    let pressed = pressed.borrow();
    assert_eq!(
        pressed.first(),
        Some(&true),
        "the parent took input before the dialog: {pressed:?}"
    );
    assert!(
        pressed.contains(&false),
        "a modal dialog did not hold its parent: {pressed:?}"
    );
    assert_eq!(
        pressed.last(),
        Some(&true),
        "input did not come back after the dialog closed: {pressed:?}"
    );
    assert!(dropped.get(), "the dialog surface outlived its dismissal");
}

/// A dialog belongs to the surface that opened it: closing the parent closes the dialog.
#[test]
fn a_dialog_closes_with_the_surface_that_opened_it() {
    let pressed = Rc::new(RefCell::new(Vec::new()));
    let dropped = Rc::new(Cell::new(false));
    let mut host = dialog_host(2, usize::MAX, pressed, Rc::clone(&dropped));
    let parent = host
        .open(spec("parent"), Clickable, OpenOptions::default())
        .expect("the parent opens");
    host.open_dialog(
        parent,
        SurfaceSpec::dialog("dev.crabsnap.test", "dialog", (200, 120)),
        Blank,
        OpenOptions::default(),
    )
    .expect("the dialog opens");
    // A deadline rather than "until empty": if the dialog were not closed with its parent
    // the loop would never be empty, and the assertion below is what reports it.
    let deadline = Instant::now() + Duration::from_millis(50);
    host.run_until(Some(deadline)).expect("the parent closes");
    assert_eq!(host.surface_count(), 0, "the dialog outlived its parent");
    assert!(dropped.get(), "the dialog surface was never dropped");
}

/// A presentation with no dialog windows says so, and the caller keeps its own window.
#[test]
fn a_presentation_without_dialog_surfaces_refuses() {
    let (first, _steps) = scripted(&[Act::Idle]);
    let (mut host, _log) = host(vec![first], usize::MAX);
    let parent = host
        .open(spec("parent"), Blank, OpenOptions::default())
        .expect("the parent opens");
    let error = host
        .open_dialog(
            parent,
            SurfaceSpec::dialog("dev.crabsnap.test", "dialog", (100, 100)),
            Blank,
            OpenOptions::default(),
        )
        .expect_err("a scripted surface has no dialogs");
    assert!(error.contains("no dialog surfaces"), "{error}");
}
