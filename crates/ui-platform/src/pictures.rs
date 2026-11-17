//! One worker for pictures: a request becomes a shared image off the frame's thread.
//!
//! The thumbnail and the file-icon service ask the same three questions — ready, being made,
//! or never coming — and both want the newest request served first, because a list scrolling
//! fast wants the rows on screen and not the ones it flew past. This module owns that queue,
//! its bounds and the completion counter once.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use ui_core::draw::ImageEntry;

use crate::jobs::Waker;
use crate::keep_alive::{self, KeepAlive};

/// Requests allowed to wait: beyond this the oldest one is dropped, because a picker
/// scrolling fast wants the rows it is looking at and not the ones it flew past.
pub(crate) const MAX_QUEUED: usize = 256;

/// What the worker made for one request.
pub(crate) struct Produced {
    /// Pictures sharing this number share one upload; the first one made is kept. The id the
    /// entry already carries must name this picture, so a picture made again replaces the
    /// engine's copy rather than appending another to a table that lives for the run.
    pub identity: u64,
    pub entry: ImageEntry,
}

/// What [`Pictures::get`] answers.
#[derive(Clone, Debug)]
pub(crate) enum State {
    /// The worker is on it; ask again after the frame its completion forces.
    Loading,
    Ready(ImageEntry),
    /// The producer has no picture for this request.
    Absent,
}

/// What one service keeps in memory.
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    /// Answers kept, one per request. The answer is the cheap half: a key and an identity.
    pub requests: usize,
    /// Pictures kept, and the bytes they may own.
    pub pictures: usize,
    pub bytes: usize,
    /// How long the whole cache survives after the last request.
    pub idle: Duration,
}

/// The service's handle. Cheap to clone; every clone shares one worker and cache.
pub(crate) struct Pictures<R> {
    shared: Arc<Shared<R>>,
}

