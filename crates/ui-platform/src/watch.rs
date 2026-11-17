//! A directory that reports when its contents change, watched by the platform.
//!
//! The provider runs a watch on its own thread; the handle answers `changed` from a flag the
//! provider sets, and the loop forces a frame so the picker can read the directory again.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::jobs::Waker;

/// The platform half. `changed` is called after the quiet period that follows a burst of
/// notifications, so one save or copy is one call.
pub trait DirectoryWatcher: Send + Sync {
    fn watch(
        &self,
        path: &Path,
        changed: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Box<dyn WatchGuard>, String>;
}

/// A live watch. Dropping it stops the platform watch; the provider owns its thread.
pub trait WatchGuard: Send {}

/// The app's handle to directory watches. Cheap to clone; every clone shares one counter.
#[derive(Clone)]
pub struct Watches {
    shared: Arc<Shared>,
}

struct Shared {
    provider: Box<dyn DirectoryWatcher>,
    waker: Arc<dyn Waker>,
    /// Changes reported so far; the loop forces a frame when this moves.
    completed: AtomicU64,
}

/// One watched directory. `changed` answers once per reported change.
pub struct Watch {
    changed: Arc<AtomicBool>,
    _guard: Box<dyn WatchGuard>,
}

impl Watches {
    pub fn new(provider: Box<dyn DirectoryWatcher>, waker: Arc<dyn Waker>) -> Self {
        Self {
            shared: Arc::new(Shared {
                provider,
                waker,
                completed: AtomicU64::new(0),
            }),
        }
    }

    /// Watch `path`, or `None` where the platform refused. A refused watch is not an error
    /// the dialog can act on: the listing still works, it just does not refresh itself.
    pub fn watch(&self, path: &Path) -> Option<Watch> {
        let changed = Arc::new(AtomicBool::new(false));
        let signal = {
            let shared = Arc::clone(&self.shared);
            let changed = Arc::clone(&changed);
            Arc::new(move || {
                changed.store(true, Ordering::Release);
                shared.completed.fetch_add(1, Ordering::Release);
                shared.waker.wake();
            }) as Arc<dyn Fn() + Send + Sync>
        };
        match self.shared.provider.watch(path, signal) {
            Ok(guard) => Some(Watch {
                changed,
                _guard: guard,
            }),
            Err(error) => {
                log::warn!(
                    "ui-platform: cannot watch {}: {error}",
                    path.display()
                );
                None
            }
        }
    }

    /// How many changes have been reported. The loop forces one frame when this moves.
    pub fn completed(&self) -> u64 {
        self.shared.completed.load(Ordering::Acquire)
    }
}

impl Watch {
    /// Whether anything changed since the last answer; asking clears the flag.
    pub fn changed(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
    }
}
