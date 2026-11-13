//! Work that must not run inside a frame.
//!
//! One worker takes jobs in order; a result waits in a slot the app polls, and a
//! completion wakes the loop so that poll happens promptly.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/// Wakes the loop from another thread. The platform implements it; portable code only
/// calls it.
pub trait Waker: Send + Sync {
    fn wake(&self);
}

type Task = Box<dyn FnOnce() + Send + 'static>;

/// The process's worker, shared by every part of the app that has background work.
#[derive(Clone)]
pub struct Jobs {
    sender: Sender<Task>,
    waker: Arc<dyn Waker>,
    /// Jobs handed to the worker.
    submitted: Arc<AtomicU64>,
    /// Completions so far; the loop compares it to know a result is waiting.
    completed: Arc<AtomicU64>,
    /// Why the worker could not start, when it did not.
    failure: Option<Arc<str>>,
}

/// The result of one submitted job.
pub struct Job<T> {
    slot: Arc<Mutex<Option<T>>>,
}

impl<T> Job<T> {
    /// Take the result once the worker has finished; the slot is empty afterwards.
    pub fn take(&self) -> Option<T> {
        match self.slot.lock() {
            Ok(mut slot) => slot.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        }
    }
}

impl Jobs {
    /// Start the worker. `waker` is what lets a completion interrupt the loop's wait.
    pub fn new(waker: Arc<dyn Waker>) -> Self {
        let (sender, receiver) = mpsc::channel::<Task>();
        let failure = match thread::Builder::new()
            .name("jobs".into())
            .spawn(move || run(receiver))
        {
            Ok(_) => None,
            Err(error) => Some(Arc::from(format!("job worker: {error}"))),
        };
        Self {
            sender,
            waker,
            submitted: Arc::new(AtomicU64::new(0)),
            completed: Arc::new(AtomicU64::new(0)),
            failure,
        }
    }

    /// Queue `work`; the returned handle carries its result.
    pub fn submit<T, F>(&self, work: F) -> Result<Job<T>, String>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let slot = Arc::new(Mutex::new(None));
        let out = Arc::clone(&slot);
        let completed = Arc::clone(&self.completed);
        let waker = Arc::clone(&self.waker);
        self.submitted.fetch_add(1, Ordering::Release);
        let task: Task = Box::new(move || {
            let _finish = Finish { completed, waker };
            let value = work();
            match out.lock() {
                Ok(mut slot) => *slot = Some(value),
                Err(poisoned) => *poisoned.into_inner() = Some(value),
            }
        });
        self.sender.send(task).map_err(|_| {
            self.submitted.fetch_sub(1, Ordering::Release);
            match &self.failure {
                Some(failure) => failure.to_string(),
                None => "the job worker stopped".to_string(),
            }
        })?;
        Ok(Job { slot })
    }

    /// How many jobs have finished. The loop forces one frame when this moves.
    pub fn completed(&self) -> u64 {
        self.completed.load(Ordering::Acquire)
    }

    /// Jobs still running. The loop does not exit while this is non-zero.
    pub fn outstanding(&self) -> u64 {
        self.submitted
            .load(Ordering::Acquire)
            .saturating_sub(self.completed.load(Ordering::Acquire))
    }
}

/// Counts a job as finished on the way out, including when it panicked: a loop waiting
/// to exit must not hang on a dead job.
struct Finish {
    completed: Arc<AtomicU64>,
    waker: Arc<dyn Waker>,
}

impl Drop for Finish {
    fn drop(&mut self) {
        self.completed.fetch_add(1, Ordering::Release);
        self.waker.wake();
    }
}

fn run(receiver: Receiver<Task>) {
    while let Ok(task) = receiver.recv() {
        task();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    struct CountingWaker(AtomicUsize);

    impl Waker for CountingWaker {
        fn wake(&self) {
            self.0.fetch_add(1, Ordering::Release);
        }
    }

    /// A job runs off the caller's thread, wakes the loop once, and yields its result
    /// exactly once.
    #[test]
    fn a_job_runs_off_thread_wakes_once_and_yields_one_result() {
        let waker = Arc::new(CountingWaker(AtomicUsize::new(0)));
        let jobs = Jobs::new(waker.clone());
        let caller = std::thread::current().id();
        let (started, started_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel::<()>();

        let job = jobs
            .submit(move || {
                started.send(()).expect("the test is listening");
                release_rx.recv().expect("the test releases the job");
                std::thread::current().id()
            })
            .expect("submit");
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the job started");
        assert_eq!(jobs.outstanding(), 1, "a running job is outstanding");
        release.send(()).expect("the job is still waiting");

        // The wake happens after the result is stored; poll briefly for it.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while waker.0.load(Ordering::Acquire) == 0 && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(waker.0.load(Ordering::Acquire), 1, "one wake per job");
        let worker = job.take().expect("the result is waiting");
        assert_ne!(worker, caller, "the job must not run on the loop thread");
        assert_eq!(job.take(), None, "the slot hands the result out once");
        assert_eq!(jobs.completed(), 1);
        assert_eq!(jobs.outstanding(), 0);
    }
}
