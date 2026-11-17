//! Copy, move, rename, delete and link, planned before anything moves and run off the frame.
//!
//! The platform supplies the few primitives that are not `std::fs` — the volume a path is on,
//! the recycle bin, and symbolic links — and this module owns the rest: planning, the
//! conflict question, recursive copies that keep symlinks as links, cancellation between
//! chunks, and the per-item report the view draws.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::jobs::Waker;

/// How much one read or write moves before the loop is allowed to notice a cancel.
const CHUNK: usize = 1 << 20;
/// The shortest gap between two progress events.
const PROGRESS_STEP: Duration = Duration::from_millis(100);
/// Temporary names are tried in order; a thousand is well past any real directory.
const PART_TRIES: u64 = 1000;

/// Names one operation; never reused within a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OperationId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Copy,
    Move,
    Rename,
    Delete(DeleteMode),
    Link(LinkKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteMode {
    /// The platform's recycle bin or trash.
    Trash,
    Permanent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Symbolic,
    Hard,
}

/// What to do when the destination already exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// Suspend the item until the view answers.
    Ask,
    Replace,
    Skip,
    /// Keep both, naming the newcomer `name (2).ext`.
    KeepBoth,
    /// Place the newcomer under this name.
    Rename(String),
}

/// One batch. Copy, Move and Link name a destination folder; Rename names one source and a
/// new name; Delete needs no destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationRequest {
    pub kind: OperationKind,
    pub sources: Vec<PathBuf>,
    pub destination: Option<PathBuf>,
    pub new_name: Option<String>,
    pub policy: ConflictPolicy,
}

impl OperationRequest {
    pub fn copy(sources: Vec<PathBuf>, destination: impl Into<PathBuf>) -> Self {
        Self::to(OperationKind::Copy, sources, destination)
    }

    pub fn move_to(sources: Vec<PathBuf>, destination: impl Into<PathBuf>) -> Self {
        Self::to(OperationKind::Move, sources, destination)
    }

    pub fn link(sources: Vec<PathBuf>, destination: impl Into<PathBuf>, kind: LinkKind) -> Self {
        Self::to(OperationKind::Link(kind), sources, destination)
    }

    pub fn rename(source: impl Into<PathBuf>, new_name: impl Into<String>) -> Self {
        Self {
            kind: OperationKind::Rename,
            sources: vec![source.into()],
            destination: None,
            new_name: Some(new_name.into()),
            policy: ConflictPolicy::Ask,
        }
    }

    pub fn delete(sources: Vec<PathBuf>, mode: DeleteMode) -> Self {
        Self {
            kind: OperationKind::Delete(mode),
            sources,
            destination: None,
            new_name: None,
            policy: ConflictPolicy::Ask,
        }
    }

    fn to(
        kind: OperationKind,
        sources: Vec<PathBuf>,
        destination: impl Into<PathBuf>,
    ) -> Self {
        Self {
            kind,
            sources,
            destination: Some(destination.into()),
            new_name: None,
            policy: ConflictPolicy::Ask,
        }
    }

    /// Set what an existing destination means for the whole batch.
    pub fn with_policy(mut self, policy: ConflictPolicy) -> Self {
        self.policy = policy;
        self
    }
}

/// The view's answer to one conflict, and whether it settles the rest of the batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictDecision {
    pub choice: ConflictPolicy,
    pub apply_to_all: bool,
}

/// A destination the newcomer collided with, as the view needs to draw the question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub id: OperationId,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub source_is_dir: bool,
    pub destination_is_dir: bool,
    /// A free name beside the destination, for the dialog's Keep Both or rename field.
    pub suggested_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationProgress {
    pub id: OperationId,
    pub done_items: u64,
    pub total_items: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// The source item being worked on, for the footer.
    pub current_name: String,
}

/// How one top-level source ended; a destination is absent for a delete or a rename.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub source: PathBuf,
    pub destination: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationReport {
    pub id: OperationId,
    pub kind: OperationKind,
    pub succeeded: Vec<Outcome>,
    pub skipped: Vec<PathBuf>,
    /// Sources not started because the user cancelled.
    pub remaining: Vec<PathBuf>,
    pub cancelled: bool,
    /// One entry per failed source. A failure never aborts the batch.
    pub failed: Vec<(PathBuf, OpError)>,
}

impl OperationReport {
    fn new(id: OperationId, kind: OperationKind) -> Self {
        Self {
            id,
            kind,
            succeeded: Vec::new(),
            skipped: Vec::new(),
            remaining: Vec::new(),
            cancelled: false,
            failed: Vec::new(),
        }
    }
}

/// What a finished job would take to put back, as the undo journal remembers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UndoAction {
    /// Move what landed back into the folders it came from.
    MoveBack {
        /// Each item's new path and the path it had before the move.
        items: Vec<(PathBuf, PathBuf)>,
    },
    /// Give an item the name it had before the rename.
    RenameBack { current: PathBuf, was: String },
    /// Remove what a copy created; the view asks before it does.
    RemoveCopy { created: Vec<PathBuf> },
}

/// Which verb an [`UndoAction`] undoes, for the menu row that offers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndoCause {
    Move,
    Rename,
    Copy,
}

impl UndoAction {
    pub fn cause(&self) -> UndoCause {
        match self {
            Self::MoveBack { .. } => UndoCause::Move,
            Self::RenameBack { .. } => UndoCause::Rename,
            Self::RemoveCopy { .. } => UndoCause::Copy,
        }
    }

    /// The folders undoing this action changes, so a view can re-read them afterwards.
    pub fn touched(&self) -> Vec<PathBuf> {
        let parents = match self {
            Self::MoveBack { items } => items.iter().map(|(_, source)| source).collect(),
            Self::RenameBack { current, .. } => vec![current],
            Self::RemoveCopy { created } => created.iter().collect(),
        };
        parents
            .into_iter()
            .filter_map(|path| path.parent().map(Path::to_path_buf))
            .collect()
    }

    /// The request that puts things back, or `None` when the journal cannot express it.
    ///
    /// A move is undone into the one folder its sources came from; a selection that spanned
    /// folders has no single destination, so it is not journaled rather than half-undone.
    pub fn request(&self) -> Option<OperationRequest> {
        match self {
            Self::MoveBack { items } => {
                let parents: Vec<&Path> = items
                    .iter()
                    .filter_map(|(_, source)| source.parent())
                    .collect();
                let first = *parents.first()?;
                if parents.iter().any(|parent| *parent != first) {
                    return None;
                }
                Some(
                    OperationRequest::move_to(
                        items.iter().map(|(landed, _)| landed.clone()).collect(),
                        first,
                    )
                    .with_policy(ConflictPolicy::Ask),
                )
            }
            Self::RenameBack { current, was } => {
                Some(OperationRequest::rename(current.clone(), was.clone()))
            }
            Self::RemoveCopy { created } => Some(OperationRequest::delete(
                created.clone(),
                DeleteMode::Trash,
            )),
        }
    }
}

