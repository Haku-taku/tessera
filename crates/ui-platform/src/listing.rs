//! What a directory holds, asked for once and read on a worker.
//!
//! Nothing here reads a file: a platform [`DirectoryLister`] does that on the module's
//! worker, and the handle answers `Loading`, `Ready` or `Failed` from memory. Ordering,
//! filtering and hidden-file policy above the dotfile rule belong to the view; the lister
//! answers the entries in the order the platform returned them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::IDLE_WINDOW;
use crate::keep_alive::{self, KeepAlive};
use crate::files::FileKind;
use crate::jobs::Waker;
use crate::thumbnails::FileVersion;

/// Requests allowed to wait: navigating faster than a directory reads replaces the queue,
/// because only the directory the user is looking at matters.
const MAX_QUEUED: usize = 16;

/// Listings kept in memory. A picker walks back and forth through a few directories; each
/// answer is the entries alone, so this is generous.
const MAX_ENTRIES: usize = 32;

/// One directory entry as the platform sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The file name, lossily decoded for display; `path` keeps the real bytes.
    pub name: String,
    pub path: PathBuf,
    /// The type the name implies; a directory is [`FileKind::Folder`].
    pub kind: FileKind,
    pub directory: bool,
    /// A symbolic link; the type follows the target. A junction is a reparse point that this
    /// flag does not carry, so a badge that must appear on one comes from the platform's own
    /// answer rather than from here.
    pub link: bool,
    /// The length in bytes; zero for a directory.
    pub len: u64,
    pub modified: Option<Duration>,
    pub hidden: bool,
}

impl Entry {
    /// The version a thumbnail request keys on, so a rewritten file is a new request.
    pub fn version(&self) -> FileVersion {
        FileVersion {
            len: self.len,
            modified: self.modified,
        }
    }
}

/// One directory, as of one hidden-files setting.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ListingRequest {
    pub path: PathBuf,
    /// Dotfiles are left out entirely when this is false, as GTK and KDE do by default.
    pub show_hidden: bool,
}

impl ListingRequest {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            show_hidden: false,
        }
    }

    pub fn show_hidden(mut self, show: bool) -> Self {
        self.show_hidden = show;
        self
    }
}

/// The platform half. Called on the module's worker thread, where blocking is expected and
/// a slow directory delays only the directories behind it.
pub trait DirectoryLister: Send + Sync {
    /// The entries `request` names, or why the directory could not be read.
    fn entries(&self, request: &ListingRequest) -> Result<Vec<Entry>, String>;
}

/// The platform's rule for a hidden entry, from its file name and metadata.
///
/// Unix desktops hide a dotfile and nothing else; Windows also hides what an attribute marks,
/// which is why the portable reader is told the rule instead of choosing one.
pub type HiddenRule = fn(&str, Option<&std::fs::Metadata>) -> bool;

/// A directory read with `std::fs`: the portable half of a [`DirectoryLister`].
pub struct FsDirectoryLister {
    hidden: HiddenRule,
}

impl FsDirectoryLister {
    pub fn new(hidden: HiddenRule) -> Self {
        Self { hidden }
    }

    /// The dot-prefix rule, which is what unix desktops use and what XDG names.
    pub fn dotfiles() -> Self {
        Self::new(|name, _| name.starts_with('.'))
    }
}

impl DirectoryLister for FsDirectoryLister {
    fn entries(&self, request: &ListingRequest) -> Result<Vec<Entry>, String> {
        let read = std::fs::read_dir(&request.path)
            .map_err(|error| format!("{}: {error}", request.path.display()))?;
        let mut entries = Vec::new();
        for item in read {
            let item = match item {
                Ok(item) => item,
                // One unreadable entry does not fail the directory: racing a deleted file
                // is normal while a directory is being listed.
                Err(error) => {
                    log::debug!("ui-platform: skipping an entry: {error}");
                    continue;
                }
            };
            if let Some(entry) = read_entry(item, self.hidden, request.show_hidden) {
                entries.push(entry);
            }
        }
        Ok(entries)
    }
}

