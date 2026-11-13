//! Host light/dark preference and the mechanism that keeps it in sync.
//!
//! Portable interface only: a platform crate supplies a concrete watcher (e.g. the XDG
//! portal on Wayland). The app reads `preference()` and drains `poll()` each frame.

/// The host's stated colour-scheme preference; `NoPreference` means "decide yourself".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemePreference {
    /// No stated preference; the app falls back to its own default.
    #[default]
    NoPreference,
    /// The host prefers a dark scheme.
    PreferDark,
    /// The host prefers a light scheme.
    PreferLight,
}

impl ThemePreference {
    /// Whether this preference means dark, given a fallback for `NoPreference`.
    pub fn is_dark(self, fallback_dark: bool) -> bool {
        match self {
            Self::PreferDark => true,
            Self::PreferLight => false,
            Self::NoPreference => fallback_dark,
        }
    }
}

/// A source of the host's colour-scheme preference that can report changes.
pub trait ThemeWatcher {
    /// The most recently known preference.
    fn preference(&self) -> ThemePreference;

    /// The new preference if it changed since the last call, else `None`.
    fn poll(&mut self) -> Option<ThemePreference>;
}

/// A watcher that never changes; the sensible default where no host signal exists.
#[derive(Clone, Copy, Debug, Default)]
pub struct FixedTheme {
    pub preference: ThemePreference,
}

impl ThemeWatcher for FixedTheme {
    fn preference(&self) -> ThemePreference {
        self.preference
    }
    fn poll(&mut self) -> Option<ThemePreference> {
        None
    }
}