/// A stack of inverses, newest last; the picker pops one per Undo.
#[derive(Default)]
pub struct UndoJournal {
    entries: Vec<UndoAction>,
}

/// How many inverses the journal keeps. Beyond Explorer's one, below a session's worth.
const UNDO_DEPTH: usize = 16;

impl UndoJournal {
    /// Remember how `report` could be undone; returns false when nothing is journaled.
    ///
    /// A trash or a permanent delete records nothing, because this engine has no restore:
    /// Windows restores through the shell and a trashed file has no path back.
    pub fn record(&mut self, report: &OperationReport) -> bool {
        let action = match report.kind {
            OperationKind::Move => {
                let items: Vec<(PathBuf, PathBuf)> = report
                    .succeeded
                    .iter()
                    .filter_map(|outcome| {
                        outcome
                            .destination
                            .clone()
                            .map(|landed| (landed, outcome.source.clone()))
                    })
                    .collect();
                (!items.is_empty()).then_some(UndoAction::MoveBack { items })
            }
            OperationKind::Rename => report.succeeded.first().and_then(|outcome| {
                let current = outcome.destination.clone()?;
                let was = outcome.source.file_name()?.to_string_lossy().into_owned();
                Some(UndoAction::RenameBack { current, was })
            }),
            OperationKind::Copy => {
                let created: Vec<PathBuf> = report
                    .succeeded
                    .iter()
                    .filter_map(|outcome| outcome.destination.clone())
                    .collect();
                (!created.is_empty()).then_some(UndoAction::RemoveCopy { created })
            }
            OperationKind::Delete(_) | OperationKind::Link(_) => None,
        };
        let Some(action) = action else {
            return false;
        };
        self.entries.push(action);
        if self.entries.len() > UNDO_DEPTH {
            self.entries.remove(0);
        }
        true
    }

    /// The inverse Undo would run, without taking it.
    pub fn last(&self) -> Option<&UndoAction> {
        self.entries.last()
    }

    pub fn pop(&mut self) -> Option<UndoAction> {
        self.entries.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationEvent {
    Progress(OperationProgress),
    Conflict(Conflict),
    Finished(OperationReport),
}

/// The portable error vocabulary; the view turns each into a localized sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpError {
    NotFound,
    PermissionDenied,
    /// The destination refuses a write, such as a read-only file being overwritten.
    ReadOnly,
    AlreadyExists,
    /// A move whose source already sits in the destination folder.
    AlreadyThere,
    CrossDevice,
    /// A destination inside the source it would copy.
    Recursive,
    /// A special file, a name the platform refuses, or a feature it lacks.
    Unsupported,
    Io(String),
}

impl OpError {
    pub fn from_io(error: &std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            std::io::ErrorKind::CrossesDevices => Self::CrossDevice,
            _ => Self::Io(error.to_string()),
        }
    }
}

impl std::fmt::Display for OpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("not found"),
            Self::PermissionDenied => formatter.write_str("permission denied"),
            Self::ReadOnly => formatter.write_str("read-only"),
            Self::AlreadyExists => formatter.write_str("already exists"),
            Self::AlreadyThere => formatter.write_str("already there"),
            Self::CrossDevice => formatter.write_str("across devices"),
            Self::Recursive => formatter.write_str("a destination inside the source"),
            Self::Unsupported => formatter.write_str("unsupported"),
            Self::Io(reason) => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for OpError {}

/// The platform half: facts about volumes and the operations `std::fs` cannot express.
///
/// Called on the service's worker, where blocking is expected. Every other step — copying,
/// merging, naming, planning — lives here and is shared by every platform.
pub trait FileOperator: Send + Sync {
    /// Whether two paths are on one filesystem, which decides a rename from a copy.
    fn same_volume(&self, a: &Path, b: &Path) -> bool;

    /// Move `path` to the platform's recycle bin or trash.
    fn trash(&self, path: &Path) -> Result<(), OpError>;

    /// Create `link` pointing at `target`; `directory` is the flag Windows needs.
    fn symlink(&self, target: &Path, link: &Path, directory: bool) -> Result<(), OpError>;

    /// Whether this platform can make `kind` links at all.
    fn supports_links(&self, _kind: LinkKind) -> bool {
        true
    }

    /// Refuse a name the platform cannot store: reserved characters, devices, trailing dots.
    fn validate_name(&self, name: &str) -> Result<(), OpError>;

    /// Create a hard link. The default is `std::fs`, which is right on every platform.
    fn hard_link(&self, source: &Path, link: &Path) -> Result<(), OpError> {
        std::fs::hard_link(source, link).map_err(|error| OpError::from_io(&error))
    }
}

/// The app's handle to file operations. Cheap to clone; every clone shares one worker and
/// one queue.
#[derive(Clone)]
pub struct FileOperations {
    shared: Arc<Shared>,
}

struct Shared {
    sender: Sender<Task>,
    waker: Arc<dyn Waker>,
    events: Mutex<Vec<OperationEvent>>,
    /// Operations finished, for the loop's frame forcing.
    completed: AtomicU64,
    /// Events published, so a progress or conflict wakes a frame as well as the wait.
    revision: AtomicU64,
    next_id: AtomicU64,
    /// Started and not yet reported. The exit policy waits for these.
    outstanding: AtomicU64,
    /// One conflict can be pending at a time; the worker runs one batch.
    question: Mutex<Option<(OperationId, Sender<Reply>)>>,
    /// Cancel flags stay reachable so a queued task is stopped before it starts.
    cancels: Mutex<HashMap<OperationId, Arc<AtomicBool>>>,
    operator: Arc<dyn FileOperator>,
}

struct Task {
    id: OperationId,
    request: OperationRequest,
    cancel: Arc<AtomicBool>,
}

enum Reply {
    Decide(ConflictDecision),
    Cancel,
}

impl FileOperations {
    /// Start the worker. `operator` supplies the platform primitives.
    ///
    /// Construction never fails: a worker that could not start drops its receiver, and the
    /// first `start` reports the failure as a finished operation.
    pub fn new(operator: Box<dyn FileOperator>, waker: Arc<dyn Waker>) -> Self {
        let (sender, receiver) = mpsc::channel::<Task>();
        let shared = Arc::new(Shared {
            sender,
            waker,
            events: Mutex::new(Vec::new()),
            completed: AtomicU64::new(0),
            revision: AtomicU64::new(0),
            next_id: AtomicU64::new(0),
            outstanding: AtomicU64::new(0),
            question: Mutex::new(None),
            cancels: Mutex::new(HashMap::new()),
            operator: Arc::from(operator),
        });
        let worker = Arc::clone(&shared);
        if let Err(error) = thread::Builder::new()
            .name("file-operations".into())
            .spawn(move || run(worker, receiver))
        {
            log::warn!("ui-platform: no file-operation worker: {error}");
        }
        Self { shared }
    }