impl<R> Clone for Pictures<R> {
    fn clone(&self) -> Self {
        self.shared.handles.fetch_add(1, Ordering::AcqRel);
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<R> Drop for Pictures<R> {
    fn drop(&mut self) {
        if self.shared.handles.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }
        // Under the lock the worker waits on: a notify without it could slip between the
        // worker's emptiness check and its wait, and leave the thread parked for ever.
        let mut queue = lock(&self.shared.queue);
        queue.closed = true;
        self.shared.ready.notify_all();
    }
}

struct Shared<R> {
    queue: Mutex<Queue<R>>,
    /// Signals the worker that requests are waiting; the queue is the state, not a channel,
    /// so a displaced request cannot linger behind the ones the user is looking at.
    ready: Condvar,
    waker: Arc<dyn Waker>,
    /// Completions so far; the loop compares it to know a picture is waiting.
    completed: AtomicU64,
    limits: Limits,
    /// The worker's thread started; a `get` otherwise has nobody to hand its request to.
    up: AtomicBool,
    /// Handles to this service. The worker exits when the last one goes, taking the cache
    /// with it: a closed picker's pictures must not outlive every holder that asked.
    handles: AtomicUsize,
    /// Claims that a live consumer is still drawing from the cache; the idle sweep waits.
    held: Arc<AtomicUsize>,
}

struct Queue<R> {
    /// What each request answered.
    answers: HashMap<R, AnswerSlot>,
    /// One entry per identity and size: a provider may answer one identity at more than one
    /// size, and the larger box must not be handed the smaller picture.
    pictures: HashMap<PictureKey, PictureSlot>,
    /// Keys the worker has not taken, oldest first; it takes from the back.
    waiting: VecDeque<R>,
    pending: HashSet<R>,
    total_bytes: usize,
    tick: u64,
    /// When the last `get` or taken request arrived; the cache is dropped once this is older
    /// than the idle limit.
    asked_at: Instant,
    /// The last handle dropped; the worker returns instead of waiting for more requests.
    closed: bool,
}

struct AnswerSlot {
    answer: Answer,
    tick: u64,
}

#[derive(Clone, Copy)]
enum Answer {
    /// The producer has no picture for this request; cached like a picture.
    Absent,
    Picture(PictureKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PictureKey {
    identity: u64,
    width: u32,
    height: u32,
}

struct PictureSlot {
    entry: ImageEntry,
    tick: u64,
}

impl PictureSlot {
    fn bytes(&self) -> usize {
        self.entry.width as usize * self.entry.height as usize * 4
    }
}

impl<R: Clone + Eq + Hash + Send + 'static> Pictures<R> {
    /// Start the worker. `name` names its thread and its log line; `waker` is what forces a
    /// frame when a picture arrives.
    ///
    /// Construction never fails: a worker that could not start leaves every `get` answering
    /// `Absent`, which the caller draws as no picture. Failing here would take the window
    /// with it.
    pub(crate) fn new(
        name: &str,
        waker: Arc<dyn Waker>,
        limits: Limits,
        produce: impl Fn(&R) -> Option<Produced> + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue {
                answers: HashMap::new(),
                pictures: HashMap::new(),
                waiting: VecDeque::new(),
                pending: HashSet::new(),
                total_bytes: 0,
                tick: 0,
                asked_at: Instant::now(),
                closed: false,
            }),
            ready: Condvar::new(),
            waker,
            completed: AtomicU64::new(0),
            limits,
            up: AtomicBool::new(false),
            handles: AtomicUsize::new(1),
            held: Arc::new(AtomicUsize::new(0)),
        });
        let worker = Arc::clone(&shared);
        match thread::Builder::new()
            .name(name.into())
            .spawn(move || run(worker, produce))
        {
            Ok(_) => shared.up.store(true, Ordering::Release),
            Err(error) => log::warn!("ui-platform: no {name} worker: {error}"),
        }
        Self { shared }
    }

    /// The answer for this request: from memory, or a queued request that answers `Loading`.
    pub(crate) fn get(&self, request: &R) -> State {
        let limits = self.shared.limits;
        let mut queue = lock(&self.shared.queue);
        queue.tick += 1;
        let tick = queue.tick;
        queue.asked_at = Instant::now();
        match queue.answers.get(request).map(|slot| slot.answer) {
            Some(Answer::Absent) => return State::Absent,
            Some(Answer::Picture(key)) => {
                if let Some(picture) = queue.pictures.get_mut(&key) {
                    picture.tick = tick;
                    let entry = picture.entry.clone();
                    if let Some(slot) = queue.answers.get_mut(request) {
                        slot.tick = tick;
                    }
                    return State::Ready(entry);
                }
                // The pixels were evicted while the answer stayed; ask again below.
                queue.answers.remove(request);
            }
            None => {}
        }
        if queue.pending.contains(request) {
            return State::Loading;
        }
        if queue.waiting.len() >= MAX_QUEUED
            && let Some(dropped) = queue.waiting.pop_front()
        {
            queue.pending.remove(&dropped);
        }
        if !self.shared.up.load(Ordering::Acquire) {
            queue.answers.insert(
                R::clone(request),
                AnswerSlot {
                    answer: Answer::Absent,
                    tick,
                },
            );
            queue.evict(limits);
            return State::Absent;
        }
        queue.pending.insert(R::clone(request));
        queue.waiting.push_back(R::clone(request));
        self.shared.ready.notify_one();
        State::Loading
    }

    /// Drop every answer matching `dropped`, so the next `get` asks again. Pictures stay
    /// until the bounds evict them, because another request may still name one.
    pub(crate) fn forget(&self, dropped: impl Fn(&R) -> bool) {
        let mut queue = lock(&self.shared.queue);
        queue.answers.retain(|request, _| !dropped(request));
        queue.pending.retain(|request| !dropped(request));
        queue.waiting.retain(|request| !dropped(request));
    }

    /// Drop every answer and picture. The cache is the app's memory, not the platform's.
    pub(crate) fn clear(&self) {
        let mut queue = lock(&self.shared.queue);
        queue.answers.clear();
        queue.pictures.clear();
        queue.total_bytes = 0;
        queue.asked_at = Instant::now();
    }

    /// Keep this worker's cache through the idle window while the returned claim lives.
    pub(crate) fn keep_alive(&self) -> KeepAlive {
        KeepAlive::new(&self.shared.held)
    }

    /// How many requests have finished. The loop forces one frame when this moves.
    pub(crate) fn completed(&self) -> u64 {
        self.shared.completed.load(Ordering::Acquire)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn wait<'a, R>(
    ready: &Condvar,
    queue: MutexGuard<'a, Queue<R>>,
    timeout: Duration,
) -> MutexGuard<'a, Queue<R>> {
    match ready.wait_timeout(queue, timeout) {
        Ok((guard, _)) => guard,
        Err(poisoned) => poisoned.into_inner().0,
    }
}

