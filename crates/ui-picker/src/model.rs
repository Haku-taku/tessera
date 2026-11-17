//! The picker's state: one directory, its rows, a selection, and where it came from.
//!
//! Nothing here draws. The listing service answers what a directory holds; the model sorts
//! and filters it, keeps the previous rows while the next directory is in flight, and drops
//! an answer for a directory the user has already left.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ui_core::Modifiers;
use ui_platform::chooser::{FileTypeFilter, OverwritePolicy};
use ui_platform::listing::{Entry, ListingRequest, ListingState, Listings};
use ui_platform::mime_type;
use ui_platform::{KeepAlive, ThumbnailRequest};

/// The contract's mode, under the name the drawn picker has always used.
pub use ui_platform::chooser::ChooserMode as Mode;
/// The contract's filter, re-exported so a caller configures one dialog's filters with the
/// same type whichever implementation ends up showing it.
pub use ui_platform::chooser::FileTypeFilter as FileFilter;

/// Whether a row may ask for a thumbnail, and the largest file it asks about.
///
/// Network mounts are not part of the prototype, so `local_only` is not here yet: there is
/// no mount table to read a filesystem from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThumbnailPolicy {
    pub show: bool,
    pub max_file_size: u64,
}

impl Default for ThumbnailPolicy {
    fn default() -> Self {
        Self {
            show: true,
            max_file_size: 64 << 20,
        }
    }
}

/// The listing service as the model sees it.
///
/// A trait so the model can be driven from a table in a test, without a worker thread.
pub trait ListingSource {
    fn get(&self, request: &ListingRequest) -> ListingState;
    fn invalidate(&self, path: &Path);

    /// A claim on the source's own cache, so a dialog left open does not lose its listing
    /// to the idle window. A source that keeps nothing in memory returns `None`.
    fn keep_alive(&self) -> Option<KeepAlive> {
        None
    }
}

impl ListingSource for Listings {
    fn get(&self, request: &ListingRequest) -> ListingState {
        Listings::get(self, request)
    }

    fn invalidate(&self, path: &Path) {
        Listings::invalidate(self, path);
    }

    fn keep_alive(&self) -> Option<KeepAlive> {
        Some(Listings::keep_alive(self))
    }
}

/// What the current directory's read is doing.
#[derive(Clone, Debug)]
pub(crate) enum Load {
    Loading,
    Ready,
    Failed(Arc<str>),
}

/// What a crumb's subfolder menu has to show.
///
/// A failure carries no reason: the menu is one line tall, and the reason belongs to the list,
/// which names it when this folder is the one on screen.
pub(crate) enum Subfolders {
    Loading,
    Ready(Vec<Entry>),
    Failed,
}

/// What a keyboard or pointer action did.
pub(crate) enum Action {
    Redraw,
    Accept(Vec<PathBuf>),
}

/// The cells a range covers on screen, which is what a Shift extension or a drag selects.
///
/// The model knows no pixels: a grid is one row count, and the rectangle follows from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RangeShape {
    /// The list: every index between the two ends, inclusive.
    Column,
    /// The grid: every cell between the two corners, inclusive, `columns` cells per row.
    Grid { columns: usize },
}

/// A pointer drag over the rows: where it began and the marks it must extend.
struct DragSelect {
    anchor: usize,
    base: Vec<usize>,
}

pub(crate) struct Model {
    source: Arc<dyn ListingSource>,
    request: ListingRequest,
    /// Everything the directory holds, as the lister answered it.
    entries: Arc<Vec<Entry>>,
    /// The rows the view draws: `entries` through the filter, in the view's order.
    rows: Vec<Entry>,
    pub(crate) load: Load,
    /// The keyboard's row, and the row the next click extends from. In a multi mode it may
    /// be unmarked: moving with the arrows does not change what is selected, as in every
    /// native dialog.
    pub(crate) cursor: Option<usize>,
    /// Rows marked in a multi mode, always sorted: the answer comes back in row order.
    marked: Vec<usize>,
    /// The row a Shift extension grows from: the last row a plain click or arrow selected.
    anchor: Option<usize>,
    /// The live pointer drag over the rows and the marks it started with, so a drag can
    /// widen the selection it began on without losing what was already marked.
    drag: Option<DragSelect>,
    /// The filters the dialog offers, and the active one. `None` shows every file.
    filters: Vec<FileTypeFilter>,
    active: Option<usize>,
    mode: Mode,
    /// The name a save mode would write; kept while navigating, as native dialogs do.
    pub(crate) save_name: String,
    /// The names a `SaveFiles` mode writes into the folder the user picks.
    save_names: Vec<String>,
    /// Applied to a save name that has no extension of its own.
    default_extension: Option<String>,
    /// What accept does about a name that is taken, and whether the active filter is
    /// enforced on a save.
    overwrite: OverwritePolicy,
    strict_types: bool,
    /// A path to put the cursor on when its directory's listing lands.
    pending_select: Option<PathBuf>,
    /// Whether the rows on screen are a search's matches rather than the directory's entries.
    searching: bool,
}

impl Model {
    pub(crate) fn new(source: Arc<dyn ListingSource>, start: PathBuf, mode: Mode) -> Self {
        Self {
            source,
            request: ListingRequest::new(start),
            entries: Arc::new(Vec::new()),
            rows: Vec::new(),
            load: Load::Loading,
            cursor: None,
            marked: Vec::new(),
            anchor: None,
            drag: None,
            filters: Vec::new(),
            active: None,
            mode,
            save_name: String::new(),
            save_names: Vec::new(),
            default_extension: None,
            overwrite: OverwritePolicy::Prompt,
            strict_types: false,
            pending_select: None,
            searching: false,
        }
    }

    /// Put the cursor on `path` as soon as a listing names it; a path the directory does
    /// not hold is silently ignored, as a native dialog does when its initial selection has
    /// gone.
    pub(crate) fn select_when_listed(&mut self, path: PathBuf) {
        self.pending_select = Some(path);
    }

    pub(crate) fn path(&self) -> &Path {
        &self.request.path
    }

