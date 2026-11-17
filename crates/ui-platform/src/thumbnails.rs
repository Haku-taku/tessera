//! The picture a desktop's thumbnail system can show for a file.
//!
//! Nothing here reads a file: a platform [`Thumbnailer`] does that on the worker this module
//! shares with file icons (`crate::pictures`), and the handle answers `Loading`, `Ready` or
//! `Unavailable` from memory. The platform's cache, decoder and thumbnailer programs are the
//! provider's business; this module owns the request, the answer and the pixels' lifetime.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ui_core::colour::ContentColour;
use ui_core::draw::{ImageEntry, ImageSource, Sampling};

use crate::jobs::Waker;
use crate::keep_alive::KeepAlive;
use crate::IDLE_WINDOW;
use crate::pictures::{Limits, Pictures, Produced, State};

/// Answers kept in memory: cheap keys naming a picture that may since have been evicted.
const MAX_REQUESTS: usize = 4096;
/// Pictures kept in memory, and the bytes they may hold.
const MAX_PICTURES: usize = 512;
const MAX_BYTES: usize = 64 << 20;

/// Thumbnail ids start above every app's own small constants and counters, because the
/// surface's texture table is keyed by id and a collision would overwrite one picture with
/// another. The prefix spells "thumb" in ASCII.
const ID_BASE: u64 = 0x7468_756d_6200_0000;

/// What the caller saw of a file on disk. Part of the cache key, so a file rewritten between
/// two looks is a different request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileVersion {
    pub len: u64,
    /// Whole and partial seconds since the Unix epoch, so it hashes and compares.
    pub modified: Option<Duration>,
}

/// One file, at one size, as of one version.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ThumbnailRequest {
    pub path: PathBuf,
    /// The longer side in physical pixels. A provider may answer with a larger picture.
    pub size: u32,
    /// `None` when the caller did not look; the picture then lives for the run or until
    /// [`Thumbnails::forget`].
    pub version: Option<FileVersion>,
}

impl ThumbnailRequest {
    pub fn new(path: impl Into<PathBuf>, size: u32) -> Self {
        Self {
            path: path.into(),
            size,
            version: None,
        }
    }

    pub fn versioned(mut self, version: FileVersion) -> Self {
        self.version = Some(version);
        self
    }
}

/// Straight-alpha RGBA8 pixels a provider made, or found in the platform's cache.
#[derive(Clone, Debug)]
pub struct ThumbnailPicture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The platform half. Called on the module's worker thread, where blocking is expected and
/// a slow call delays only the pictures behind it.
pub trait Thumbnailer: Send + Sync {
    /// The picture for this file, or `None` when the platform has none.
    fn thumbnail(&self, request: &ThumbnailRequest) -> Option<ThumbnailPicture>;
}

/// What a [`Thumbnails::get`] call answers.
#[derive(Clone, Debug)]
pub enum ThumbnailState {
    /// The worker is on it; ask again after the frame its completion forces.
    Loading,
    Ready(ImageEntry),
    /// The platform has no picture for this file, for the rest of the run.
    Unavailable,
}

/// The app's handle to thumbnails. Cheap to clone; every clone shares one worker and cache.
#[derive(Clone)]
pub struct Thumbnails {
    pictures: Pictures<ThumbnailRequest>,
}

impl Thumbnails {
    /// Start the worker. `waker` is what forces a frame when a picture arrives.
    ///
    /// Construction never fails: a worker that could not start answers `Unavailable` for
    /// everything, which the app draws as no picture. Failing here would take the window
    /// with it.
    pub fn new(provider: Box<dyn Thumbnailer>, waker: Arc<dyn Waker>) -> Self {
        Self {
            pictures: Pictures::new(
                "thumbnails",
                waker,
                Limits {
                    requests: MAX_REQUESTS,
                    pictures: MAX_PICTURES,
                    bytes: MAX_BYTES,
                    idle: IDLE_WINDOW,
                },
                move |request| {
                    let picture = provider.thumbnail(request)?;
                    let entry = picture_entry(request, picture)?;
                    Some(Produced {
                        identity: entry.id,
                        entry,
                    })
                },
            ),
        }
    }

    /// The answer for this file: from memory, or a queued request that answers `Loading`.
    pub fn get(&self, request: &ThumbnailRequest) -> ThumbnailState {
        match self.pictures.get(request) {
            State::Loading => ThumbnailState::Loading,
            State::Ready(entry) => ThumbnailState::Ready(entry),
            State::Absent => ThumbnailState::Unavailable,
        }
    }

    /// Drop every answer for this file, at every size, so the next `get` asks again.
    pub fn forget(&self, path: &Path) {
        self.pictures.forget(|request| request.path == path);
    }

    /// Drop every picture. The cache is the app's memory, not the platform's.
    pub fn clear(&self) {
        self.pictures.clear();
    }

    /// Keep the pictures through the idle window while the returned claim lives: a window
    /// that is open but has not drawn for a minute still wants them.
    pub fn keep_alive(&self) -> KeepAlive {
        self.pictures.keep_alive()
    }