    /// Queue `request`; the returned id names every event it produces.
    pub fn start(&self, request: OperationRequest) -> OperationId {
        let id = OperationId(self.shared.next_id.fetch_add(1, Ordering::Relaxed));
        let cancel = Arc::new(AtomicBool::new(false));
        lock(&self.shared.cancels).insert(id, Arc::clone(&cancel));
        self.shared.outstanding.fetch_add(1, Ordering::Release);
        let task = Task {
            id,
            request,
            cancel,
        };
        if let Err(mpsc::SendError(task)) = self.shared.sender.send(task) {
            let mut report = OperationReport::new(task.id, task.request.kind);
            for source in task.request.sources {
                report
                    .failed
                    .push((source, OpError::Io("no file-operation worker".into())));
            }
            self.shared.finish(report);
        }
        id
    }

    /// Ask a running or queued operation to stop. What has already moved stays moved.
    pub fn cancel(&self, id: OperationId) {
        if let Some(flag) = lock(&self.shared.cancels).get(&id) {
            flag.store(true, Ordering::Release);
        }
        let question = lock(&self.shared.question);
        if let Some((pending, reply)) = question.as_ref()
            && *pending == id
        {
            let _ = reply.send(Reply::Cancel);
        }
    }

    /// Answer the conflict `id` is waiting on. Ignored when that operation is not asking.
    pub fn answer(&self, id: OperationId, decision: ConflictDecision) {
        let question = lock(&self.shared.question);
        if let Some((pending, reply)) = question.as_ref()
            && *pending == id
        {
            let _ = reply.send(Reply::Decide(decision));
        }
    }

    /// Everything that happened since the last call, in order.
    pub fn take(&self) -> Vec<OperationEvent> {
        std::mem::take(&mut *lock(&self.shared.events))
    }

    /// How many operations have finished. The loop forces one frame when this moves.
    pub fn completed(&self) -> u64 {
        self.shared.completed.load(Ordering::Acquire)
    }

    /// Every event, including progress and conflicts, moves this count. The loop compares
    /// it to force a frame for anything the worker publishes.
    pub fn revision(&self) -> u64 {
        self.shared.revision.load(Ordering::Acquire)
    }

    /// Operations still running, which the exit policy must not abandon.
    pub fn outstanding(&self) -> u64 {
        self.shared.outstanding.load(Ordering::Acquire)
    }

    /// The device heuristic the drag menu uses before a request is made.
    pub fn same_volume(&self, a: &Path, b: &Path) -> bool {
        self.shared.operator.same_volume(a, b)
    }

    /// Whether this platform can make `kind` links, for the menu's disabled reason.
    pub fn supports_links(&self, kind: LinkKind) -> bool {
        self.shared.operator.supports_links(kind)
    }

    /// Whether this platform can store `name`, for a rename field's inline check.
    ///
    /// The same rule the worker applies before it starts, asked a frame earlier so the
    /// dialog can underline the field instead of reporting a failed operation.
    pub fn validate_name(&self, name: &str) -> Result<(), OpError> {
        self.shared.operator.validate_name(name)
    }
}

impl Shared {
    fn emit(&self, event: OperationEvent) {
        lock(&self.events).push(event);
        self.revision.fetch_add(1, Ordering::Release);
        self.waker.wake();
    }

    fn finish(&self, report: OperationReport) {
        lock(&self.cancels).remove(&report.id);
        self.outstanding.fetch_sub(1, Ordering::Release);
        self.completed.fetch_add(1, Ordering::Release);
        self.emit(OperationEvent::Finished(report));
    }

    fn cancelled(&self, id: OperationId) -> bool {
        lock(&self.cancels)
            .get(&id)
            .is_none_or(|flag| flag.load(Ordering::Acquire))
    }

    /// Ask the view what to do, and block the worker until it answers or cancels.
    fn ask(&self, conflict: Conflict) -> Option<ConflictDecision> {
        let id = conflict.id;
        let (sender, receiver) = mpsc::channel();
        *lock(&self.question) = Some((id, sender));
        self.emit(OperationEvent::Conflict(conflict));
        loop {
            match receiver.recv_timeout(PROGRESS_STEP) {
                Ok(Reply::Decide(decision)) => {
                    self.clear_question(id);
                    return Some(decision);
                }
                Ok(Reply::Cancel) | Err(RecvTimeoutError::Disconnected) => {
                    self.clear_question(id);
                    return None;
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.cancelled(id) {
                        self.clear_question(id);
                        return None;
                    }
                }
            }
        }
    }