    pub(crate) fn rows(&self) -> &[Entry] {
        &self.rows
    }

    pub(crate) fn show_hidden(&self) -> bool {
        self.request.show_hidden
    }

    /// The directories inside `path`, for the menu under the crumb that names it.
    ///
    /// The current directory's request is the one the model already holds, so the menu
    /// answers from that listing; an ancestor is read through the same service and the same
    /// hidden-files setting as the list.
    pub(crate) fn subfolders(&self, path: &Path) -> Subfolders {
        let request = ListingRequest {
            path: path.to_path_buf(),
            show_hidden: self.request.show_hidden,
        };
        match self.source.get(&request) {
            ListingState::Loading => Subfolders::Loading,
            ListingState::Failed(_) => Subfolders::Failed,
            ListingState::Ready(entries) => {
                let mut folders: Vec<Entry> = entries
                    .iter()
                    .filter(|entry| entry.directory)
                    .cloned()
                    .collect();
                folders.sort_by(compare_entries);
                Subfolders::Ready(folders)
            }
        }
    }

    /// The active filter, or `None` when every file is shown.
    pub(crate) fn active_filter(&self) -> Option<&FileTypeFilter> {
        self.active.and_then(|index| self.filters.get(index))
    }

    /// The active filter's index, for an answer that names it.
    pub(crate) fn active_filter_index(&self) -> Option<usize> {
        self.active
    }

    pub(crate) fn set_filters(&mut self, filters: Vec<FileTypeFilter>, active: Option<usize>) {
        self.filters = filters;
        self.active = active.filter(|index| *index < self.filters.len());
        self.sort_and_filter();
    }

    pub(crate) fn set_default_extension(&mut self, extension: Option<String>) {
        self.default_extension = extension;
    }

    pub(crate) fn set_save_names(&mut self, names: Vec<String>) {
        self.save_names = names;
    }

    pub(crate) fn save_names(&self) -> &[String] {
        &self.save_names
    }

    pub(crate) fn set_save_policy(&mut self, overwrite: OverwritePolicy, strict_types: bool) {
        self.overwrite = overwrite;
        self.strict_types = strict_types;
    }

    /// What accepting a name that is taken should do. `Allow` and `Forbid` are decided by
    /// [`Self::accepted`]; `Prompt` is the caller's question to ask.
    pub(crate) fn overwrite(&self) -> OverwritePolicy {
        self.overwrite
    }

    /// The filters the type menu lists, in the caller's order.
    pub(crate) fn filters(&self) -> &[FileTypeFilter] {
        &self.filters
    }

