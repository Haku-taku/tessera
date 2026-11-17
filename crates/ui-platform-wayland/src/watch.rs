//! A directory watch on Linux: one thread per directory over inotify.
//!
//! A burst — copying a folder is hundreds of events — is allowed to go quiet before the
//! picker is told, so one copy is one listing read instead of one per file.

use std::mem::MaybeUninit;
use std::path::Path;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fd::OwnedFd;
use rustix::fs::inotify::{self, CreateFlags, Reader, WatchFlags};
use rustix::io::Errno;
use rustix::pipe::pipe;
use ui_platform::watch::{DirectoryWatcher, WatchGuard};

/// How long the events must be quiet before the change is reported.
const QUIET_MS: i32 = 250;

/// The Linux watcher: every `watch` starts a thread of its own.
pub struct InotifyWatcher;

impl DirectoryWatcher for InotifyWatcher {
    fn watch(
        &self,
        path: &Path,
        changed: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Box<dyn WatchGuard>, String> {
        let (read, write) = pipe().map_err(|error| format!("watch pipe: {error}"))?;
        let target = path.to_path_buf();
        let worker = thread::Builder::new()
            .name("directory-watch".into())
            .spawn(move || run(&target, read, changed));
        match worker {
            Ok(worker) => Ok(Box::new(InotifyWatch {
                stop: write,
                worker: Some(worker),
            })),
            Err(error) => Err(format!("watch thread: {error}")),
        }
    }
}

struct InotifyWatch {
    /// The write end of the stop pipe; a byte wakes the watcher's poll.
    stop: OwnedFd,
    worker: Option<JoinHandle<()>>,
}

impl WatchGuard for InotifyWatch {}

impl Drop for InotifyWatch {
    fn drop(&mut self) {
        let _ = rustix::io::write(&self.stop, &[0]);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(path: &Path, stop: OwnedFd, changed: Arc<dyn Fn() + Send + Sync>) {
    let inotify = match inotify::init(CreateFlags::NONBLOCK | CreateFlags::CLOEXEC) {
        Ok(fd) => fd,
        Err(error) => {
            log::warn!("ui-platform-wayland: no inotify: {error}");
            return;
        }
    };
    let flags = WatchFlags::CREATE
        | WatchFlags::DELETE
        | WatchFlags::MODIFY
        | WatchFlags::ATTRIB
        | WatchFlags::CLOSE_WRITE
        | WatchFlags::MOVED_FROM
        | WatchFlags::MOVED_TO
        | WatchFlags::DELETE_SELF
        | WatchFlags::MOVE_SELF
        | WatchFlags::ONLYDIR;
    // rustix takes a path as `Arg`; the bytes are what the kernel names the directory by.
    if let Err(error) = inotify::add_watch(&inotify, path, flags) {
        log::warn!(
            "ui-platform-wayland: cannot watch {}: {error}",
            path.display()
        );
        return;
    }
    loop {
        let ready = match wait(&inotify, &stop, None) {
            Ok(ready) => ready,
            Err(error) => {
                log::debug!("ui-platform-wayland: watch wait failed: {error}");
                return;
            }
        };
        if ready.stop {
            return;
        }
        if !ready.changed {
            continue;
        }
        drain(&inotify);
        // A save writes several events; report once the directory has been quiet.
        loop {
            let ready = match wait(&inotify, &stop, Some(QUIET_MS)) {
                Ok(ready) => ready,
                Err(error) => {
                    log::debug!("ui-platform-wayland: watch wait failed: {error}");
                    changed();
                    return;
                }
            };
            if ready.stop {
                return;
            }
            if !ready.changed {
                changed();
                break;
            }
            drain(&inotify);
        }
    }
}

struct Ready {
    changed: bool,
    stop: bool,
}

fn wait(inotify: &OwnedFd, stop: &OwnedFd, timeout: Option<i32>) -> Result<Ready, Errno> {
    let mut fds = [
        PollFd::new(inotify, PollFlags::IN),
        PollFd::new(stop, PollFlags::IN),
    ];
    let deadline = timeout.map(|millis| Timespec {
        tv_sec: (millis / 1000) as i64,
        tv_nsec: ((millis % 1000) as i64) * 1_000_000,
    });
    let ready = poll(&mut fds, deadline.as_ref())?;
    if ready == 0 {
        return Ok(Ready {
            changed: false,
            stop: false,
        });
    }
    Ok(Ready {
        changed: fds[0].revents().intersects(PollFlags::IN),
        stop: fds[1]
            .revents()
            .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR),
    })
}

/// Read and drop what is pending, so the next wait starts from an empty queue.
fn drain(inotify: &OwnedFd) {
    let mut buffer = [MaybeUninit::<u8>::uninit(); 4096];
    let mut reader = Reader::new(inotify, &mut buffer);
    loop {
        match reader.next() {
            Ok(_) => {}
            Err(Errno::AGAIN) => return,
            Err(error) => {
                log::debug!("ui-platform-wayland: watch read failed: {error}");
                return;
            }
        }
    }
}