/// One directory item, read the way both the listing and the search read it; `None` when
/// the hidden rule leaves it out.
pub(crate) fn read_entry(
    item: std::fs::DirEntry,
    hidden: HiddenRule,
    show_hidden: bool,
) -> Option<Entry> {
    let name = item.file_name().to_string_lossy().into_owned();
    let path = item.path();
    let file_type = item.file_type().ok();
    let link = file_type.as_ref().is_some_and(|kind| kind.is_symlink());
    // The entry's own metadata is what the hidden rule reads: a link marked hidden is hidden
    // whatever it points at.
    let own = item.metadata().ok();
    let hidden = hidden(&name, own.as_ref());
    if hidden && !show_hidden {
        return None;
    }
    // `DirEntry::metadata` does not follow a link, so a link is asked about again through
    // its target: the row's type and size describe what it opens.
    let metadata = if link {
        std::fs::metadata(&path).ok()
    } else {
        own
    };
    let directory = metadata
        .as_ref()
        .map(|metadata| metadata.is_dir())
        .or_else(|| file_type.map(|kind| kind.is_dir()))
        .unwrap_or(false);
    let len = match &metadata {
        Some(metadata) if !directory => metadata.len(),
        _ => 0,
    };
    let modified = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok());
    let kind = if directory {
        FileKind::Folder
    } else {
        FileKind::of_path(&path)
    };
    Some(Entry {
        name,
        path,
        kind,
        directory,
        link,
        len,
        modified,
        hidden,
    })
}

/// What a [`Listings::get`] call answers.
#[derive(Clone, Debug)]
pub enum ListingState {
    /// The worker is on it; ask again after the frame its completion forces.
    Loading,
    Ready(Arc<Vec<Entry>>),
    Failed(Arc<str>),
}

/// The app's handle to directory listings. Cheap to clone; every clone shares one worker
/// and cache.
#[derive(Clone)]
pub struct Listings {
    shared: Arc<Shared>,
}

struct Shared {
    queue: Mutex<Queue>,
    sender: Sender<ListingRequest>,
    waker: Arc<dyn Waker>,
    /// Completions so far; the loop compares it to know an answer is waiting.
    completed: AtomicU64,
    /// How long the cache survives after the last ask.
    idle: Duration,
    /// Claims that a live consumer is still drawing from the cache; the idle sweep waits.
    held: Arc<AtomicUsize>,
}

struct Queue {
    entries: std::collections::HashMap<ListingRequest, Slot>,
    /// Requests handed to the worker and not yet taken; the worker takes the newest first.
    waiting: Vec<ListingRequest>,
    pending: std::collections::HashSet<ListingRequest>,
    tick: u64,
    /// When the cache was last asked for; the worker drops it once this is older than `idle`.
    asked_at: Instant,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            waiting: Vec::new(),
            pending: std::collections::HashSet::new(),
            tick: 0,
            asked_at: Instant::now(),
        }
    }
}

struct Slot {
    result: Result<Arc<Vec<Entry>>, Arc<str>>,
    /// Order of last use, for eviction.
    tick: u64,
}

impl Listings {
    /// Start the worker. `waker` is what forces a frame when a directory lands.
    ///
    /// Construction never fails: a worker that could not start drops its receiver, and the
    /// first `get` then answers `Failed` for every directory, which the picker draws as an
    /// error row. Failing here would take the window with it.
    pub fn new(provider: Box<dyn DirectoryLister>, waker: Arc<dyn Waker>) -> Self {
        Self::with_idle(provider, waker, IDLE_WINDOW)
    }

