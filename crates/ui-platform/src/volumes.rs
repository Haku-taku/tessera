//! Volumes a desktop can mount but has not: the other half of the device list a sidebar
//! offers. A mounted volume is already a [`Device`](crate::places::Device); these are the
//! removable media the user inserted and no mount point has claimed yet.
//!
//! Enumeration and mounting run on this module's worker, like every other platform
//! service: mounting a disk can take seconds, and the frame that asked for it must not wait.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use crate::jobs::Waker;

/// One volume the desktop can mount but has not.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Mountable {
    /// The platform's own name for the volume; opaque to the picker, and what
    /// [`VolumeMounter::mount`] takes.
    pub id: String,
    /// The name the volume gives itself, when it has one.
    pub label: Option<String>,
}

/// The platform half. Called on this module's worker, where blocking is expected.
pub trait VolumeMounter: Send + Sync {
    /// The volumes that exist and have no mount point.
    fn mountable(&self) -> Result<Vec<Mountable>, String>;
    /// Mount the volume `id` names, and answer the directory it is now reachable at.
    fn mount(&self, id: &str) -> Result<PathBuf, String>;
}

/// What a [`Volumes::state`] call answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VolumesState {
    /// The worker is on it; ask again after the frame its completion forces.
    Loading,
    Ready(Arc<Vec<Mountable>>),
    Failed(Arc<str>),
}

/// A mount that finished since the app last drained the queue.
pub type MountOutcome = Result<PathBuf, Arc<str>>;

/// The app's handle to the platform's mountable volumes. Cheap to clone; every clone shares
/// one worker.
#[derive(Clone)]
pub struct Volumes {
    shared: Arc<Shared>,
}

struct Shared {
    state: Mutex<State>,
    sender: Sender<Job>,
    waker: Arc<dyn Waker>,
    /// Completions so far; the loop compares it to know an answer is waiting.
    completed: AtomicU64,
}

#[derive(Default)]
struct State {
    /// The last answer, kept while a refresh runs so the sidebar does not blank.
    list: Option<VolumesState>,
    /// A list is queued or running; a second refresh would only queue a third.
    listing: bool,
    mounted: Vec<MountOutcome>,
}

enum Job {
    List,
    Mount(String),
}

impl Volumes {
    /// Start the worker and ask it for the list once. `waker` is what forces a frame when
    /// an answer lands.
    pub fn new(provider: Box<dyn VolumeMounter>, waker: Arc<dyn Waker>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                list: None,
                listing: true,
                mounted: Vec::new(),
            }),
            sender: sender.clone(),
            waker,
            completed: AtomicU64::new(0),
        });
        // The channel is buffered, so the first request waits for the worker to take it.
        if sender.send(Job::List).is_err() {
            lock(&shared.state).listing = false;
        }
        let worker = Arc::clone(&shared);
        if let Err(error) = thread::Builder::new()
            .name("volumes".into())
            .spawn(move || run(worker, provider, receiver))
        {
            log::warn!("ui-platform: no volume worker: {error}");
            lock(&shared.state).list = Some(VolumesState::Failed(Arc::from(
                "no volume worker",
            )));
        }
        Self { shared }
    }

    /// The list as of the last answer, or `Loading` before the first one.
    pub fn state(&self) -> VolumesState {
        lock(&self.shared.state)
            .list
            .clone()
            .unwrap_or(VolumesState::Loading)
    }

    /// Ask for the list again. Volumes appear and disappear while the dialog is open, so a
    /// sidebar calls this when it has reason to believe the desktop changed.
    pub fn refresh(&self) {
        let mut state = lock(&self.shared.state);
        if state.listing {
            return;
        }
        state.listing = true;
        if self.shared.sender.send(Job::List).is_err() {
            state.listing = false;
            state.list = Some(VolumesState::Failed(Arc::from("no volume worker")));
        }
    }

    /// Mount the volume `id` names. False when there is no worker to take it.
    pub fn mount(&self, id: &str) -> bool {
        self.shared.sender.send(Job::Mount(id.to_string())).is_ok()
    }

    /// Mounts that finished since the last call, in completion order.
    pub fn take_mounted(&self) -> Vec<MountOutcome> {
        std::mem::take(&mut lock(&self.shared.state).mounted)
    }

    /// How many list and mount jobs have finished. The loop forces one frame when this
    /// moves.
    pub fn completed(&self) -> u64 {
        self.shared.completed.load(Ordering::Acquire)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn run(shared: Arc<Shared>, provider: Box<dyn VolumeMounter>, receiver: Receiver<Job>) {
    while let Ok(job) = receiver.recv() {
        match job {
            Job::List => {
                let result = provider.mountable().map(Arc::new).map_err(Arc::<str>::from);
                let mut state = lock(&shared.state);
                state.listing = false;
                state.list = Some(match result {
                    Ok(list) => VolumesState::Ready(list),
                    Err(reason) => VolumesState::Failed(reason),
                });
            }
            Job::Mount(id) => {
                let outcome = provider.mount(&id).map_err(Arc::<str>::from);
                let mut state = lock(&shared.state);
                state.mounted.push(outcome);
                // The volume is mounted or the attempt failed; either way the list moved.
                state.listing = true;
                let _ = shared.sender.send(Job::List);
            }
        }
        shared.completed.fetch_add(1, Ordering::Release);
        shared.waker.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    use std::time::{Duration, Instant};

    struct Fake {
        mounted: Arc<AtomicBool>,
        lists: Arc<AtomicUsize>,
    }

    impl VolumeMounter for Fake {
        fn mountable(&self) -> Result<Vec<Mountable>, String> {
            self.lists.fetch_add(1, Ordering::Release);
            Ok(if self.mounted.load(Ordering::Acquire) {
                Vec::new()
            } else {
                vec![Mountable {
                    id: "disk".into(),
                    label: Some("USB".into()),
                }]
            })
        }

        fn mount(&self, id: &str) -> Result<PathBuf, String> {
            self.mounted.store(true, Ordering::Release);
            Ok(PathBuf::from("/mnt").join(id))
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

    /// A mount answers the directory it produced, and the list that follows no longer
    /// offers the volume: a sidebar that kept it would mount the same disk twice.
    #[test]
    fn a_mount_reports_its_point_and_the_next_list_drops_the_volume() {
        let mounted = Arc::new(AtomicBool::new(false));
        let lists = Arc::new(AtomicUsize::new(0));
        let volumes = Volumes::new(
            Box::new(Fake {
                mounted: Arc::clone(&mounted),
                lists: Arc::clone(&lists),
            }),
            Arc::new(NoWaker),
        );
        wait_until(|| volumes.completed() == 1);
        assert!(matches!(&volumes.state(), VolumesState::Ready(list) if list.len() == 1));
        assert!(volumes.mount("disk"));
        wait_until(|| volumes.completed() >= 2);
        assert_eq!(
            volumes.take_mounted(),
            vec![Ok(PathBuf::from("/mnt/disk"))]
        );
        assert_eq!(
            volumes.take_mounted(),
            Vec::new(),
            "an outcome is handed over once"
        );
        wait_until(|| volumes.completed() >= 3);
        assert!(matches!(&volumes.state(), VolumesState::Ready(list) if list.is_empty()));
        assert_eq!(lists.load(Ordering::Acquire), 2);
    }
}