    pub(crate) fn set_active_filter(&mut self, index: usize) {
        if index >= self.filters.len() || self.active == Some(index) {
            return;
        }
        self.active = Some(index);
        self.sort_and_filter();
    }

    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }

    /// Whether the row at `index` is part of what accept would return.
    ///
    /// A single mode's cursor is its selection even when the row is a directory the mode
    /// will not accept: the highlight says where the keyboard is, and `accepted` is what
    /// decides whether the button may be pressed.
    pub(crate) fn is_selected(&self, index: usize) -> bool {
        if self.mode.is_multi() {
            return self.marked.contains(&index);
        }
        self.cursor == Some(index)
    }

    /// Whether the keyboard's cursor stands on `index`.
    ///
    /// A multi mode can hold marks the cursor is not on (`Ctrl+Arrow` browses without
    /// changing the answer), so the marks alone cannot say where the keyboard is: the view
    /// paints the cursor apart from the selection.
    pub(crate) fn is_cursor(&self, index: usize) -> bool {
        self.cursor == Some(index)
    }

    /// The paths accept would return now, with the filter they were chosen under.
    pub(crate) fn accepted(&self) -> Option<(Vec<PathBuf>, Option<usize>)> {
        if !matches!(self.load, Load::Ready) {
            return None;
        }
        let filter = self.active_filter_index();
        match self.mode {
            Mode::SaveFile => {
                let path = self.save_path()?;
                self.save_accepts(&[path.as_path()])
                    .then(|| (vec![path], filter))
            }
            Mode::SaveFiles => {
                let paths: Vec<PathBuf> = self
                    .save_names
                    .iter()
                    .map(|name| self.request.path.join(self.with_default_extension(name)))
                    .collect();
                let chosen: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
                self.save_accepts(&chosen).then_some((paths, filter))
            }
            Mode::SelectFolder if self.searching => None,
            Mode::SelectFolder => Some((vec![self.request.path.clone()], filter)),
            Mode::SelectFolders => {
                let paths: Vec<PathBuf> = self
                    .marked
                    .iter()
                    .filter_map(|index| self.rows.get(*index))
                    .filter(|entry| entry.directory)
                    .map(|entry| entry.path.clone())
                    .collect();
                (!paths.is_empty()).then_some((paths, filter))
            }
            Mode::OpenFile => self
                .cursor
                .and_then(|index| self.rows.get(index))
                .filter(|entry| !entry.directory)
                .map(|entry| (vec![entry.path.clone()], filter)),
            Mode::OpenFiles => {
                let paths: Vec<PathBuf> = self
                    .marked
                    .iter()
                    .filter_map(|index| self.rows.get(*index))
                    .filter(|entry| !entry.directory)
                    .map(|entry| entry.path.clone())
                    .collect();
                (!paths.is_empty()).then_some((paths, filter))
            }
        }
    }

    /// Whether a save may accept these names: the active filter, when the request enforces
    /// it, and the overwrite policy's refusal of names that already exist.
    fn save_accepts(&self, paths: &[&Path]) -> bool {
        if paths.is_empty() {
            return false;
        }
        if self.overwrite == OverwritePolicy::Forbid && paths.iter().any(|path| path.exists()) {
            return false;
        }
        if !self.strict_types {
            return true;
        }
        let Some(filter) = self.active_filter() else {
            return true;
        };
        paths.iter().all(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            filter.matches(name)
                || mime_type(path).is_some_and(|mime| filter.matches_mime(mime))
        })
    }

    /// The path a save would write: the current folder plus the typed name, with the
    /// default extension appended when the name has none.
    fn save_path(&self) -> Option<PathBuf> {
        let name = self.save_name.trim();
        if name.is_empty() || name.contains(['/', '\\', '\0']) {
            return None;
        }
        Some(self.request.path.join(self.with_default_extension(name)))
    }

    fn with_default_extension(&self, name: &str) -> String {
        if Path::new(name).extension().is_some() {
            return name.to_string();
        }
        match &self.default_extension {
            Some(extension) if !extension.is_empty() => {
                format!("{name}.{}", extension.trim_start_matches('.'))
            }
            _ => name.to_string(),
        }
    }

    /// Read the current directory again when the user navigates into it.
    ///
    /// A destination is re-read so a picker opened twice does not show yesterday's files;
    /// the directory being left keeps its cached answer for a quick step back.
    pub(crate) fn navigate(&mut self, path: PathBuf) {
        if path == self.request.path {
            // A jump to the folder already on screen is still a way out of a search: the
            // rows it left behind are matches, not this folder's entries.
            self.end_search();
            return;
        }
        self.searching = false;
        self.source.invalidate(&path);
        self.request.path = path;
        self.load = Load::Loading;
        self.forget_selection();
    }

    /// A new directory (or a new listing) has no rows the old selection could name.
    fn forget_selection(&mut self) {
        self.cursor = None;
        self.marked.clear();
        self.anchor = None;
        self.drag = None;
    }

    pub(crate) fn up(&mut self) {
        match self.request.path.parent().map(Path::to_path_buf) {
            Some(parent) => self.navigate(parent),
            // At a root there is nowhere to go, but the rows are still a search's: leaving
            // the search is the answer the key can give.
            None => self.end_search(),
        }
    }

    pub(crate) fn set_show_hidden(&mut self, show: bool) {
        if self.request.show_hidden == show {
            return;
        }
        self.request.show_hidden = show;
        self.load = Load::Loading;
        self.forget_selection();
    }

    /// Turn whatever the worker finished into rows.
    pub(crate) fn poll(&mut self) {
        if self.searching || !matches!(self.load, Load::Loading) {
            return;
        }
        match self.source.get(&self.request) {
            ListingState::Loading => {}
            ListingState::Ready(entries) => {
                self.entries = entries;
                self.sort_and_filter();
                self.load = Load::Ready;
                if let Some(path) = self.pending_select.take() {
                    self.cursor = self
                        .rows
                        .iter()
                        .position(|entry| entry.path == path);
                }
            }
            ListingState::Failed(reason) => {
                // The previous directory's rows are not this directory's answer: keeping
                // them would hide the failure behind a list of somewhere else.
                self.entries = Arc::new(Vec::new());
                self.rows.clear();
                self.forget_selection();
                self.load = Load::Failed(reason);
            }
        }
    }

    /// Whether the rows on screen are a search's matches rather than a directory's entries.
    pub(crate) fn searching(&self) -> bool {
        self.searching
    }

    /// Show what a search has found so far. `done` says the walk ended, which is what lets
    /// the accept button answer; a running walk may still add rows under it.
    pub(crate) fn show_search(&mut self, entries: Arc<Vec<Entry>>, done: bool) {
        let was_done = matches!(self.load, Load::Ready);
        let unchanged = self.searching && Arc::ptr_eq(&self.entries, &entries) && was_done == done;
        self.searching = true;
        self.load = if done { Load::Ready } else { Load::Loading };
        if unchanged {
            return;
        }
        self.entries = entries;
        self.sort_and_filter();
    }

    /// The walk failed before it could answer.
    pub(crate) fn search_failed(&mut self, reason: Arc<str>) {
        self.searching = true;
        self.entries = Arc::new(Vec::new());
        self.rows.clear();
        self.forget_selection();
        self.load = Load::Failed(reason);
    }

    /// Leave a search and show the current directory again.
    pub(crate) fn end_search(&mut self) {
        if !self.searching {
            return;
        }
        self.searching = false;
        self.entries = Arc::new(Vec::new());
        self.rows.clear();
        self.forget_selection();
        self.source.invalidate(&self.request.path);
        self.load = Load::Loading;
    }

    /// Read the current directory again after something changed it.
    pub(crate) fn refresh(&mut self) {
        if self.searching {
            return;
        }
        self.source.invalidate(&self.request.path);
        self.load = Load::Loading;
    }

    /// Put the cursor on `index`. A single mode's selection moves with it; a multi mode's
    /// marks are left alone, so the arrows can browse without changing the answer.
    pub(crate) fn select(&mut self, index: usize) {
        if index >= self.rows.len() {
            return;
        }
        self.cursor = Some(index);
    }

    /// Mark or unmark the row at `index`, the way a click does in a multi mode.
    pub(crate) fn toggle(&mut self, index: usize) {
        if index >= self.rows.len() || !self.mode.is_multi() {
            return;
        }
        match self.marked.binary_search(&index) {
            Ok(at) => {
                self.marked.remove(at);
            }
            Err(at) => self.marked.insert(at, index),
        }
    }

    /// The paths a file verb acts on: the marked rows in a multi mode, the cursor's row in
    /// a single one.
    ///
    /// The cursor answers even when the mode will not accept it (a folder in a file mode):
    /// "Select folder" and "Rename" are about the row the user stands on, not about what the
    /// dialog's accept button would return.
    pub(crate) fn selection_paths(&self) -> Vec<PathBuf> {
        self.selection_entries()
            .iter()
            .map(|entry| entry.path.clone())
            .collect()
    }

    /// The rows a verb acts on, as the listing knows them: a mark answers both the path and
    /// whether the row is a folder, and a verb that must refuse one needs both.
    pub(crate) fn selection_entries(&self) -> Vec<&Entry> {
        if self.mode.is_multi() {
            return self
                .marked
                .iter()
                .filter_map(|index| self.rows.get(*index))
                .collect();
        }
        self.cursor
            .and_then(|index| self.rows.get(index))
            .into_iter()
            .collect()
    }

    /// Mark every row the mode can answer that is not marked, and unmark the rest.
    pub(crate) fn invert_selection(&mut self) -> Action {
        if !self.mode.is_multi() {
            return Action::Redraw;
        }
        self.marked = (0..self.rows.len())
            .filter(|index| self.row_takes(*index) && self.marked.binary_search(index).is_err())
            .collect();
        if let Some(first) = self.marked.first().copied() {
            self.cursor = Some(first);
            self.anchor = Some(first);
        }
        Action::Redraw
    }

    /// Mark the cursor's row in a multi mode, so Enter and the accept button agree.
    pub(crate) fn toggle_cursor(&mut self) {
        if let Some(index) = self.cursor {
            self.toggle(index);
        }
    }

    /// Move the cursor to `next`, the row a plain arrow selects.
    ///
    /// The caller decides what one screen row means — one entry in the list, a row of cells
    /// in the grid — because that is the view's geometry, not the model's.
    pub(crate) fn move_selection(&mut self, next: usize) -> Action {
        self.cursor = Some(next);
        if self.mode.is_multi() {
            // An arrow moves the selection, as it does in Explorer and Dolphin: a cursor
            // the answer does not follow would leave the highlighted row unexplained.
            if self.row_takes(next) {
                self.marked.clear();
                self.marked.push(next);
            }
            self.anchor = Some(next);
        }
        Action::Redraw
    }

    /// Move the cursor to `next` and leave the marks alone (Ctrl+Arrow).
    pub(crate) fn move_cursor(&mut self, next: usize) -> Action {
        self.cursor = Some(next);
        Action::Redraw
    }

    /// Grow the marks from the anchor to `next` (Shift+Arrow).
    pub(crate) fn extend_selection(&mut self, next: usize, shape: RangeShape) -> Action {
        let anchor = self.anchor.or(self.cursor).unwrap_or(next);
        self.anchor = Some(anchor);
        self.cursor = Some(next);
        if self.mode.is_multi() {
            self.marked = self.marks_in(anchor, next, shape);
        }
        Action::Redraw
    }

    /// Move to the next row whose name starts with `letter`, cycling through the rows.
    ///
    /// The search starts after the cursor, so pressing a letter again advances past the name
    /// it is already on; a cursor on the last such row wraps to the first, and a letter no
    /// name starts with leaves the selection where it was.
    pub(crate) fn type_ahead(&mut self, letter: char) -> bool {
        if self.rows.is_empty() {
            return false;
        }
        let count = self.rows.len();
        let start = self.cursor.map_or(0, |index| (index + 1) % count);
        let found = (start..start + count)
            .map(|at| at % count)
            .find(|index| {
                self.rows[*index]
                    .name
                    .chars()
                    .next()
                    .is_some_and(|initial| initial.to_ascii_lowercase() == letter.to_ascii_lowercase())
            });
        let Some(index) = found else {
            return false;
        };
        self.move_selection(index);
        true
    }

    /// Select every row the mode can answer (Ctrl+A).
    pub(crate) fn select_all(&mut self) -> Action {
        if !self.mode.is_multi() {
            return Action::Redraw;
        }
        self.marked = (0..self.rows.len())
            .filter(|index| self.row_takes(*index))
            .collect();
        if let Some(first) = self.marked.first().copied() {
            self.cursor = Some(first);
            self.anchor = Some(first);
        }
        Action::Redraw
    }

    pub(crate) fn select_first(&mut self) -> Action {
        self.place_cursor(0);
        Action::Redraw
    }

    pub(crate) fn select_last(&mut self) -> Action {
        if !self.rows.is_empty() {
            self.place_cursor(self.rows.len() - 1);
        }
        Action::Redraw
    }

    /// Move the cursor the way a plain selection does: the marks follow it in a multi mode.
    fn place_cursor(&mut self, index: usize) {
        if index >= self.rows.len() {
            return;
        }
        self.cursor = Some(index);
        if self.mode.is_multi() && self.row_takes(index) {
            self.marked.clear();
            self.marked.push(index);
        }
        self.anchor = Some(index);
    }

    /// Whether the mode's answer can name this row, which is also what a click may mark.
    fn row_takes(&self, index: usize) -> bool {
        self.rows.get(index).is_some_and(|entry| {
            if self.mode.takes_directories() {
                entry.directory
            } else {
                !entry.directory
            }
        })
    }

    /// Every acceptable cell between two indices, inclusive, in the shape the view shows.
    fn marks_in(&self, from: usize, to: usize, shape: RangeShape) -> Vec<usize> {
        let cells: Vec<usize> = match shape {
            RangeShape::Grid { columns } if columns > 1 => {
                let (from_row, from_column) = (from / columns, from % columns);
                let (to_row, to_column) = (to / columns, to % columns);
                let rows = from_row.min(to_row)..=from_row.max(to_row);
                let left = from_column.min(to_column);
                let right = from_column.max(to_column);
                let mut cells = Vec::new();
                for row in rows {
                    for column in left..=right {
                        let index = row * columns + column;
                        if index < self.rows.len() {
                            cells.push(index);
                        }
                    }
                }
                cells
            }
            _ => (from.min(to)..=from.max(to)).collect(),
        };
        cells
            .into_iter()
            .filter(|index| self.row_takes(*index))
            .collect()
    }

    /// The engine reported a press travelled past the drag threshold on `anchor`.
    pub(crate) fn begin_drag(&mut self, anchor: usize) {
        if anchor >= self.rows.len() {
            return;
        }
        self.drag = Some(DragSelect {
            anchor,
            base: self.marked.clone(),
        });
        self.anchor = Some(anchor);
    }

    /// The paths a drag starting on `index` carries.
    ///
    /// A marked row carries the whole marked set, as every file manager does; a row outside
    /// it carries only itself, because dragging an unmarked row must not smuggle the old
    /// selection out of the dialog.
    pub(crate) fn drag_paths(&self, index: usize) -> Vec<PathBuf> {
        let marked = self.mode.is_multi() && self.marked.contains(&index);
        if marked {
            let paths: Vec<PathBuf> = self
                .marked
                .iter()
                .filter_map(|index| self.rows.get(*index))
                .map(|entry| entry.path.clone())
                .collect();
            if !paths.is_empty() {
                return paths;
            }
        }
        self.rows
            .get(index)
            .map(|entry| vec![entry.path.clone()])
            .unwrap_or_default()
    }

    /// Forget an answer the filesystem can no longer stand behind.
    pub(crate) fn invalidate(&mut self, path: &Path) {
        self.source.invalidate(path);
    }

    /// The pointer is over `row` during the drag that began on the gesture's anchor.
    ///
    /// A plain drag replaces the selection with the rows it swept; Ctrl adds them to the
    /// marks the gesture began with, as a file manager does.
    pub(crate) fn drag_to(
        &mut self,
        row: usize,
        modifiers: Modifiers,
        shape: RangeShape,
    ) -> Action {
        if self.rows.is_empty() {
            return Action::Redraw;
        }
        let row = row.min(self.rows.len() - 1);
        let (anchor, base) = match &self.drag {
            Some(drag) => (drag.anchor, drag.base.clone()),
            None => (row, self.marked.clone()),
        };
        if self.drag.is_none() {
            self.drag = Some(DragSelect {
                anchor,
                base: base.clone(),
            });
        }
        let anchor = anchor.min(self.rows.len() - 1);
        self.cursor = Some(row);
        if self.mode.is_multi() {
            let mut marks = self.marks_in(anchor, row, shape);
            if modifiers.ctrl {
                marks.extend(base);
                marks.sort_unstable();
                marks.dedup();
            }
            self.marked = marks;
        }
        Action::Redraw
    }

    /// The drag ended; the next gesture captures its own base marks.
    pub(crate) fn end_drag(&mut self) {
        self.drag = None;
    }

    /// What Enter does to the cursor's row.
    ///
    /// A directory is always a step into it; a folder picker accepts its current folder
    /// with the button, not with Enter. A save mode's Enter is the accept button's, since
    /// the name field, not the row, is what it would write.
    ///
    /// A listing that has not landed yet answers nothing: the rows on screen are the
    /// directory the user left, and accepting one of them would hand the app a path the
    /// dialog is not showing.
    pub(crate) fn activate(&mut self) -> Action {
        if !matches!(self.load, Load::Ready) {
            return Action::Redraw;
        }
        if let Some((paths, _)) = self.accepted() {
            return Action::Accept(paths);
        }
        let Some(entry) = self.cursor.and_then(|index| self.rows.get(index)) else {
            return Action::Redraw;
        };
        if entry.directory {
            let path = entry.path.clone();
            self.navigate(path);
        }
        Action::Redraw
    }

    /// The row a click at `index` touches, shaped by the held modifiers the way Explorer and
    /// Dolphin shape it: plain replaces, Ctrl toggles, Shift extends from the anchor.
    ///
    /// `open` is a double click or Enter: a directory is entered and a file is the answer.
    /// A single click only marks the row, so a slow second click stays a selection.
    pub(crate) fn row_clicked(
        &mut self,
        index: usize,
        modifiers: Modifiers,
        shape: RangeShape,
        open: bool,
    ) -> Action {
        if index >= self.rows.len() {
            return Action::Redraw;
        }
        self.drag = None;
        let is_file = !self.rows[index].directory;
        // A save with a name field takes an existing file's name, as native dialogs do;
        // `SaveFiles` has no field, its names come from the request.
        if self.mode == Mode::SaveFile && is_file {
            self.take_click(index, modifiers, shape);
            self.save_name = self.rows[index].name.clone();
            return Action::Redraw;
        }
        if open {
            if self.rows[index].directory {
                let path = self.rows[index].path.clone();
                self.navigate(path);
                return Action::Redraw;
            }
            self.take_click(index, modifiers, shape);
            // A file is the answer in the modes that open one; a mode that answers with
            // directories, or with names no click names, has nothing to take here.
            if !self.mode.is_save() && !self.mode.takes_directories() {
                return match self.accepted() {
                    Some((paths, _)) => Action::Accept(paths),
                    None => Action::Redraw,
                };
            }
            return Action::Redraw;
        }
        self.take_click(index, modifiers, shape);
        Action::Redraw
    }

    /// Mark the row a click touched: plain replaces, Ctrl toggles, Shift extends.
    fn take_click(&mut self, index: usize, modifiers: Modifiers, shape: RangeShape) {
        if self.mode.is_multi() {
            let takes = self.row_takes(index);
            if modifiers.ctrl {
                self.cursor = Some(index);
                self.anchor = Some(index);
                if takes {
                    self.toggle(index);
                }
                return;
            }
            if modifiers.shift {
                let anchor = self.anchor.or(self.cursor).unwrap_or(index);
                self.cursor = Some(index);
                self.marked = self.marks_in(anchor, index, shape);
                return;
            }
            self.cursor = Some(index);
            self.anchor = Some(index);
            // A row the mode cannot answer is a step of navigation; clicking it while
            // browsing must not throw away the marks the user is assembling.
            if takes {
                self.marked.clear();
                self.marked.push(index);
            }
            return;
        }
        self.select(index);
        self.anchor = Some(index);
    }

    pub(crate) fn accept_enabled(&self) -> bool {
        self.accepted().is_some()
    }

    /// The thumbnail request this row may send, if its policy allows one at `size` pixels.
    pub(crate) fn thumbnail_request(
        &self,
        entry: &Entry,
        policy: ThumbnailPolicy,
        size: u32,
    ) -> Option<ThumbnailRequest> {
        if !policy.show || entry.directory || entry.len > policy.max_file_size {
            return None;
        }
        Some(
            ThumbnailRequest::new(entry.path.clone(), size).versioned(entry.version()),
        )
    }

    /// Ask for the current directory again after a read failed.
    pub(crate) fn retry(&mut self) {
        self.source.invalidate(&self.request.path);
        self.load = Load::Loading;
        self.forget_selection();
    }

    fn sort_and_filter(&mut self) {
        let active = self.active;
        let mut rows: Vec<Entry> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.directory
                    || active.is_none_or(|index| {
                        self.filters.get(index).is_none_or(|filter| {
                            filter.matches(&entry.name)
                                || mime_type(&entry.path)
                                    .is_some_and(|mime| filter.matches_mime(mime))
                        })
                    })
            })
            .cloned()
            .collect();
        rows.sort_by(compare_entries);
        // A filter can hide the rows under the cursor and the marks; the selection follows
        // a row by path, so a filter that shows it again finds it still selected.
        let cursor_path = self
            .cursor
            .and_then(|index| self.rows.get(index))
            .map(|entry| entry.path.clone());
        let anchor_path = self
            .anchor
            .and_then(|index| self.rows.get(index))
            .map(|entry| entry.path.clone());
        let marked_paths: Vec<PathBuf> = self
            .marked
            .iter()
            .filter_map(|index| self.rows.get(*index))
            .map(|entry| entry.path.clone())
            .collect();
        self.rows = rows;
        self.cursor =
            cursor_path.and_then(|path| self.rows.iter().position(|entry| entry.path == path));
        self.anchor =
            anchor_path.and_then(|path| self.rows.iter().position(|entry| entry.path == path));
        self.drag = None;
        self.marked = marked_paths
            .iter()
            .filter_map(|path| self.rows.iter().position(|entry| &entry.path == path))
            .collect();
        self.marked.sort_unstable();
        self.marked.dedup();
    }
}