    /// The same service with its own idle window, which only a test picks.
    fn with_idle(
        provider: Box<dyn DirectoryLister>,
        waker: Arc<dyn Waker>,
        idle: Duration,
    ) -> Self {
        let (sender, receiver) = mpsc::channel::<ListingRequest>();
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            sender,
            waker,
            completed: AtomicU64::new(0),
            idle,
            held: Arc::new(AtomicUsize::new(0)),
        });
        let worker = Arc::clone(&shared);
        if let Err(error) = thread::Builder::new()
            .name("listings".into())
            .spawn(move || run(worker, provider, receiver))
        {
            log::warn!("ui-platform: no directory worker: {error}");
        }
        Self { shared }
    }

    /// The answer for this directory: from memory, or a queued request that answers
    /// `Loading`.
    pub fn get(&self, request: &ListingRequest) -> ListingState {
        let mut queue = lock(&self.shared.queue);
        queue.tick += 1;
        queue.asked_at = Instant::now();
        let tick = queue.tick;
        if let Some(slot) = queue.entries.get_mut(request) {
            slot.tick = tick;
            return match &slot.result {
                Ok(entries) => ListingState::Ready(Arc::clone(entries)),
                Err(reason) => ListingState::Failed(Arc::clone(reason)),
            };
        }
        if queue.pending.contains(request) {
            return ListingState::Loading;
        }
        if queue.waiting.len() >= MAX_QUEUED
            && !queue.waiting.is_empty()
        {
            // The queue keeps the newest requests, so the oldest one is the one dropped.
            let dropped = queue.waiting.remove(0);
            queue.pending.remove(&dropped);
        }
        if self.shared.sender.send(request.clone()).is_err() {
            let reason: Arc<str> = Arc::from("no directory worker");
            queue.entries.insert(
                request.clone(),
                Slot {
                    result: Err(Arc::clone(&reason)),
                    tick,
                },
            );
            return ListingState::Failed(reason);
        }
        queue.pending.insert(request.clone());
        queue.waiting.push(request.clone());
        ListingState::Loading
    }

    /// Forget one directory, so the next `get` reads it again. A picker calls this when the
    /// user navigates into a directory: what was listed before this run has changed.
    pub fn invalidate(&self, path: &Path) {
        let mut queue = lock(&self.shared.queue);
        queue.asked_at = Instant::now();
        queue.entries.retain(|request, _| request.path != path);
    }

    /// Keep this worker's cache through the idle window while the returned claim lives.
    pub fn keep_alive(&self) -> KeepAlive {
        KeepAlive::new(&self.shared.held)
    }

    /// How many listings have finished. The loop forces one frame when this moves.
    pub fn completed(&self) -> u64 {
        self.shared.completed.load(Ordering::Acquire)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Counts a request as finished on the way out, including when the provider panicked: a
/// count that froze would leave the loop waiting for a directory that never arrives.
struct Finish<'a>(&'a Shared);

impl Drop for Finish<'_> {
    fn drop(&mut self) {
        self.0.completed.fetch_add(1, Ordering::Release);
        self.0.waker.wake();
    }
}

