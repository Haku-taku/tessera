//! Finding entries by name under a directory, walked on a worker as the user types.
//!
//! A search is not a listing: the tree may be deep, so a newer query stops the walk where it
//! is instead of waiting for the whole tree, and matches are handed over as they are found.

use std::cell::Cell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::jobs::Waker;
use crate::listing::{Entry, HiddenRule, read_entry};

/// Matches kept for one query. A broader query is answered by narrowing it, and the result
/// is memory the app holds until the picker closes.
pub const MAX_RESULTS: usize = 2000;

/// Matches per answer, so a deep tree paints while the walk is still running.
const BATCH: usize = 64;

/// A name search under one directory.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SearchRequest {
    pub root: PathBuf,
    /// Matched case-insensitively against the entry's name; must not be empty.
    pub query: String,
    pub show_hidden: bool,
}

impl SearchRequest {
    pub fn new(root: impl Into<PathBuf>, query: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            query: query.into(),
            show_hidden: false,
        }
    }

    pub fn show_hidden(mut self, show: bool) -> Self {
        self.show_hidden = show;
        self
    }
}

/// The platform half. Called on the module's worker thread; `stop` is read between entries
/// and before every subdirectory, and `found` may be called many times.
pub trait DirectorySearcher: Send + Sync {
    fn search(
        &self,
        request: &SearchRequest,
        found: &mut dyn FnMut(Entry),
        stop: &dyn Fn() -> bool,
    ) -> Result<(), String>;
}

/// A recursive name search with `std::fs`: the portable half of a [`DirectorySearcher`].
pub struct FsDirectorySearcher {
    hidden: HiddenRule,
}

impl FsDirectorySearcher {
    pub fn new(hidden: HiddenRule) -> Self {
        Self { hidden }
    }

    /// The dot-prefix rule, which is what unix desktops use and what XDG names.
    pub fn dotfiles() -> Self {
        Self::new(|name, _| name.starts_with('.'))
    }
}