/// Counts a request as finished on the way out, including when the producer panicked: a
/// count that froze would leave the loop waiting for a picture that never arrives.
struct Finish<'a> {
    completed: &'a AtomicU64,
    waker: &'a dyn Waker,
}

impl Drop for Finish<'_> {
    fn drop(&mut self) {
        self.completed.fetch_add(1, Ordering::Release);
        self.waker.wake();
    }
}

/// The worker: take the newest request, make it, hand it to the cache, repeat.
///
/// A request is taken whole from the queue, so a displaced one is never made. Only this
/// thread pops, so nothing can steal a request between the pop and the call.
fn run<R: Clone + Eq + Hash + Send + 'static>(
    shared: Arc<Shared<R>>,
    produce: impl Fn(&R) -> Option<Produced>,
) {
    loop {
        let request = {
            let mut queue = lock(&shared.queue);
            loop {
                if queue.closed {
                    return;
                }
                if let Some(request) = queue.waiting.pop_back() {
                    queue.pending.remove(&request);
                    queue.asked_at = Instant::now();
                    break request;
                }
                if queue.asked_at.elapsed() >= shared.limits.idle {
                    // Nobody asked for a picture for the whole window: the cache is the
                    // app's memory, and a window closed an hour ago must not still own it.
                    // A claim means the window is open and merely idle, so the cache stays.
                    if keep_alive::released(&shared.held) {
                        queue.answers.clear();
                        queue.pictures.clear();
                        queue.total_bytes = 0;
                    }
                    queue.asked_at = Instant::now();
                }
                let timeout = shared.limits.idle.saturating_sub(queue.asked_at.elapsed());
                queue = wait(&shared.ready, queue, timeout);
            }
        };
        let _finish = Finish {
            completed: &shared.completed,
            waker: &*shared.waker,
        };
        settle(&shared, &request, produce(&request));
    }
}

fn settle<R: Clone + Eq + Hash>(shared: &Shared<R>, request: &R, produced: Option<Produced>) {
    let mut queue = lock(&shared.queue);
    queue.tick += 1;
    let tick = queue.tick;
    let answer = match produced {
        None => Answer::Absent,
        Some(Produced { identity, entry }) => {
            let key = PictureKey {
                identity,
                width: entry.width,
                height: entry.height,
            };
            if queue.pictures.contains_key(&key) {
                // One picture, one upload: keep the entry already made and touch it.
                if let Some(picture) = queue.pictures.get_mut(&key) {
                    picture.tick = tick;
                }
            } else {
                let slot = PictureSlot { entry, tick };
                queue.total_bytes += slot.bytes();
                queue.pictures.insert(key, slot);
            }
            Answer::Picture(key)
        }
    };
    queue.answers.insert(R::clone(request), AnswerSlot { answer, tick });
    queue.evict(shared.limits);
}