fn run(shared: Arc<Shared>, provider: Box<dyn DirectoryLister>, receiver: Receiver<ListingRequest>) {
    loop {
        let wait = {
            let queue = lock(&shared.queue);
            shared.idle.saturating_sub(queue.asked_at.elapsed())
        };
        let first = match receiver.recv_timeout(wait) {
            Ok(first) => first,
            Err(RecvTimeoutError::Timeout) => {
                // Nobody has asked for a directory for the whole window: what a closed picker
                // read is the app's memory, so it goes. A claim means the picker is open and
                // merely idle, so the cache stays.
                let mut queue = lock(&shared.queue);
                if queue.asked_at.elapsed() >= shared.idle {
                    if keep_alive::released(&shared.held) {
                        queue.entries.clear();
                    }
                    queue.asked_at = Instant::now();
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        };
        // Take the newest first: the directory the user navigated to now is the one read
        // first, and the one they left is usually skipped below.
        let mut batch = vec![first];
        batch.extend(receiver.try_iter());
        for request in batch.into_iter().rev() {
            let wanted = {
                let mut queue = lock(&shared.queue);
                let kept = queue.pending.remove(&request);
                if kept {
                    queue.waiting.retain(|queued| queued != &request);
                }
                kept
            };
            if !wanted {
                // Superseded while it waited: the user has navigated elsewhere.
                continue;
            }
            let _finish = Finish(&shared);
            let result = provider
                .entries(&request)
                .map(Arc::new)
                .map_err(Arc::<str>::from);
            settle(&shared, &request, result);
        }
    }
}

fn settle(shared: &Shared, request: &ListingRequest, result: Result<Arc<Vec<Entry>>, Arc<str>>) {
    let mut queue = lock(&shared.queue);
    queue.tick += 1;
    let tick = queue.tick;
    queue.entries.insert(request.clone(), Slot { result, tick });
    while queue.entries.len() > MAX_ENTRIES {
        let Some(oldest) = queue
            .entries
            .iter()
            .min_by_key(|(_, slot)| slot.tick)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        queue.entries.remove(&oldest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct CountingLister(Arc<AtomicUsize>);

    impl DirectoryLister for CountingLister {
        fn entries(&self, request: &ListingRequest) -> Result<Vec<Entry>, String> {
            self.0.fetch_add(1, Ordering::Release);
            Ok(vec![Entry {
                name: String::from("a"),
                path: request.path.join("a"),
                kind: FileKind::Generic,
                directory: false,
                link: false,
                len: 1,
                modified: None,
                hidden: false,
            }])
        }
    }

    struct NoWaker;

    impl Waker for NoWaker {
        fn wake(&self) {}
    }

    fn wait_until(mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready() && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(ready(), "the worker never got there");
    }

    /// A directory read for a picker that has closed is the app's memory: once nobody has
    /// asked for it for the window the cache goes, and the next look reads again.
    #[test]
    fn an_idle_cache_is_dropped_and_the_next_look_asks_again() {
        let idle = Duration::from_millis(30);
        let reads = Arc::new(AtomicUsize::new(0));
        let listings = Listings::with_idle(
            Box::new(CountingLister(Arc::clone(&reads))),
            Arc::new(NoWaker),
            idle,
        );
        let request = ListingRequest::new("somewhere");
        assert!(matches!(listings.get(&request), ListingState::Loading));
        wait_until(|| listings.completed() == 1);
        assert!(matches!(listings.get(&request), ListingState::Ready(_)));
        // Every look counts, so the cache must be left alone for the whole window before it
        // goes; the first look after that is a request, not a hit.
        let deadline = Instant::now() + idle * 10;
        while Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(
            matches!(listings.get(&request), ListingState::Loading),
            "the idle cache was kept"
        );
        wait_until(|| listings.completed() == 2);
        assert_eq!(reads.load(Ordering::Acquire), 2);
    }

    /// A dialog that is still open keeps its listings: the idle window alone must not make
    /// the worker read a directory again the moment the pointer moves.
    #[test]
    fn a_claimed_cache_survives_the_idle_window() {
        let idle = Duration::from_millis(30);
        let reads = Arc::new(AtomicUsize::new(0));
        let listings = Listings::with_idle(
            Box::new(CountingLister(Arc::clone(&reads))),
            Arc::new(NoWaker),
            idle,
        );
        let claim = listings.keep_alive();
        let request = ListingRequest::new("somewhere");
        assert!(matches!(listings.get(&request), ListingState::Loading));
        wait_until(|| listings.completed() == 1);

        let deadline = Instant::now() + idle * 10;
        while Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(
            matches!(listings.get(&request), ListingState::Ready(_)),
            "the claimed cache was dropped"
        );
        assert_eq!(reads.load(Ordering::Acquire), 1, "the directory was read again");

        // The claim is the whole difference: released, the window applies as before.
        drop(claim);
        let deadline = Instant::now() + idle * 10;
        while Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(matches!(listings.get(&request), ListingState::Loading));
    }
}