    /// How many requests have finished. The loop forces one frame when this moves.
    pub fn completed(&self) -> u64 {
        self.pictures.completed()
    }
}

/// The id names the picture, so a picture made again replaces the engine's copy instead of
/// adding a second: the engine keeps every registered id it is handed for the run.
fn picture_entry(request: &ThumbnailRequest, picture: ThumbnailPicture) -> Option<ImageEntry> {
    let expected = picture.width as usize * picture.height as usize * 4;
    if picture.width == 0 || picture.height == 0 || picture.rgba.len() != expected {
        log::warn!(
            "ui-platform: a thumbnail came back {}×{} with {} bytes, not {expected}",
            picture.width,
            picture.height,
            picture.rgba.len()
        );
        return None;
    }
    let mut hasher = DefaultHasher::new();
    request.hash(&mut hasher);
    Some(ImageEntry {
        id: ID_BASE | hasher.finish(),
        width: picture.width,
        height: picture.height,
        source: ImageSource::Pixels(Arc::new(picture.rgba)),
        colour: ContentColour::SDR,
        sampling: Sampling::Smooth,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pictures::MAX_QUEUED;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{Receiver as Rx, Sender, channel};
    use std::sync::Mutex;
    use std::thread;
    use std::time::Instant;

    struct CountingWaker(AtomicUsize);

    impl Waker for CountingWaker {
        fn wake(&self) {
            self.0.fetch_add(1, Ordering::Release);
        }
    }

    /// A provider the test releases one call at a time, so every queue state is reachable.
    struct Manual {
        calls: Arc<Mutex<Vec<(PathBuf, u32)>>>,
        release: Mutex<Rx<()>>,
        answer: fn(&ThumbnailRequest) -> Option<ThumbnailPicture>,
    }

    impl Thumbnailer for Manual {
        fn thumbnail(&self, request: &ThumbnailRequest) -> Option<ThumbnailPicture> {
            if let Ok(mut calls) = self.calls.lock() {
                calls.push((request.path.clone(), request.size));
            }
            if let Ok(release) = self.release.lock() {
                let _ = release.recv_timeout(Duration::from_secs(5));
            }
            (self.answer)(request)
        }
    }

    fn manual(
        answer: fn(&ThumbnailRequest) -> Option<ThumbnailPicture>,
    ) -> (Box<Manual>, Sender<()>) {
        let (release, receiver) = channel();
        let provider = Box::new(Manual {
            calls: Arc::new(Mutex::new(Vec::new())),
            release: Mutex::new(receiver),
            answer,
        });
        (provider, release)
    }

    fn square(request: &ThumbnailRequest) -> Option<ThumbnailPicture> {
        let side = request.size;
        Some(ThumbnailPicture {
            width: side,
            height: side,
            rgba: vec![7; side as usize * side as usize * 4],
        })
    }

    fn nothing(_request: &ThumbnailRequest) -> Option<ThumbnailPicture> {
        None
    }

    fn waker() -> Arc<dyn Waker> {
        Arc::new(CountingWaker(AtomicUsize::new(0)))
    }

    fn wait_for(thumbnails: &Thumbnails, done: u64) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while thumbnails.completed() < done && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(thumbnails.completed() >= done, "the worker never finished");
    }

    /// One request per key, whatever the caller asks per frame, and the size reaches the
    /// provider as given.
    #[test]
    fn a_queued_key_is_asked_once() {
        let (provider, release) = manual(square);
        let calls = Arc::clone(&provider.calls);
        let thumbnails = Thumbnails::new(provider, waker());
        let request = ThumbnailRequest::new("/tmp/a.png", 96);
        for _ in 0..3 {
            assert!(matches!(thumbnails.get(&request), ThumbnailState::Loading));
        }
        release.send(()).expect("the provider is listening");
        wait_for(&thumbnails, 1);
        assert_eq!(
            calls.lock().expect("not poisoned").as_slice(),
            [(PathBuf::from("/tmp/a.png"), 96)]
        );
        match thumbnails.get(&request) {
            ThumbnailState::Ready(entry) => {
                assert_eq!((entry.width, entry.height), (96, 96));
                assert!(entry.id >= ID_BASE, "thumbnail ids live above the app's");
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    /// A file rewritten between two looks is a different key, and the old picture is not
    /// handed out for it.
    #[test]
    fn a_newer_version_is_a_new_request() {
        let (provider, release) = manual(square);
        let calls = Arc::clone(&provider.calls);
        let thumbnails = Thumbnails::new(provider, waker());
        let old = FileVersion {
            len: 10,
            modified: Some(Duration::from_secs(1)),
        };
        let new = FileVersion {
            len: 20,
            modified: Some(Duration::from_secs(2)),
        };
        let first = ThumbnailRequest::new("/tmp/a.png", 64).versioned(old);
        thumbnails.get(&first);
        release.send(()).expect("the provider is listening");
        wait_for(&thumbnails, 1);
        assert!(matches!(thumbnails.get(&first), ThumbnailState::Ready(_)));

        let second = ThumbnailRequest::new("/tmp/a.png", 64).versioned(new);
        assert!(matches!(thumbnails.get(&second), ThumbnailState::Loading));
        release.send(()).expect("the provider is listening");
        wait_for(&thumbnails, 2);
        assert_eq!(calls.lock().expect("not poisoned").len(), 2);
    }

    /// An answer of "no picture" is cached: a corrupt file is asked about once, not once per
    /// frame, and `forget` is what asks again.
    #[test]
    fn an_unavailable_file_is_asked_once() {
        let (provider, release) = manual(nothing);
        let calls = Arc::clone(&provider.calls);
        let thumbnails = Thumbnails::new(provider, waker());
        let request = ThumbnailRequest::new("/tmp/a.bin", 64);
        thumbnails.get(&request);
        release.send(()).expect("the provider is listening");
        wait_for(&thumbnails, 1);
        for _ in 0..3 {
            assert!(matches!(
                thumbnails.get(&request),
                ThumbnailState::Unavailable
            ));
        }
        assert_eq!(calls.lock().expect("not poisoned").len(), 1);
        thumbnails.forget(Path::new("/tmp/a.bin"));
        assert!(matches!(thumbnails.get(&request), ThumbnailState::Loading));
    }

    /// A burst larger than the queue keeps the newest requests and drops the oldest.
    #[test]
    fn the_queue_keeps_the_newest_requests() {
        let (provider, release) = manual(square);
        let calls = Arc::clone(&provider.calls);
        let thumbnails = Thumbnails::new(provider, waker());
        // The fills below queue behind this request, so the worker must have taken it out of
        // the queue first; a thread that starts late would leave the blocker as the queue's
        // oldest entry, and the first fill would displace it instead of a filler.
        thumbnails.get(&ThumbnailRequest::new("/tmp/blocker.png", 1));
        let started = Instant::now() + Duration::from_secs(10);
        while calls.lock().expect("not poisoned").is_empty() && Instant::now() < started {
            thread::yield_now();
        }
        assert!(
            !calls.lock().expect("not poisoned").is_empty(),
            "the worker never took the blocking request"
        );
        let total = MAX_QUEUED + 40;
        for index in 0..total {
            thumbnails.get(&ThumbnailRequest::new(format!("/tmp/{index}.png"), 1));
        }
        for _ in 0..=total {
            release.send(()).expect("the provider is listening");
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while calls.lock().expect("not poisoned").len() <= MAX_QUEUED
            && Instant::now() < deadline
        {
            thread::yield_now();
        }
        let calls = calls.lock().expect("not poisoned");
        assert_eq!(
            calls.len(),
            1 + MAX_QUEUED,
            "the blocker and one queue's worth"
        );
        assert!(
            calls
                .iter()
                .any(|(path, _)| path == Path::new(&format!("/tmp/{}.png", total - 1))),
            "the newest request survives"
        );
        assert!(
            !calls.iter().any(|(path, _)| path == Path::new("/tmp/0.png")),
            "the oldest queued request was displaced"
        );
    }

    /// A picture keeps its id when it is made again. The engine keeps every registered id
    /// for the run, so minting a fresh one per production leaks the pixels it names.
    #[test]
    fn a_remade_thumbnail_keeps_its_id() {
        let (provider, release) = manual(square);
        let thumbnails = Thumbnails::new(provider, waker());
        let request = ThumbnailRequest::new("/tmp/a.png", 32);
        thumbnails.get(&request);
        release.send(()).expect("the provider is listening");
        wait_for(&thumbnails, 1);
        let ThumbnailState::Ready(first) = thumbnails.get(&request) else {
            panic!("the first picture is ready");
        };
        thumbnails.forget(Path::new("/tmp/a.png"));
        assert!(matches!(thumbnails.get(&request), ThumbnailState::Loading));
        release.send(()).expect("the provider is listening");
        wait_for(&thumbnails, 2);
        let ThumbnailState::Ready(second) = thumbnails.get(&request) else {
            panic!("the re-made picture is ready");
        };
        assert_eq!(first.id, second.id, "one picture, one id");
    }

    /// The picture is one `Arc` for the life of the entry, which is what lets the renderer
    /// upload it once.
    #[test]
    fn a_ready_entry_hands_over_the_same_pixels() {
        let (provider, release) = manual(square);
        let thumbnails = Thumbnails::new(provider, waker());
        let request = ThumbnailRequest::new("/tmp/a.png", 32);
        thumbnails.get(&request);
        release.send(()).expect("the provider is listening");
        wait_for(&thumbnails, 1);
        let (first, second) = (thumbnails.get(&request), thumbnails.get(&request));
        match (first, second) {
            (ThumbnailState::Ready(a), ThumbnailState::Ready(b)) => {
                let (Some(a), Some(b)) = (a.pixels(), b.pixels()) else {
                    panic!("a thumbnail is pixels");
                };
                assert!(Arc::ptr_eq(a, b), "the same Arc, so one upload");
            }
            other => panic!("expected two Ready answers, got {other:?}"),
        }
    }
}