impl<R: Clone + Eq + Hash> Queue<R> {
    fn evict(&mut self, limits: Limits) {
        while self.pictures.len() > limits.pictures || self.total_bytes > limits.bytes {
            let oldest = self
                .pictures
                .iter()
                .min_by_key(|(_, slot)| slot.tick)
                .map(|(key, _)| *key);
            let Some(oldest) = oldest else {
                break;
            };
            if let Some(dropped) = self.pictures.remove(&oldest) {
                self.total_bytes = self.total_bytes.saturating_sub(dropped.bytes());
            }
        }
        while self.answers.len() > limits.requests {
            let oldest = self
                .answers
                .iter()
                .min_by_key(|(_, slot)| slot.tick)
                .map(|(request, _)| R::clone(request));
            let Some(oldest) = oldest else {
                break;
            };
            self.answers.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::colour::ContentColour;
    use ui_core::draw::{ImageSource, Sampling};

    struct CountingWaker(AtomicUsize);

    impl Waker for CountingWaker {
        fn wake(&self) {
            self.0.fetch_add(1, Ordering::Release);
        }
    }

    fn wait_until(mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready() && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(ready(), "the worker never got there");
    }

    /// A cache nobody has asked from for the idle window is dropped, and the next look asks
    /// the producer again: the pixels a closed picker decoded are the app's memory.
    #[test]
    fn an_idle_cache_is_dropped_and_the_next_look_asks_again() {
        let idle = Duration::from_millis(30);
        let made = Arc::new(AtomicUsize::new(0));
        let produced = Arc::clone(&made);
        let pictures: Pictures<u32> = Pictures::new(
            "test pictures",
            Arc::new(CountingWaker(AtomicUsize::new(0))),
            Limits {
                requests: 8,
                pictures: 8,
                bytes: 1 << 20,
                idle,
            },
            move |_request| {
                produced.fetch_add(1, Ordering::Release);
                Some(Produced {
                    identity: 1,
                    entry: ImageEntry {
                        id: 7,
                        width: 1,
                        height: 1,
                        source: ImageSource::Pixels(Arc::new(vec![0, 0, 0, 255])),
                        colour: ContentColour::SDR,
                        sampling: Sampling::Smooth,
                    },
                })
            },
        );
        assert!(matches!(pictures.get(&1), State::Loading));
        wait_until(|| pictures.completed() == 1);
        assert!(matches!(pictures.get(&1), State::Ready(_)));
        // Every `get` counts as use, so the cache must be left alone for the whole window
        // before it goes; the first look after that is a request, not a hit.
        let deadline = Instant::now() + idle * 10;
        while Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(
            matches!(pictures.get(&1), State::Loading),
            "the idle cache was kept"
        );
        wait_until(|| pictures.completed() == 2);
        assert_eq!(made.load(Ordering::Acquire), 2);
    }

    /// A window that is open but has not drawn for the idle window still owns its pictures:
    /// a modal left alone for a minute must not decode and upload its icons again on the
    /// first frame after the pointer moves.
    #[test]
    fn a_claimed_cache_survives_the_idle_window() {
        let idle = Duration::from_millis(30);
        let made = Arc::new(AtomicUsize::new(0));
        let produced = Arc::clone(&made);
        let pictures: Pictures<u32> = Pictures::new(
            "test pictures",
            Arc::new(CountingWaker(AtomicUsize::new(0))),
            Limits {
                requests: 8,
                pictures: 8,
                bytes: 1 << 20,
                idle,
            },
            move |_request| {
                produced.fetch_add(1, Ordering::Release);
                Some(Produced {
                    identity: 1,
                    entry: ImageEntry {
                        id: 7,
                        width: 1,
                        height: 1,
                        source: ImageSource::Pixels(Arc::new(vec![0, 0, 0, 255])),
                        colour: ContentColour::SDR,
                        sampling: Sampling::Smooth,
                    },
                })
            },
        );
        let claim = pictures.keep_alive();
        assert!(matches!(pictures.get(&1), State::Loading));
        wait_until(|| pictures.completed() == 1);

        let deadline = Instant::now() + idle * 10;
        while Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(
            matches!(pictures.get(&1), State::Ready(_)),
            "the claimed cache was dropped"
        );
        assert_eq!(made.load(Ordering::Acquire), 1, "the picture was made again");

        // The claim is the whole difference: released, the window applies as before.
        drop(claim);
        let deadline = Instant::now() + idle * 10;
        while Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(matches!(pictures.get(&1), State::Loading));
    }
}
