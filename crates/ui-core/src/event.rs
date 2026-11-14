//! Input events, kept as plain data so every backend can produce them.

use crate::geom::Point;

/// Which pointer button an event refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

/// Keys the framework names itself; text input arrives as `Text`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Escape,
    Enter,
    /// Space: activates like Enter on most toolkits.
    Space,
    Tab,
    Backspace,
    /// Forward delete: removes the character after the caret.
    Delete,
    /// Insert: overtype in an editor that has it, and a free key for an app's own command.
    Insert,
    Home,
    End,
    /// Page Up: a screenful towards the start of a list or document.
    PageUp,
    /// Page Down: a screenful towards the end.
    PageDown,
    Left,
    Right,
    Up,
    Down,
    /// Print Screen: the screenshot key, which never types text.
    Print,
    /// A function key, 1 through 24.
    Function(u8),
    /// A character pressed with a command modifier (Ctrl/Alt/Logo), so it is a command and
    /// not text: the platform sends it here instead of `Text`, and nothing gets inserted.
    Char(char),
}

/// Held modifier keys, reported whenever the set changes.
///
/// Ambient rather than per-key: a key press does not say which modifiers were down,
/// because the platform reports them separately and a client may miss either.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
}

impl Modifiers {
    /// Nothing held, for a chord that must match a bare key.
    pub const NONE: Self = Self {
        shift: false,
        ctrl: false,
        alt: false,
        logo: false,
    };

    /// No modifier held.
    pub fn is_empty(&self) -> bool {
        !(self.shift || self.ctrl || self.alt || self.logo)
    }

    /// The set as a bit per modifier: `shift`, `ctrl`, `alt`, `logo` in that order.
    ///
    /// A chord that leaves a modifier free needs set arithmetic, which four booleans
    /// cannot state.
    pub const fn bits(&self) -> u8 {
        self.shift as u8 | (self.ctrl as u8) << 1 | (self.alt as u8) << 2 | (self.logo as u8) << 3
    }

    /// Every modifier held by either set.
    pub const fn union(self, other: Self) -> Self {
        Self {
            shift: self.shift || other.shift,
            ctrl: self.ctrl || other.ctrl,
            alt: self.alt || other.alt,
            logo: self.logo || other.logo,
        }
    }
}

/// One input event.
///
/// Positions are logical units, the same space as layout, so hit testing needs no conversion.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    PointerMoved {
        pos: Point,
    },
    /// The pointer left the surface; nothing is hovered afterwards.
    PointerLeft,
    PointerPressed {
        pos: Point,
        button: PointerButton,
    },
    PointerReleased {
        pos: Point,
        button: PointerButton,
    },
    /// A committed character or string, already decoded by the platform.
    Text(String),
    /// Clipboard text the platform read for a paste request.
    ///
    /// Separate from `Text`: a paste replaces the selection as one edit, and an app that
    /// filters typed characters must not filter it the same way.
    Paste(String),
    KeyPressed {
        key: Key,
    },
    KeyReleased {
        key: Key,
    },
    /// A wheel or trackpad step, in pixels per axis; a plain wheel reports `y` only.
    Scroll {
        pos: Point,
        delta: Point,
    },
    /// The set of held modifiers changed.
    Modifiers(Modifiers),
    /// The input method's composition changed; `None` ends it without committing.
    Preedit(Option<crate::ime::Preedit>),
    /// The input method asks for the bytes before and after the caret to be removed.
    DeleteSurrounding {
        before: usize,
        after: usize,
    },
}

impl Event {
    /// A vertical wheel step, the shape every plain mouse reports.
    pub fn wheel(pos: Point, dy: f32) -> Self {
        Event::Scroll {
            pos,
            delta: Point::new(0.0, dy),
        }
    }

    /// Pointer position this event carries, if any.
    pub fn pos(&self) -> Option<Point> {
        match self {
            Event::PointerMoved { pos }
            | Event::PointerPressed { pos, .. }
            | Event::PointerReleased { pos, .. }
            | Event::Scroll { pos, .. } => Some(*pos),
            Event::PointerLeft | Event::Text(_) | Event::KeyPressed { .. } => None,
            Event::KeyReleased { .. } | Event::Modifiers(_) => None,
            Event::Preedit(_) | Event::DeleteSurrounding { .. } | Event::Paste(_) => None,
        }
    }

    /// True for a pure position report: a higher rate of these cannot add anything.
    ///
    /// A wheel is not one of these: it changes a scroller's offset, so the frame has to be
    /// rebuilt. Only a move that cannot change a pixel may be skipped.
    pub fn is_continuous(&self) -> bool {
        matches!(self, Event::PointerMoved { .. })
    }

    /// True when this event can change what is hovered.
    pub fn moves_pointer(&self) -> bool {
        matches!(
            self,
            Event::PointerMoved { .. } | Event::PointerLeft | Event::PointerPressed { .. }
        )
    }
}
