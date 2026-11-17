//! The platform's file context menu, as rows a picker can draw or hand back to the system.
//!
//! A merge is partial: a row the shell draws itself has no text to carry, so it is counted
//! as omitted and stays reachable through the native menu the caller keeps beside the merge.
//!
//! The shell builds its menu by calling third-party extensions, which may block for as long
//! as they like, so a host reads and runs commands on a thread of its own: the caller asks and
//! polls instead of waiting.

use std::path::PathBuf;

/// The rows a host could read, and how many it could not carry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellMenuRead {
    pub rows: Vec<ShellMenuRow>,
    /// Rows the shell draws through `WM_DRAWITEM` or a callback bitmap; only the native menu
    /// can show them, and it holds the same selection.
    pub omitted: usize,
}

/// One row of a shell context menu.
///
/// `token` is the command's id relative to the menu's own first id, which is what
/// [`ShellMenuHost::invoke`] takes. It carries no meaning for a separator or a submenu
/// header, so those rows report zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellMenuRow {
    pub label: String,
    pub token: u32,
    pub enabled: bool,
    pub checked: bool,
    pub separator: bool,
    /// The rows inside a cascading menu; `None` for a command row.
    pub submenu: Option<Vec<ShellMenuRow>>,
}

/// A desktop's file context menu for a selection the picker holds.
///
/// The picker keeps this as `Option<Arc<dyn ShellMenuHost>>`: Windows has the classic shell
/// menu, Wayland has no context-menu protocol. The shell menu is built for one folder's
/// selection, so every method refuses a selection that spans folders.
pub trait ShellMenuHost: Send + Sync {
    /// Whether this selection is one this host can answer for.
    fn available(&self, paths: &[PathBuf]) -> bool;

    /// Start reading the menu's rows for `paths`, answering with the ticket to poll.
    ///
    /// The read runs on the host's own thread. A newer read replaces one still queued, so a
    /// burst of presses does not queue a menu each.
    fn read(&self, paths: Vec<PathBuf>) -> u64;

    /// The answer to `ticket`'s read, once it has finished; the slot is emptied.
    ///
    /// `None` while the read still runs. A menu the shell could not build at all answers an
    /// empty read.
    fn take_rows(&self, ticket: u64) -> Option<ShellMenuRead>;

    /// Run the command `token` for this selection, on the host's own thread.
    ///
    /// The token is expected to still name the same command a previous
    /// [`read`](Self::read) returned. A shell menu whose command ids shift between reads is
    /// out of contract.
    fn invoke(&self, paths: Vec<PathBuf>, token: u32);

    /// How many commands have finished. A caller that sees this move re-reads the folder,
    /// which a shell verb may have changed.
    fn invocations(&self) -> u64;

    /// Show the platform's own menu at `at`, in the window's logical client units; the host
    /// converts it to screen pixels on the window it belongs to. False when the menu could
    /// not be built, so the caller can draw its own instead.
    fn show_native(&self, paths: &[PathBuf], at: (f32, f32)) -> bool;
}
