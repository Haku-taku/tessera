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
use ui_platform::host::{Backend, Effect, Host, OpenOptions, SurfaceHandle, SurfaceStep};
use ui_platform::hotkeys::{GlobalHotkeyError, GlobalHotkeys};
use ui_platform::ipc::{Channel, Reply};
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
    fn add_image(&mut self, image: ImageEntry) -> Result<(), String> {
        self.images.borrow_mut().push(image.id);
        Ok(())
    }

    fn step(&mut self) -> Result<SurfaceStep, String> {
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

    fn next_deadline(&self) -> Option<Instant> {
        self.deadline
    }

    fn request_frame(&mut self) {
        self.requested.set(self.requested.get() + 1);
    }

    fn is_closed(&self) -> bool {
        self.closed
    }

    fn take_copies(&mut self) -> Vec<ClipboardCopy> {
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
    prepared: VecDeque<Scripted>,
    log: Rc<RefCell<Log>>,
    /// Stop the loop once this many waits have happened; the fake never blocks.
    stop_after: usize,
    hotkeys: Option<ScriptedHotkeys>,
}

impl Backend for FakeBackend {
    fn open(
        &mut self,
        _spec: SurfaceSpec,
        _app: Box<dyn App>,
        _options: OpenOptions,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        match self.prepared.pop_front() {
            Some(surface) => Ok(Box::new(surface)),
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
    let backend = FakeBackend {
        prepared: surfaces.into_iter().collect(),
        log: Rc::clone(&log),
        stop_after,
        hotkeys,
    };
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
        4,
        "every turn's texture and request asked the surface to build"
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