/// Directories first, then a natural name order, then the exact name so the order is total.
pub(crate) fn compare_entries(a: &Entry, b: &Entry) -> Ordering {
    b.directory
        .cmp(&a.directory)
        .then_with(|| natural_compare(&a.name, &b.name))
        .then_with(|| a.name.cmp(&b.name))
}

/// Compare names the way a file manager does: a run of ASCII digits is a number, every
/// other byte compares case-insensitively.
///
/// `file2` sorts before `file10`, and `File2` beside `file2`; the caller's tie-break makes
/// that last pair deterministic. Non-ASCII bytes compare as they are, because locale
/// collation is not part of the prototype.
pub(crate) fn natural_compare(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (a_start, b_start) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let a_run = significant(&a[a_start..i]);
            let b_run = significant(&b[b_start..j]);
            match a_run.len().cmp(&b_run.len()) {
                Ordering::Equal => match a_run.cmp(b_run) {
                    Ordering::Equal => continue,
                    other => return other,
                },
                other => return other,
            }
        }
        let (a_byte, b_byte) = (a[i].to_ascii_lowercase(), b[j].to_ascii_lowercase());
        if a_byte != b_byte {
            return a_byte.cmp(&b_byte);
        }
        i += 1;
        j += 1;
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// A digit run without its leading zeros, so `007` and `7` weigh the same.
fn significant(run: &[u8]) -> &[u8] {
    let first = run.iter().position(|byte| *byte != b'0');
    match first {
        Some(index) => &run[index..],
        // All zeros: one zero, so `0` and `000` compare equal and the tie-break decides.
        None => &run[run.len().saturating_sub(1)..],
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use ui_platform::FileKind;

    use super::*;

    struct Answers(RefCell<HashMap<ListingRequest, ListingState>>);

    impl ListingSource for Answers {
        fn get(&self, request: &ListingRequest) -> ListingState {
            match self.0.borrow().get(request) {
                Some(state) => state.clone(),
                None => ListingState::Loading,
            }
        }

        fn invalidate(&self, _path: &Path) {}
    }

    fn entry(name: &str, directory: bool) -> Entry {
        Entry {
            name: name.to_string(),
            path: PathBuf::from("/dir").join(name),
            kind: if directory {
                FileKind::Folder
            } else {
                FileKind::Generic
            },
            directory,
            link: false,
            len: 10,
            modified: None,
            hidden: name.starts_with('.'),
        }
    }

    fn answer(source: &Answers, path: &str, entries: Vec<Entry>) {
        source.0.borrow_mut().insert(
            ListingRequest::new(path),
            ListingState::Ready(Arc::new(entries)),
        );
    }

    fn model_with(path: &str, entries: Vec<Entry>) -> Model {
        let source = Answers(RefCell::new(HashMap::new()));
        answer(&source, path, entries);
        let mut model = Model::new(Arc::new(source), PathBuf::from(path), Mode::OpenFile);
        model.poll();
        model
    }

    fn names(model: &Model) -> Vec<&str> {
        model.rows().iter().map(|entry| entry.name.as_str()).collect()
    }

    fn ctrl() -> Modifiers {
        Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        }
    }

    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Modifiers::NONE
        }
    }

    #[test]
    fn directories_sort_before_files() {
        let model = model_with("/a", vec![entry("z.txt", false), entry("m", true)]);
        assert_eq!(names(&model), ["m", "z.txt"]);
    }

    #[test]
    fn a_natural_name_order_compares_digit_runs() {
        let model = model_with(
            "/a",
            vec![entry("file10", false), entry("file2", false), entry("file1", false)],
        );
        assert_eq!(names(&model), ["file1", "file2", "file10"]);
    }

    #[test]
    fn a_filter_never_hides_a_directory() {
        let mut model = model_with(
            "/a",
            vec![entry("keep", true), entry("notes.txt", false), entry("photo.png", false)],
        );
        model.set_filters(
            vec![FileTypeFilter::new("Text", vec!["*.txt".to_string()])],
            Some(0),
        );
        assert_eq!(names(&model), ["keep", "notes.txt"]);
    }

    #[test]
    fn a_stale_listing_is_dropped() {
        let source = Answers(RefCell::new(HashMap::new()));
        answer(&source, "/first", vec![entry("from-first", false)]);
        answer(&source, "/second", vec![entry("from-second", false)]);
        let mut model = Model::new(Arc::new(source), PathBuf::from("/first"), Mode::OpenFile);
        // Both directories are read; the user leaves the first before its answer is read.
        model.navigate(PathBuf::from("/second"));
        model.poll();
        assert_eq!(names(&model), ["from-second"]);
    }

    #[test]
    fn an_in_flight_listing_accepts_nothing() {
        let source = Answers(RefCell::new(HashMap::new()));
        answer(&source, "/first", vec![entry("from-first", false)]);
        answer(&source, "/second", vec![entry("from-second", false)]);
        let mut model = Model::new(Arc::new(source), PathBuf::from("/first"), Mode::OpenFile);
        model.poll();
        model.navigate(PathBuf::from("/second"));
        // The previous directory's row is still on screen and still clickable; accepting it
        // would hand the app a path the dialog is no longer showing.
        model.select(0);
        assert!(!model.accept_enabled());
        assert!(matches!(model.activate(), Action::Redraw));
        model.poll();
        model.select(0);
        assert!(model.accept_enabled());
        assert!(matches!(model.activate(), Action::Accept(_)));
    }

    #[test]
    fn a_failed_listing_leaves_no_rows_from_the_one_before_it() {
        let source = Answers(RefCell::new(HashMap::new()));
        answer(&source, "/first", vec![entry("from-first", false)]);
        source
            .0
            .borrow_mut()
            .insert(
                ListingRequest::new("/second"),
                ListingState::Failed(Arc::from("permission denied")),
            );
        let mut model = Model::new(Arc::new(source), PathBuf::from("/first"), Mode::OpenFile);
        model.poll();
        model.navigate(PathBuf::from("/second"));
        model.poll();
        assert!(
            names(&model).is_empty(),
            "a failure shows the error, not the previous directory's rows"
        );
    }

    #[test]
    fn a_multi_mode_answers_the_marked_files_in_row_order() {
        let source = Answers(RefCell::new(HashMap::new()));
        answer(
            &source,
            "/a",
            vec![entry("b.txt", false), entry("a.txt", false), entry("c.txt", false)],
        );
        let mut model = Model::new(Arc::new(source), PathBuf::from("/a"), Mode::OpenFiles);
        model.poll();
        // Marks are toggled out of row order; the answer follows the rows, not the clicks.
        model.row_clicked(2, ctrl(), RangeShape::Column, false);
        model.row_clicked(0, ctrl(), RangeShape::Column, false);
        let (paths, _) = model.accepted().expect("two files are marked");
        assert_eq!(
            paths,
            [PathBuf::from("/dir/a.txt"), PathBuf::from("/dir/c.txt")],
            "the answer is in row order, not click order"
        );
        model.row_clicked(1, ctrl(), RangeShape::Column, false);
        let (paths, _) = model.accepted().expect("three files are marked");
        assert_eq!(
            paths,
            [
                PathBuf::from("/dir/a.txt"),
                PathBuf::from("/dir/b.txt"),
                PathBuf::from("/dir/c.txt"),
            ]
        );
    }

    #[test]
    fn clicks_and_a_drag_select_the_way_a_file_manager_does() {
        let mut model = model_with(
            "/a",
            vec![
                entry("a.txt", false),
                entry("b.txt", false),
                entry("c.txt", false),
                entry("d.txt", false),
            ],
        );
        model.mode = Mode::OpenFiles;
        model.row_clicked(2, Modifiers::NONE, RangeShape::Column, false);
        assert_eq!(model.marked, [2], "a plain click replaces the selection");
        model.row_clicked(0, shift(), RangeShape::Column, false);
        assert_eq!(model.marked, [0, 1, 2], "Shift extends from the anchor");
        model.row_clicked(3, ctrl(), RangeShape::Column, false);
        assert_eq!(model.marked, [0, 1, 2, 3], "Ctrl adds the row it hits");
        model.row_clicked(1, ctrl(), RangeShape::Column, false);
        assert_eq!(model.marked, [0, 2, 3], "Ctrl removes a marked row");
        model.row_clicked(0, Modifiers::NONE, RangeShape::Column, false);
        model.begin_drag(0);
        model.drag_to(2, ctrl(), RangeShape::Column);
        assert_eq!(
            model.marked,
            [0, 1, 2],
            "a Ctrl drag adds the rows it sweeps to the marks it began with"
        );
        model.drag_to(1, ctrl(), RangeShape::Column);
        assert_eq!(
            model.marked,
            [0, 1],
            "a drag follows the pointer back out of the marks it added"
        );
        model.end_drag();
        model.begin_drag(3);
        model.drag_to(1, Modifiers::NONE, RangeShape::Column);
        assert_eq!(
            model.marked,
            [1, 2, 3],
            "a plain drag replaces the selection with the rows it swept"
        );
    }

    #[test]
    fn a_double_click_opens_a_file_while_one_click_only_selects_it() {
        let mut model = model_with("/a", vec![entry("folder", true), entry("go.txt", false)]);
        assert!(
            matches!(
                model.row_clicked(1, Modifiers::NONE, RangeShape::Column, false),
                Action::Redraw
            ),
            "one click selects the file, whatever the selection was before"
        );
        assert_eq!(model.cursor, Some(1));
        assert!(
            matches!(
                model.row_clicked(1, Modifiers::NONE, RangeShape::Column, true),
                Action::Accept(_)
            ),
            "a double click takes the file it names"
        );
        assert!(
            matches!(
                model.row_clicked(0, Modifiers::NONE, RangeShape::Column, true),
                Action::Redraw
            ),
            "a double click on a folder is a step into it, not an answer"
        );
        assert_eq!(model.path(), Path::new("/dir/folder"));
    }

    #[test]
    fn type_ahead_advances_through_names_with_the_same_initial_and_wraps() {
        let mut model = model_with(
            "/a",
            vec![
                entry("apple.txt", false),
                entry("apricot.txt", false),
                entry("avocado.txt", false),
                entry("banana.txt", false),
            ],
        );
        assert!(model.type_ahead('a'));
        assert_eq!(model.cursor, Some(0), "nothing focused starts at the first match");
        assert!(model.type_ahead('a'));
        assert_eq!(model.cursor, Some(1), "the same letter advances past the focused name");
        assert!(model.type_ahead('A'));
        assert_eq!(model.cursor, Some(2), "a capital finds the same names");
        assert!(model.type_ahead('a'));
        assert_eq!(model.cursor, Some(0), "the search wraps around the end");
        assert!(model.type_ahead('b'));
        assert_eq!(model.cursor, Some(3), "another letter jumps to its own first name");
        assert!(!model.type_ahead('z'), "a letter no name starts with changes nothing");
        assert_eq!(model.cursor, Some(3));
    }

    #[test]
    fn a_grid_range_is_the_rectangle_between_its_corners() {
        let mut model = model_with(
            "/a",
            (0..9)
                .map(|index| entry(&format!("{index}.txt"), false))
                .collect(),
        );
        model.mode = Mode::OpenFiles;
        // Three columns; dragging from the top middle to the bottom right selects the
        // corner columns in both rows, not the whole rows the linear range would take.
        model.begin_drag(1);
        model.drag_to(8, Modifiers::NONE, RangeShape::Grid { columns: 3 });
        assert_eq!(model.marked, [1, 2, 4, 5, 7, 8]);
        // A rectangle swept the other way is the same rectangle.
        model.end_drag();
        model.begin_drag(7);
        model.drag_to(2, Modifiers::NONE, RangeShape::Grid { columns: 3 });
        assert_eq!(model.marked, [1, 2, 4, 5, 7, 8]);
        // Ctrl keeps the marks the drag began with, as it does in the list.
        model.end_drag();
        model.row_clicked(0, Modifiers::NONE, RangeShape::Grid { columns: 3 }, false);
        model.begin_drag(4);
        model.drag_to(5, ctrl(), RangeShape::Grid { columns: 3 });
        assert_eq!(model.marked, [0, 4, 5]);
    }

    #[test]
    fn a_save_name_is_joined_to_the_folder_with_the_default_extension() {
        let mut model = model_with("/a", vec![entry("old.txt", false)]);
        model.mode = Mode::SaveFile;
        model.set_default_extension(Some("png".to_string()));
        model.save_name = "shot".to_string();
        let (paths, _) = model.accepted().expect("a name in a readable folder is a path");
        assert_eq!(paths, [PathBuf::from("/a/shot.png")]);
        model.save_name = "shot.png".to_string();
        let (paths, _) = model.accepted().expect("the name already has an extension");
        assert_eq!(paths, [PathBuf::from("/a/shot.png")]);
        model.save_name = "no/slashes".to_string();
        assert!(
            model.accepted().is_none(),
            "a name carrying a path is not a file name"
        );
    }

    #[test]
    fn a_multi_folder_mode_answers_the_marked_folders_and_still_navigates() {
        let source = Answers(RefCell::new(HashMap::new()));
        answer(
            &source,
            "/a",
            vec![entry("one", true), entry("two", true), entry("notes.txt", false)],
        );
        answer(&source, "/dir/two", vec![entry("inside", true)]);
        let mut model = Model::new(Arc::new(source), PathBuf::from("/a"), Mode::SelectFolders);
        model.poll();
        // A file is not an answer in a folder mode, so clicking one marks nothing.
        model.row_clicked(2, Modifiers::NONE, RangeShape::Column, false);
        assert!(model.accepted().is_none());
        model.row_clicked(0, ctrl(), RangeShape::Column, false);
        model.row_clicked(1, ctrl(), RangeShape::Column, false);
        let (paths, _) = model.accepted().expect("two folders are marked");
        assert_eq!(paths, [PathBuf::from("/dir/one"), PathBuf::from("/dir/two")]);
        // A double click on a marked folder steps into it, dropping the marks: browsing
        // must stay possible while an answer is being assembled.
        model.row_clicked(1, Modifiers::NONE, RangeShape::Column, true);
        assert_eq!(model.path(), Path::new("/dir/two"));
        model.poll();
        assert!(
            model.accepted().is_none(),
            "entering a folder leaves no marked answer behind"
        );
    }

    #[test]
    fn a_save_of_several_names_joins_each_to_the_folder() {
        let mut model = model_with("/a", Vec::new());
        model.mode = Mode::SaveFiles;
        model.set_default_extension(Some("png".to_string()));
        model.set_save_names(vec!["one.png".to_string(), "two".to_string()]);
        let (paths, _) = model
            .accepted()
            .expect("two names in a readable folder are paths");
        assert_eq!(
            paths,
            [PathBuf::from("/a/one.png"), PathBuf::from("/a/two.png")]
        );
    }

    #[test]
    fn a_strict_save_refuses_a_name_outside_the_active_filter() {
        let mut model = model_with("/a", Vec::new());
        model.mode = Mode::SaveFile;
        model.set_filters(
            vec![FileTypeFilter::new("Images", vec!["*.png".to_string()])],
            Some(0),
        );
        model.set_save_policy(OverwritePolicy::Allow, true);
        model.save_name = "notes.txt".to_string();
        assert!(
            model.accepted().is_none(),
            "a strict request does not accept a name its filter rejects"
        );
        model.save_name = "notes.png".to_string();
        assert!(model.accepted().is_some());
    }

    #[test]
    fn a_row_outside_the_thumbnail_policy_sends_no_request() {
        let model = model_with("/a", vec![entry("photo.png", false), entry("folder", true)]);
        let policy = ThumbnailPolicy::default();
        let file = &model.rows()[1];
        assert!(
            model.thumbnail_request(file, policy, 32).is_some(),
            "a file inside the policy asks for its picture"
        );
        assert!(
            model
                .thumbnail_request(file, ThumbnailPolicy { show: false, ..policy }, 32)
                .is_none(),
            "previews off sends no request"
        );
        let folder = &model.rows()[0];
        assert!(
            model.thumbnail_request(folder, policy, 32).is_none(),
            "a directory has no picture to ask for"
        );
        let too_big = ThumbnailPolicy {
            max_file_size: 1,
            ..policy
        };
        assert!(
            model.thumbnail_request(file, too_big, 32).is_none(),
            "a file past the size limit sends no request"
        );
    }
}