impl DirectorySearcher for FsDirectorySearcher {
    fn search(
        &self,
        request: &SearchRequest,
        found: &mut dyn FnMut(Entry),
        stop: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        let query = request.query.to_lowercase();
        if query.is_empty() {
            return Ok(());
        }
        let root = std::fs::read_dir(&request.root)
            .map_err(|error| format!("{}: {error}", request.root.display()))?;
        let mut pending = vec![root];
        // A junction or a bind mount can point back at an ancestor; the canonical path is
        // what keeps the walk from going round a loop until the user gives up.
        let mut visited: HashSet<PathBuf> = HashSet::new();
        if let Ok(canonical) = std::fs::canonicalize(&request.root) {
            visited.insert(canonical);
        }
        let mut matches = 0usize;
        while let Some(read) = pending.pop() {
            for item in read {
                if stop() {
                    return Ok(());
                }
                let item = match item {
                    Ok(item) => item,
                    Err(error) => {
                        log::debug!("ui-platform: skipping a searched entry: {error}");
                        continue;
                    }
                };
                let Some(entry) = read_entry(item, self.hidden, request.show_hidden) else {
                    continue;
                };
                if entry.directory && !entry.link {
                    let canonical = std::fs::canonicalize(&entry.path).ok();
                    if canonical
                        .as_ref()
                        .is_none_or(|path| visited.insert(path.clone()))
                    {
                        match std::fs::read_dir(&entry.path) {
                            Ok(child) => pending.push(child),
                            Err(error) => log::debug!(
                                "ui-platform: skipping a searched directory {}: {error}",
                                entry.path.display()
                            ),
                        }
                    }
                }
                if entry.name.to_lowercase().contains(&query) {
                    matches += 1;
                    found(entry);
                    if matches >= MAX_RESULTS {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
}

/// What a [`Searches`] handle answers.
#[derive(Clone, Debug)]
pub enum SearchState {
    /// No search is running.
    Idle,
    /// The walk is still finding; these are the matches so far.
    Running(Arc<Vec<Entry>>),
    Done(Arc<Vec<Entry>>),
    Failed(Arc<str>),
}

/// The app's handle to searches. Cheap to clone; every clone shares one worker, and a new
/// query stops the walk the previous one started.
#[derive(Clone)]
pub struct Searches {
    shared: Arc<Shared>,
}

struct Shared {
    slot: Mutex<Slot>,
    sender: Sender<SearchRequest>,
    waker: Arc<dyn Waker>,
    /// Answers produced so far; the loop forces a frame when this moves.
    completed: AtomicU64,
    /// Which query the worker should still be answering.
    generation: AtomicU64,
}

struct Slot {
    generation: u64,
    state: SearchState,
}

impl Searches {
    pub fn new(searcher: Box<dyn DirectorySearcher>, waker: Arc<dyn Waker>) -> Self {
        let (sender, receiver) = mpsc::channel::<SearchRequest>();
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot {
                generation: 0,
                state: SearchState::Idle,
            }),
            sender,
            waker,
            completed: AtomicU64::new(0),
            generation: AtomicU64::new(0),
        });
        let worker = Arc::clone(&shared);
        if let Err(error) = thread::Builder::new()
            .name("searches".into())
            .spawn(move || run(worker, searcher, receiver))
        {
            log::warn!("ui-platform: no search worker: {error}");
        }
        Self { shared }
    }

    /// Ask for `request`, stopping whatever walk is running. The state is `Running` at once,
    /// so the view can say the query is being answered before the first match lands.
    pub fn start(&self, request: SearchRequest) {
        let generation = self.shared.generation.fetch_add(1, Ordering::AcqRel) + 1;
        {
            let mut slot = lock(&self.shared.slot);
            slot.generation = generation;
            slot.state = SearchState::Running(Arc::new(Vec::new()));
        }
        self.shared.bump();
        if self.shared.sender.send(request).is_err() {
            let mut slot = lock(&self.shared.slot);
            if slot.generation == generation {
                slot.state = SearchState::Failed(Arc::from("no search worker"));
            }
            drop(slot);
            self.shared.bump();
        }
    }

    /// Stop the running walk and forget the matches. A closed picker must not leave a walk
    /// reading a tree nobody is looking at.
    pub fn cancel(&self) {
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        let mut slot = lock(&self.shared.slot);
        slot.state = SearchState::Idle;
        drop(slot);
        self.shared.bump();
    }

    pub fn state(&self) -> SearchState {
        lock(&self.shared.slot).state.clone()
    }

    /// How many answers have been produced. The loop forces one frame when this moves.
    pub fn completed(&self) -> u64 {
        self.shared.completed.load(Ordering::Acquire)
    }
}

impl Shared {
    fn bump(&self) {
        self.completed.fetch_add(1, Ordering::Release);
        self.waker.wake();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn run(shared: Arc<Shared>, searcher: Box<dyn DirectorySearcher>, receiver: Receiver<SearchRequest>) {
    while let Ok(first) = receiver.recv() {
        // Only the newest query matters: a request typed while the last one was queued has
        // already replaced it in the picker.
        let request = receiver.try_iter().last().unwrap_or(first);
        let generation = shared.generation.load(Ordering::Acquire);
        {
            let mut slot = lock(&shared.slot);
            if slot.generation != generation {
                continue;
            }
            slot.state = SearchState::Running(Arc::new(Vec::new()));
        }
        shared.bump();

        let reached = Cell::new(false);
        let total = Cell::new(0usize);
        let stop = || reached.get() || shared.generation.load(Ordering::Acquire) != generation;
        let mut buffer = Vec::new();
        let mut found = |entry: Entry| {
            total.set(total.get() + 1);
            buffer.push(entry);
            if buffer.len() >= BATCH {
                flush(&shared, generation, &mut buffer);
            }
            if total.get() >= MAX_RESULTS {
                reached.set(true);
            }
        };
        let result = searcher.search(&request, &mut found, &stop);
        drop(found);
        flush(&shared, generation, &mut buffer);

        let answer = if shared.generation.load(Ordering::Acquire) != generation {
            None
        } else {
            match result {
                Ok(()) => Some(SearchState::Done(current(&shared, generation))),
                Err(reason) => Some(SearchState::Failed(Arc::from(reason))),
            }
        };
        if let Some(answer) = answer {
            let mut slot = lock(&shared.slot);
            if slot.generation == generation {
                slot.state = answer;
            }
        }
        shared.bump();
    }
}

/// Move the matches found since the last answer into the state the picker reads.
fn flush(shared: &Shared, generation: u64, buffer: &mut Vec<Entry>) {
    if buffer.is_empty() {
        return;
    }
    let kept = {
        let mut slot = lock(&shared.slot);
        if slot.generation != generation {
            false
        } else if let SearchState::Running(entries) = &mut slot.state {
            Arc::make_mut(entries).append(buffer);
            true
        } else {
            false
        }
    };
    buffer.clear();
    if kept {
        shared.bump();
    }
}

fn current(shared: &Shared, generation: u64) -> Arc<Vec<Entry>> {
    let slot = lock(&shared.slot);
    if slot.generation != generation {
        return Arc::new(Vec::new());
    }
    match &slot.state {
        SearchState::Running(entries) | SearchState::Done(entries) => Arc::clone(entries),
        _ => Arc::new(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    struct NoWaker;

    impl Waker for NoWaker {
        fn wake(&self) {}
    }

    /// A walk that reports when it starts and waits until the test lets it go, so the second
    /// query is always asked for while the first walk is running.
    struct SlowSearcher {
        started: Mutex<Option<Sender<()>>>,
        proceed: Mutex<Option<Receiver<()>>>,
        stopped: Arc<AtomicBool>,
    }

    impl DirectorySearcher for SlowSearcher {
        fn search(
            &self,
            request: &SearchRequest,
            found: &mut dyn FnMut(Entry),
            stop: &dyn Fn() -> bool,
        ) -> Result<(), String> {
            if let Some(started) = self.started.lock().ok().and_then(|mut s| s.take()) {
                let _ = started.send(());
            }
            if let Some(proceed) = self.proceed.lock().ok().and_then(|mut p| p.take()) {
                let _ = proceed.recv();
            }
            // A short walk: if stop never arrives the walk ends on its own, and the test sees
            // that it was not cut short rather than waiting for the result cap.
            for index in 0..10 {
                if stop() {
                    self.stopped.store(true, Ordering::Release);
                    return Ok(());
                }
                found(Entry {
                    name: format!("{}-{index}", request.query),
                    path: PathBuf::from(format!("{}-{index}", request.query)),
                    kind: crate::files::FileKind::Generic,
                    directory: false,
                    link: false,
                    len: 0,
                    modified: None,
                    hidden: false,
                });
            }
            Ok(())
        }
    }

    fn wait_until(mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready() && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(ready(), "the worker never got there");
    }

    /// A newer query stops the walk the older one started, and only the newer answer is left
    /// for the picker: a stale tree must not overwrite the rows the user is waiting for.
    #[test]
    fn a_new_query_stops_the_previous_walk_and_only_its_answer_survives() {
        let (started, started_receiver) = mpsc::channel();
        let (proceed, proceed_receiver) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let searches = Searches::new(
            Box::new(SlowSearcher {
                started: Mutex::new(Some(started)),
                proceed: Mutex::new(Some(proceed_receiver)),
                stopped: Arc::clone(&stopped),
            }),
            Arc::new(NoWaker),
        );
        searches.start(SearchRequest::new("somewhere", "old"));
        started_receiver.recv().expect("the walk never started");
        searches.start(SearchRequest::new("somewhere", "new"));
        let _ = proceed.send(());
        wait_until(|| stopped.load(Ordering::Acquire));
        wait_until(|| matches!(searches.state(), SearchState::Done(_)));
        assert!(stopped.load(Ordering::Acquire));
        let SearchState::Done(entries) = searches.state() else {
            unreachable!("the search is done");
        };
        assert!(!entries.is_empty());
        assert!(
            entries.iter().all(|entry| entry.name.starts_with("new-")),
            "a stopped walk's matches reached the rows: {:?}",
            entries.first().map(|entry| &entry.name)
        );
    }
}