    fn clear_question(&self, id: OperationId) {
        let mut question = lock(&self.question);
        if question.as_ref().is_some_and(|(pending, _)| *pending == id) {
            *question = None;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn run(shared: Arc<Shared>, receiver: Receiver<Task>) {
    let engine = FsOperator {
        platform: Arc::clone(&shared.operator),
    };
    while let Ok(task) = receiver.recv() {
        if shared.cancelled(task.id) {
            let mut report = OperationReport::new(task.id, task.request.kind);
            report.remaining = task.request.sources;
            report.cancelled = true;
            shared.finish(report);
            continue;
        }
        let mut emit = |event| shared.emit(event);
        let mut ask = |conflict| shared.ask(conflict);
        let report = execute(
            &engine,
            task.id,
            task.request,
            &task.cancel,
            &mut emit,
            &mut ask,
        );
        shared.finish(report);
    }
}

/// The portable engine. It owns no thread; the service's worker drives it, and a test can
/// drive it directly on a temporary directory.
pub struct FsOperator {
    platform: Arc<dyn FileOperator>,
}

impl FsOperator {
    pub fn new(platform: Box<dyn FileOperator>) -> Self {
        Self {
            platform: Arc::from(platform),
        }
    }

    pub fn same_volume(&self, a: &Path, b: &Path) -> bool {
        self.platform.same_volume(a, b)
    }

    pub fn supports_links(&self, kind: LinkKind) -> bool {
        self.platform.supports_links(kind)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    File,
    Dir,
    Link,
    Special,
}

impl NodeKind {
    fn of(metadata: &fs::Metadata) -> Self {
        let kind = metadata.file_type();
        if kind.is_symlink() {
            Self::Link
        } else if kind.is_dir() {
            Self::Dir
        } else if kind.is_file() {
            Self::File
        } else {
            Self::Special
        }
    }

    fn of_path(path: &Path) -> Result<Self, OpError> {
        fs::symlink_metadata(path)
            .map(|metadata| Self::of(&metadata))
            .map_err(|error| OpError::from_io(&error))
    }
}

/// How a collision was settled.
enum Place {
    /// The destination is free.
    Fresh(PathBuf),
    /// Remove what is there and put the newcomer in its place.
    Replace(PathBuf),
    /// Both are directories: write into the existing one.
    Merge(PathBuf),
    Skip,
}

/// An internal stop, which is either the user or a real error.
enum Stop {
    Cancelled,
    Failed(OpError),
}

impl From<OpError> for Stop {
    fn from(error: OpError) -> Self {
        Self::Failed(error)
    }
}

impl From<std::io::Error> for Stop {
    fn from(error: std::io::Error) -> Self {
        Self::Failed(OpError::from_io(&error))
    }
}

fn execute(
    operator: &FsOperator,
    id: OperationId,
    request: OperationRequest,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(OperationEvent),
    ask: &mut dyn FnMut(Conflict) -> Option<ConflictDecision>,
) -> OperationReport {
    let total_items = request.sources.len() as u64;
    let mut executor = Executor {
        id,
        operator,
        cancel,
        emit,
        ask,
        policy: request.policy.clone(),
        report: OperationReport::new(id, request.kind),
        done_items: 0,
        total_items,
        bytes_done: 0,
        bytes_total: 0,
        current_name: String::new(),
        last_emit: Instant::now() - PROGRESS_STEP,
    };
    executor.progress(true);
    match request.kind {
        OperationKind::Rename => {
            executor.rename(&request.sources, request.new_name.as_deref());
        }
        OperationKind::Delete(mode) => executor.delete(&request.sources, mode),
        kind => {
            executor.transfer(
                kind,
                &request.sources,
                request.destination.as_deref(),
            );
        }
    }
    executor.report
}

struct Executor<'a> {
    id: OperationId,
    operator: &'a FsOperator,
    cancel: &'a AtomicBool,
    emit: &'a mut dyn FnMut(OperationEvent),
    ask: &'a mut dyn FnMut(Conflict) -> Option<ConflictDecision>,
    policy: ConflictPolicy,
    report: OperationReport,
    done_items: u64,
    total_items: u64,
    bytes_done: u64,
    bytes_total: u64,
    current_name: String,
    last_emit: Instant,
}

impl Executor<'_> {
    fn progress(&mut self, force: bool) {
        if !force && self.last_emit.elapsed() < PROGRESS_STEP {
            return;
        }
        self.last_emit = Instant::now();
        let event = OperationEvent::Progress(OperationProgress {
            id: self.id,
            done_items: self.done_items,
            total_items: self.total_items,
            bytes_done: self.bytes_done,
            bytes_total: self.bytes_total,
            current_name: self.current_name.clone(),
        });
        (self.emit)(event);
    }

    fn stop(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    fn check_stop(&self) -> Result<(), Stop> {
        if self.stop() {
            Err(Stop::Cancelled)
        } else {
            Ok(())
        }
    }

    fn transfer(&mut self, kind: OperationKind, sources: &[PathBuf], destination: Option<&Path>) {
        let Some(destination) = destination else {
            self.fail_all(sources, OpError::Unsupported);
            return;
        };
        if let Err(error) = self.check_destination(destination) {
            self.fail_all(sources, error);
            return;
        }
        for (index, source) in sources.iter().enumerate() {
            if self.stop() {
                self.cancel_rest(&sources[index..]);
                break;
            }
            self.current_name = name_of(source);
            let result = match kind {
                OperationKind::Copy => self.copy_item(source, destination),
                OperationKind::Move => self.move_item(source, destination),
                OperationKind::Link(link) => self.link_item(source, destination, link),
                OperationKind::Rename | OperationKind::Delete(_) => Ok(None),
            };
            match result {
                Ok(Some(destination)) => {
                    self.report.succeeded.push(Outcome {
                        source: source.clone(),
                        destination: Some(destination),
                    });
                    self.done_items += 1;
                    self.progress(true);
                }
                Ok(None) => {
                    self.report.skipped.push(source.clone());
                    self.done_items += 1;
                    self.progress(true);
                }
                Err(Stop::Failed(error)) => {
                    self.report.failed.push((source.clone(), error));
                    self.done_items += 1;
                    self.progress(true);
                }
                Err(Stop::Cancelled) => {
                    // An item cancelled mid-copy is still undone, partial bytes or not.
                    self.cancel_rest(&sources[index..]);
                    break;
                }
            }
        }
    }

    fn fail_all(&mut self, sources: &[PathBuf], error: OpError) {
        for source in sources {
            self.report.failed.push((source.clone(), error.clone()));
            self.done_items += 1;
        }
        self.report.remaining = Vec::new();
        self.progress(true);
    }

    fn cancel_rest(&mut self, sources: &[PathBuf]) {
        self.report.remaining.extend(sources.iter().cloned());
        self.report.cancelled = true;
    }

    fn check_destination(&self, destination: &Path) -> Result<(), OpError> {
        // A symlinked folder is as good a destination as the folder it reaches.
        let metadata = fs::metadata(destination).map_err(|error| OpError::from_io(&error))?;
        if !metadata.is_dir() {
            return Err(OpError::Unsupported);
        }
        if metadata.permissions().readonly() {
            return Err(OpError::PermissionDenied);
        }
        Ok(())
    }

    fn check_recursive(&self, source: &Path, destination: &Path) -> Result<(), OpError> {
        if NodeKind::of_path(source)? != NodeKind::Dir {
            return Ok(());
        }
        let source = fs::canonicalize(source).map_err(|error| OpError::from_io(&error))?;
        let destination =
            fs::canonicalize(destination).map_err(|error| OpError::from_io(&error))?;
        if destination.starts_with(&source) {
            return Err(OpError::Recursive);
        }
        Ok(())
    }

    fn copy_item(&mut self, source: &Path, destination: &Path) -> Result<Option<PathBuf>, Stop> {
        let kind = NodeKind::of_path(source)?;
        if kind == NodeKind::Special {
            return Err(Stop::Failed(OpError::Unsupported));
        }
        self.check_recursive(source, destination)?;
        let Some(mut desired) = join_name(destination, source) else {
            return Err(Stop::Failed(OpError::Unsupported));
        };
        // Copying into the folder the entry already sits in duplicates it, as Explorer
        // does, instead of asking whether to replace the source with itself.
        if same_file(source, &desired) {
            desired = unique_name(&desired);
        }
        self.copy_into(source, &desired)
    }

    fn copy_into(&mut self, source: &Path, desired: &Path) -> Result<Option<PathBuf>, Stop> {
        let kind = NodeKind::of_path(source)?;
        if kind == NodeKind::Special {
            return Err(Stop::Failed(OpError::Unsupported));
        }
        let placement = self.place(kind, source, desired, true)?;
        match placement {
            Place::Skip => Ok(None),
            Place::Fresh(destination) | Place::Merge(destination) => {
                self.copy_at(source, &destination)?;
                Ok(Some(destination))
            }
            Place::Replace(destination) => {
                self.replace(&destination)?;
                self.copy_at(source, &destination)?;
                Ok(Some(destination))
            }
        }
    }

    fn copy_at(&mut self, source: &Path, destination: &Path) -> Result<(), Stop> {
        match NodeKind::of_path(source)? {
            NodeKind::Link => {
                let target = fs::read_link(source)?;
                // The flag describes the target, not the link; Windows needs it to create
                // a directory link, and a broken link is recreated as a file link.
                let directory = fs::metadata(source).is_ok_and(|metadata| metadata.is_dir());
                self.operator
                    .platform
                    .symlink(&target, destination, directory)
                    .map_err(Stop::Failed)
            }
            NodeKind::Dir => {
                fs::create_dir_all(destination)?;
                let entries = fs::read_dir(source)?;
                for entry in entries {
                    self.check_stop()?;
                    let entry = entry?;
                    let child = destination.join(entry.file_name());
                    let _ = self.copy_into(&entry.path(), &child)?;
                }
                // The mode goes on after the children: a read-only source directory must
                // not stop us writing into its copy.
                if let Ok(metadata) = fs::metadata(source) {
                    let _ = fs::set_permissions(destination, metadata.permissions());
                }
                Ok(())
            }
            NodeKind::File => {
                if fs::metadata(destination).is_ok_and(|metadata| metadata.permissions().readonly())
                {
                    return Err(Stop::Failed(OpError::ReadOnly));
                }
                let len = fs::metadata(source).map_or(0, |metadata| metadata.len());
                self.bytes_total += len;
                self.progress(false);
                self.copy_file(source, destination)
            }
            NodeKind::Special => Err(Stop::Failed(OpError::Unsupported)),
        }
    }

    fn copy_file(&mut self, source: &Path, destination: &Path) -> Result<(), Stop> {
        let mut input = File::open(source)?;
        let (part, mut output) = self.create_part(destination)?;
        let mut buffer = vec![0u8; CHUNK];
        let mut failure = None;
        loop {
            if self.stop() {
                failure = Some(Stop::Cancelled);
                break;
            }
            let read = match input.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    failure = Some(Stop::Failed(OpError::from_io(&error)));
                    break;
                }
            };
            if let Err(error) = output.write_all(&buffer[..read]) {
                failure = Some(Stop::Failed(OpError::from_io(&error)));
                break;
            }
            self.bytes_done += read as u64;
            self.progress(false);
        }
        if failure.is_none()
            && let Err(error) = output.sync_all()
        {
            failure = Some(Stop::Failed(OpError::from_io(&error)));
        }
        // The mode and the time are carried where the filesystem permits it; a copy that
        // lands with the default mode is still the copy the user asked for.
        if failure.is_none()
            && let Ok(metadata) = input.metadata()
        {
            let _ = output.set_permissions(metadata.permissions());
            if let Ok(modified) = metadata.modified() {
                let _ = output.set_times(fs::FileTimes::new().set_modified(modified));
            }
        }
        drop(output);
        if let Some(stop) = failure {
            let _ = fs::remove_file(&part);
            return Err(stop);
        }
        if let Err(error) = fs::rename(&part, destination) {
            let _ = fs::remove_file(&part);
            return Err(Stop::Failed(OpError::from_io(&error)));
        }
        Ok(())
    }

    fn create_part(&self, destination: &Path) -> Result<(PathBuf, File), Stop> {
        let parent = destination.parent().unwrap_or_else(|| Path::new(""));
        let name = destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        for attempt in 0..PART_TRIES {
            let candidate = if attempt == 0 {
                parent.join(format!("{name}.part"))
            } else {
                parent.join(format!("{name}.{attempt}.part"))
            };
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => return Ok((candidate, file)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(Stop::Failed(OpError::from_io(&error))),
            }
        }
        Err(Stop::Failed(OpError::Io(
            "no free temporary name".to_owned(),
        )))
    }

    fn move_item(&mut self, source: &Path, destination: &Path) -> Result<Option<PathBuf>, Stop> {
        let kind = NodeKind::of_path(source)?;
        if kind == NodeKind::Special {
            return Err(Stop::Failed(OpError::Unsupported));
        }
        self.check_recursive(source, destination)?;
        let Some(desired) = join_name(destination, source) else {
            return Err(Stop::Failed(OpError::Unsupported));
        };
        if source
            .parent()
            .and_then(|parent| fs::canonicalize(parent).ok())
            .is_some_and(|parent| {
                fs::canonicalize(destination).is_ok_and(|destination| destination == parent)
            })
        {
            return Err(Stop::Failed(OpError::AlreadyThere));
        }
        self.move_into(source, &desired)
    }

    fn move_into(&mut self, source: &Path, desired: &Path) -> Result<Option<PathBuf>, Stop> {
        let kind = NodeKind::of_path(source)?;
        let placement = self.place(kind, source, desired, true)?;
        match placement {
            Place::Skip => Ok(None),
            Place::Merge(destination) => {
                self.merge_move(source, &destination)?;
                Ok(Some(destination))
            }
            Place::Replace(destination) => {
                self.replace(&destination)?;
                self.relocate(source, &destination)?;
                Ok(Some(destination))
            }
            Place::Fresh(destination) => {
                self.relocate(source, &destination)?;
                Ok(Some(destination))
            }
        }
    }

    fn relocate(&mut self, source: &Path, destination: &Path) -> Result<(), Stop> {
        let source_volume = source.parent().unwrap_or(source);
        let destination_volume = destination.parent().unwrap_or(destination);
        if self.operator.same_volume(source_volume, destination_volume) {
            return fs::rename(source, destination).map_err(Into::into);
        }
        self.copy_at(source, destination)?;
        self.remove_any(source).map_err(Stop::Failed)
    }

    /// A directory move onto a directory merges, exactly as a copy does; the source goes
    /// only if the merge emptied it.
    fn merge_move(&mut self, source: &Path, destination: &Path) -> Result<(), Stop> {
        let entries = fs::read_dir(source)?;
        for entry in entries {
            self.check_stop()?;
            let entry = entry?;
            let child = destination.join(entry.file_name());
            let _ = self.move_into(&entry.path(), &child)?;
        }
        match fs::remove_dir(source) {
            Ok(()) => Ok(()),
            // A skipped child keeps the source directory alive; that is the merge's answer,
            // not a failure of the move.
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
            Err(error) => Err(Stop::Failed(OpError::from_io(&error))),
        }
    }

    fn link_item(
        &mut self,
        source: &Path,
        destination: &Path,
        kind: LinkKind,
    ) -> Result<Option<PathBuf>, Stop> {
        let node = NodeKind::of_path(source)?;
        if node == NodeKind::Special {
            return Err(Stop::Failed(OpError::Unsupported));
        }
        if !self.operator.supports_links(kind) {
            return Err(Stop::Failed(OpError::Unsupported));
        }
        if kind == LinkKind::Hard && node == NodeKind::Dir {
            return Err(Stop::Failed(OpError::Unsupported));
        }
        if kind == LinkKind::Hard && !self.operator.same_volume(source, destination) {
            return Err(Stop::Failed(OpError::CrossDevice));
        }
        let Some(desired) = join_name(destination, source) else {
            return Err(Stop::Failed(OpError::Unsupported));
        };
        let placement = self.place(node, source, &desired, false)?;
        let target = match placement {
            Place::Skip => return Ok(None),
            Place::Replace(target) => {
                self.replace(&target)?;
                target
            }
            Place::Fresh(target) => target,
            Place::Merge(target) => target,
        };
        match kind {
            LinkKind::Hard => self
                .operator
                .platform
                .hard_link(source, &target)
                .map_err(Stop::Failed)?,
            LinkKind::Symbolic => {
                let absolute = std::path::absolute(source)?;
                let directory = fs::metadata(source).is_ok_and(|metadata| metadata.is_dir());
                self.operator
                    .platform
                    .symlink(&absolute, &target, directory)
                    .map_err(Stop::Failed)?;
            }
        }
        Ok(Some(target))
    }

    fn rename(&mut self, sources: &[PathBuf], new_name: Option<&str>) {
        let Some(source) = sources.first() else {
            return;
        };
        for extra in &sources[1..] {
            self.report
                .failed
                .push((extra.clone(), OpError::Unsupported));
        }
        self.current_name = name_of(source);
        let result = match new_name {
            Some(new_name) => self.rename_one(source, new_name),
            None => Err(Stop::Failed(OpError::Unsupported)),
        };
        match result {
            Ok(()) => {
                let destination = source
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(new_name.unwrap_or_default());
                self.report.succeeded.push(Outcome {
                    source: source.clone(),
                    destination: Some(destination),
                });
            }
            Err(Stop::Cancelled) => self.report.remaining.push(source.clone()),
            Err(Stop::Failed(error)) => self.report.failed.push((source.clone(), error)),
        }
        self.done_items = self.total_items;
        self.progress(true);
    }

    fn rename_one(&mut self, source: &Path, new_name: &str) -> Result<(), Stop> {
        self.operator
            .platform
            .validate_name(new_name)
            .map_err(Stop::Failed)?;
        check_name(new_name)?;
        let parent = source.parent().unwrap_or_else(|| Path::new(""));
        let destination = parent.join(new_name);
        if source == destination {
            return Err(Stop::Failed(OpError::AlreadyThere));
        }
        if case_only_name(source, &destination) && same_file(source, &destination) {
            // A case-only rename on a case-insensitive filesystem: the destination names
            // the same entry, so the move goes through a name neither of them has.
            let temporary = unique_name(&parent.join(format!(".{new_name}.rename")));
            fs::rename(source, &temporary)?;
            if let Err(error) = fs::rename(&temporary, &destination) {
                let _ = fs::rename(&temporary, source);
                return Err(Stop::Failed(OpError::from_io(&error)));
            }
            return Ok(());
        }
        let kind = NodeKind::of_path(source)?;
        let placement = self.place(kind, source, &destination, false)?;
        match placement {
            Place::Skip => {
                self.report.skipped.push(source.to_owned());
                Ok(())
            }
            Place::Replace(target) => {
                self.replace(&target)?;
                fs::rename(source, &target).map_err(Into::into)
            }
            Place::Fresh(target) | Place::Merge(target) => {
                fs::rename(source, &target).map_err(Into::into)
            }
        }
    }

    fn delete(&mut self, sources: &[PathBuf], mode: DeleteMode) {
        for (index, source) in sources.iter().enumerate() {
            if self.stop() {
                self.cancel_rest(&sources[index..]);
                break;
            }
            self.current_name = name_of(source);
            let result = match mode {
                DeleteMode::Trash => self.operator.platform.trash(source),
                DeleteMode::Permanent => self.remove_any(source),
            };
            match result {
                Ok(()) => self.report.succeeded.push(Outcome {
                    source: source.clone(),
                    destination: None,
                }),
                Err(error) => self.report.failed.push((source.clone(), error)),
            }
            self.done_items += 1;
            self.progress(true);
        }
    }

    /// Settle a collision. Both directories merge without a question, which is the model
    /// every file manager uses; anything else follows the policy or asks.
    fn place(
        &mut self,
        source_kind: NodeKind,
        source: &Path,
        desired: &Path,
        merge_dirs: bool,
    ) -> Result<Place, Stop> {
        let destination_kind = match fs::symlink_metadata(desired) {
            Ok(metadata) => NodeKind::of(&metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Place::Fresh(desired.to_owned()));
            }
            Err(error) => return Err(Stop::Failed(OpError::from_io(&error))),
        };
        if merge_dirs && source_kind == NodeKind::Dir && destination_kind == NodeKind::Dir {
            return Ok(Place::Merge(desired.to_owned()));
        }
        let mut choice = match &self.policy {
            ConflictPolicy::Ask => {
                let conflict = Conflict {
                    id: self.id,
                    source: source.to_owned(),
                    destination: desired.to_owned(),
                    source_is_dir: source_kind == NodeKind::Dir,
                    destination_is_dir: destination_kind == NodeKind::Dir,
                    suggested_name: unique_name(desired)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                };
                let Some(decision) = (self.ask)(conflict) else {
                    return Err(Stop::Cancelled);
                };
                if decision.apply_to_all && !matches!(decision.choice, ConflictPolicy::Rename(_)) {
                    self.policy = decision.choice.clone();
                }
                decision.choice
            }
            policy => policy.clone(),
        };
        // Replacing the source with itself would delete the only copy.
        if same_file(source, desired) && matches!(choice, ConflictPolicy::Replace) {
            choice = ConflictPolicy::KeepBoth;
        }
        match choice {
            ConflictPolicy::Skip => Ok(Place::Skip),
            ConflictPolicy::Replace => Ok(Place::Replace(desired.to_owned())),
            ConflictPolicy::KeepBoth => Ok(Place::Fresh(unique_name(desired))),
            ConflictPolicy::Rename(name) => {
                self.operator
                    .platform
                    .validate_name(&name)
                    .map_err(Stop::Failed)?;
                check_name(&name)?;
                let target = desired
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(name);
                if fs::symlink_metadata(&target).is_ok() {
                    Err(Stop::Failed(OpError::AlreadyExists))
                } else {
                    Ok(Place::Fresh(target))
                }
            }
            // An answer never asks again; skipping is the safe reading of a stray one.
            ConflictPolicy::Ask => Ok(Place::Skip),
        }
    }

    /// Remove a destination an explicit Replace chose. A read-only destination is its own
    /// error, not a permission failure, so the view can say what actually blocked it.
    fn replace(&self, path: &Path) -> Result<(), Stop> {
        if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.permissions().readonly()) {
            return Err(Stop::Failed(OpError::ReadOnly));
        }
        self.remove_any(path).map_err(Stop::Failed)
    }

    fn remove_any(&self, path: &Path) -> Result<(), OpError> {
        let metadata = fs::symlink_metadata(path).map_err(|error| OpError::from_io(&error))?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(path).map_err(|error| OpError::from_io(&error))
        } else {
            fs::remove_file(path).map_err(|error| OpError::from_io(&error))
        }
    }
}

fn join_name(destination: &Path, source: &Path) -> Option<PathBuf> {
    source.file_name().map(|name| destination.join(name))
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Whether two paths name the same entry apart from the case of the last component. A
/// symlink and another name that reach the same target are two entries, so they fail this.
fn case_only_name(a: &Path, b: &Path) -> bool {
    match (a.file_name(), b.file_name()) {
        (Some(a), Some(b)) => {
            a != b && a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
        }
        _ => false,
    }
}

/// `path` itself when nothing holds it, else the first free `name (2).ext` beside it.
///
/// A copy needs a name that is new by definition; a folder the user asked to create keeps
/// the plain name it was given until something else already owns it.
pub fn available_name(path: &Path) -> PathBuf {
    if fs::symlink_metadata(path).is_err() {
        path.to_path_buf()
    } else {
        unique_name(path)
    }
}

/// The first free `name (2).ext` beside `path`; the same rule serves Keep Both and a
/// same-folder copy.
pub fn unique_name(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (base, current) = match split_counter(&stem) {
        Some((base, number)) => (base, number),
        None => (stem, 1),
    };
    let extension = path.extension().map(|ext| ext.to_string_lossy().into_owned());
    let mut number = current + 1;
    loop {
        let name = match &extension {
            Some(extension) => format!("{base} ({number}).{extension}"),
            None => format!("{base} ({number})"),
        };
        let candidate = parent.join(name);
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
        number += 1;
    }
}

/// Split `name (2)` into `name` and `2`, so a second copy continues the count.
fn split_counter(stem: &str) -> Option<(String, u64)> {
    let inner = stem.strip_suffix(')')?;
    let (base, number) = inner.rsplit_once(" (")?;
    let number = number.parse::<u64>().ok()?;
    Some((base.to_owned(), number))
}

fn check_name(name: &str) -> Result<(), Stop> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        Err(Stop::Failed(OpError::Unsupported))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct TestOperator {
        same_volume: bool,
    }

    impl TestOperator {
        fn of(same_volume: bool) -> FsOperator {
            FsOperator::new(Box::new(Self { same_volume }))
        }
    }

    impl FileOperator for TestOperator {
        fn same_volume(&self, _: &Path, _: &Path) -> bool {
            self.same_volume
        }

        fn trash(&self, path: &Path) -> Result<(), OpError> {
            let metadata = fs::symlink_metadata(path).map_err(|error| OpError::from_io(&error))?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                fs::remove_dir_all(path).map_err(|error| OpError::from_io(&error))
            } else {
                fs::remove_file(path).map_err(|error| OpError::from_io(&error))
            }
        }

        fn symlink(&self, _: &Path, _: &Path, _: bool) -> Result<(), OpError> {
            Err(OpError::Unsupported)
        }

        fn supports_links(&self, kind: LinkKind) -> bool {
            kind == LinkKind::Hard
        }

        fn validate_name(&self, name: &str) -> Result<(), OpError> {
            if name.is_empty()
                || name.contains('/')
                || name.contains('\\')
                || name.contains('\0')
            {
                Err(OpError::Unsupported)
            } else {
                Ok(())
            }
        }
    }

    fn run(
        operator: &FsOperator,
        request: OperationRequest,
        decisions: Vec<ConflictDecision>,
    ) -> OperationReport {
        let cancel = AtomicBool::new(false);
        let mut decisions = VecDeque::from(decisions);
        let mut events = Vec::new();
        let mut emit = |event| events.push(event);
        let mut ask = |_| decisions.pop_front();
        execute(
            operator,
            OperationId(0),
            request,
            &cancel,
            &mut emit,
            &mut ask,
        )
    }

    fn read(path: &Path) -> Vec<u8> {
        std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).expect("the test file is writable");
    }

    /// A requested name is kept while it is free and steps aside only when it is taken:
    /// a copy needs a new name by definition, a folder the user creates does not.
    #[test]
    fn a_free_name_is_kept_and_a_taken_one_steps_aside() {
        let temp = tempfile::tempdir().expect("a temporary directory");
        let wanted = temp.path().join("New Folder");
        assert_eq!(available_name(&wanted), wanted);
        std::fs::create_dir(&wanted).expect("the first folder");
        assert_eq!(available_name(&wanted), temp.path().join("New Folder (2)"));
    }

    /// A directory copy merges into an existing destination; a file inside it that collides
    /// keeps both under the numbered name the conflict rule derives.
    #[test]
    fn copying_a_directory_merges_and_a_collision_keeps_both() {
        let temp = tempfile::tempdir().expect("a temporary directory");
        let source = temp.path().join("tree");
        std::fs::create_dir_all(source.join("sub")).expect("the source tree");
        write(&source.join("a.txt"), b"a");
        write(&source.join("sub/b.txt"), b"b");
        let destination = temp.path().join("dest");
        std::fs::create_dir_all(&destination).expect("the destination");

        let operator = TestOperator::of(true);
        let report = run(
            &operator,
            OperationRequest::copy(vec![source.clone()], destination.clone()),
            Vec::new(),
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(read(&destination.join("tree/a.txt")), b"a");
        assert_eq!(read(&destination.join("tree/sub/b.txt")), b"b");

        write(&source.join("a.txt"), b"second");
        let report = run(
            &operator,
            OperationRequest::copy(vec![source], destination.clone()),
            vec![ConflictDecision {
                choice: ConflictPolicy::KeepBoth,
                apply_to_all: false,
            }],
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(read(&destination.join("tree/a.txt")), b"a");
        assert_eq!(read(&destination.join("tree/a (2).txt")), b"second");
    }

    /// Across devices a move is a copy followed by a removal, and the source is gone only
    /// after the copy finished.
    #[test]
    fn a_cross_device_move_copies_then_removes_the_source() {
        let temp = tempfile::tempdir().expect("a temporary directory");
        let source = temp.path().join("tree");
        std::fs::create_dir_all(source.join("sub")).expect("the source tree");
        write(&source.join("sub/b.txt"), b"b");
        let destination = temp.path().join("dest");
        std::fs::create_dir_all(&destination).expect("the destination");

        let report = run(
            &TestOperator::of(false),
            OperationRequest::move_to(vec![source.clone()], destination.clone()),
            Vec::new(),
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert!(!source.exists(), "the source is removed after the copy");
        assert_eq!(read(&destination.join("tree/sub/b.txt")), b"b");
    }

    /// The conflict question is per item: Skip leaves the destination, Replace overwrites it.
    #[test]
    fn a_conflict_answer_decides_the_item() {
        let temp = tempfile::tempdir().expect("a temporary directory");
        let source = temp.path().join("a.txt");
        write(&source, b"new");
        let destination = temp.path().join("dest");
        std::fs::create_dir_all(&destination).expect("the destination");
        write(&destination.join("a.txt"), b"old");
        let operator = TestOperator::of(true);

        let report = run(
            &operator,
            OperationRequest::copy(vec![source.clone()], destination.clone()),
            vec![ConflictDecision {
                choice: ConflictPolicy::Skip,
                apply_to_all: false,
            }],
        );
        assert_eq!(read(&destination.join("a.txt")), b"old");
        assert_eq!(report.skipped.as_slice(), std::slice::from_ref(&source));

        let report = run(
            &operator,
            OperationRequest::copy(vec![source], destination.clone()),
            vec![ConflictDecision {
                choice: ConflictPolicy::Replace,
                apply_to_all: false,
            }],
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(read(&destination.join("a.txt")), b"new");
    }

    /// Copying a directory into its own subtree would grow while it copies, so it is refused
    /// before the first byte moves.
    #[test]
    fn a_destination_inside_the_source_is_refused() {
        let temp = tempfile::tempdir().expect("a temporary directory");
        let source = temp.path().join("tree");
        std::fs::create_dir_all(source.join("inner")).expect("the source tree");
        let report = run(
            &TestOperator::of(true),
            OperationRequest::copy(vec![source.clone()], source.join("inner")),
            Vec::new(),
        );
        assert_eq!(report.failed, [(source, OpError::Recursive)]);
    }

    /// A hard link cannot cross a device, and the plan says so instead of failing in the OS.
    #[test]
    fn a_hard_link_across_devices_is_refused() {
        let temp = tempfile::tempdir().expect("a temporary directory");
        let source = temp.path().join("a.txt");
        write(&source, b"a");
        let destination = temp.path().join("dest");
        std::fs::create_dir_all(&destination).expect("the destination");
        let report = run(
            &TestOperator::of(false),
            OperationRequest::link(vec![source.clone()], destination, LinkKind::Hard),
            Vec::new(),
        );
        assert_eq!(report.failed, [(source, OpError::CrossDevice)]);
    }

    /// Copying into the folder the file already sits in duplicates it, as Explorer does,
    /// even under a Replace policy that would otherwise overwrite it with itself.
    #[test]
    fn a_same_folder_copy_keeps_both() {
        let temp = tempfile::tempdir().expect("a temporary directory");
        let source = temp.path().join("a.txt");
        write(&source, b"a");
        let report = run(
            &TestOperator::of(true),
            OperationRequest::copy(vec![source.clone()], temp.path().to_owned())
                .with_policy(ConflictPolicy::Replace),
            Vec::new(),
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(read(&temp.path().join("a (2).txt")), b"a");
        assert_eq!(read(&source), b"a");
    }

    struct TestWaker;

    impl crate::jobs::Waker for TestWaker {
        fn wake(&self) {}
    }

    /// The worker asks through its event queue and takes the answer on another thread: the
    /// question must not be waiting behind the task that asked it.
    #[test]
    fn the_worker_delivers_a_conflict_and_accepts_the_answer() {
        let temp = tempfile::tempdir().expect("a temporary directory");
        let source = temp.path().join("a.txt");
        write(&source, b"new");
        let destination = temp.path().join("dest");
        std::fs::create_dir_all(&destination).expect("the destination");
        write(&destination.join("a.txt"), b"old");

        let operations = FileOperations::new(
            Box::new(TestOperator { same_volume: true }),
            Arc::new(TestWaker),
        );
        let id = operations.start(OperationRequest::copy(vec![source], destination.clone()));

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut events = Vec::new();
        while !events
            .iter()
            .any(|event| matches!(event, OperationEvent::Conflict(_)))
        {
            events.extend(operations.take());
            assert!(Instant::now() < deadline, "the conflict arrived");
            std::thread::sleep(Duration::from_millis(1));
        }
        let conflict = events
            .iter()
            .find_map(|event| match event {
                OperationEvent::Conflict(conflict) => Some(conflict.clone()),
                _ => None,
            })
            .expect("the conflict");
        assert_eq!(conflict.id, id);
        assert!(conflict.suggested_name.starts_with("a ("));

        operations.answer(
            id,
            ConflictDecision {
                choice: ConflictPolicy::KeepBoth,
                apply_to_all: false,
            },
        );
        let mut finished = None;
        while finished.is_none() {
            for event in operations.take() {
                if let OperationEvent::Finished(report) = event {
                    finished = Some(report);
                }
            }
            assert!(Instant::now() < deadline, "the operation finished");
            std::thread::sleep(Duration::from_millis(1));
        }
        let report = finished.expect("the report");
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(read(&destination.join("a (2).txt")), b"new");
        assert_eq!(operations.outstanding(), 0);
        assert_eq!(operations.completed(), 1);
    }

    fn report_of(kind: OperationKind, outcomes: &[(&str, Option<&str>)]) -> OperationReport {
        OperationReport {
            id: OperationId(0),
            kind,
            succeeded: outcomes
                .iter()
                .map(|(source, destination)| Outcome {
                    source: PathBuf::from(source),
                    destination: destination.map(PathBuf::from),
                })
                .collect(),
            skipped: Vec::new(),
            remaining: Vec::new(),
            cancelled: false,
            failed: Vec::new(),
        }
    }

    /// Undo of a move sends every item that landed back to the folder it came from, and
    /// nothing else: an item that failed is not in the journal, so Undo cannot invent it.
    #[test]
    fn undo_of_a_move_puts_back_what_moved() {
        let mut journal = UndoJournal::default();
        let mut report = report_of(
            OperationKind::Move,
            &[
                ("/from/one", Some("/to/one")),
                ("/from/two", Some("/to/two")),
            ],
        );
        report.failed.push((PathBuf::from("/from/three"), OpError::NotFound));
        assert!(journal.record(&report));

        let action = journal.pop().expect("the inverse");
        assert_eq!(action.cause(), UndoCause::Move);
        let request = action.request().expect("one folder to move into");
        assert_eq!(request.kind, OperationKind::Move);
        assert_eq!(
            request.sources,
            [PathBuf::from("/to/one"), PathBuf::from("/to/two")]
        );
        assert_eq!(request.destination, Some(PathBuf::from("/from")));
        assert_eq!(request.policy, ConflictPolicy::Ask);
        assert!(journal.is_empty(), "a popped inverse is spent");
    }

    /// A rename's inverse names the file's old name; a copy's inverse trashes what the copy
    /// created, and a delete has no inverse here because the engine cannot restore one.
    #[test]
    fn a_rename_and_a_copy_are_undoable_but_a_delete_is_not() {
        let mut journal = UndoJournal::default();
        assert!(journal.record(&report_of(
            OperationKind::Rename,
            &[("/dir/old.txt", Some("/dir/new.txt"))],
        )));
        let action = journal.pop().expect("the inverse");
        assert_eq!(action.cause(), UndoCause::Rename);
        let request = action.request().expect("a rename request");
        assert_eq!(request.sources, [PathBuf::from("/dir/new.txt")]);
        assert_eq!(request.new_name.as_deref(), Some("old.txt"));

        assert!(journal.record(&report_of(
            OperationKind::Copy,
            &[("/a/x", Some("/b/x")), ("/a/y", Some("/b/y"))],
        )));
        let action = journal.pop().expect("the inverse");
        assert_eq!(action.cause(), UndoCause::Copy);
        let request = action.request().expect("a delete request");
        assert_eq!(request.kind, OperationKind::Delete(DeleteMode::Trash));
        assert_eq!(request.sources, [PathBuf::from("/b/x"), PathBuf::from("/b/y")]);

        assert!(!journal.record(&report_of(
            OperationKind::Delete(DeleteMode::Trash),
            &[("/a/x", None)],
        )));
        assert!(journal.is_empty(), "a trash records no inverse");
    }
}
