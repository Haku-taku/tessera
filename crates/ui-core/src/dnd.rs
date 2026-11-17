//! Drag and drop: what a drag carries, and the phases an operating-system drag reports.
//!
//! [`crate::drag`] is the gesture — a press that becomes a movement. This module is the
//! payload and the vocabulary a drop target negotiates with, so an element can declare what
//! it offers and what it takes without knowing which platform moves the bytes.

use crate::event::{Modifiers, PointerButton};
use crate::geom::Point;

/// One payload a drag carries: a MIME type and its bytes.
///
/// The representation is the same one the clipboard uses, so a value can move between the
/// two without a conversion table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragItem {
    pub mime: String,
    pub data: Vec<u8>,
}

/// What a drag carries, and what accepting it may mean.
#[derive(Clone, Debug, PartialEq)]
pub struct DragData {
    pub items: Vec<DragItem>,
    /// The effects a target may perform with this payload.
    ///
    /// A set rather than one effect: the source states what the data can mean, and the
    /// target (and the modifiers held at the drop) pick from it.
    pub effects: DropEffects,
    /// The effect a target performs when the user holds no modifier.
    pub preferred: DropEffect,
}

impl DragData {
    /// A payload offering `items`, copied by a target that takes it.
    pub fn new(items: impl IntoIterator<Item = DragItem>) -> Self {
        Self {
            items: items.into_iter().collect(),
            effects: DropEffects::COPY,
            preferred: DropEffect::Copy,
        }
    }

    /// A payload with one representation, copied by a target that takes it.
    pub fn bytes(mime: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        Self::new([DragItem {
            mime: mime.into(),
            data: data.into(),
        }])
    }

    /// The same payload, permitted and preferred as exactly `effect`.
    pub fn with_effect(mut self, effect: DropEffect) -> Self {
        self.effects = DropEffects::from_effect(effect);
        self.preferred = effect;
        self
    }

    /// The same payload, permitted as `effects` and preferring `preferred`.
    ///
    /// A preferred effect outside the set falls back to the first permitted one, so the
    /// two fields can never disagree.
    pub fn with_effects(mut self, effects: DropEffects, preferred: DropEffect) -> Self {
        self.preferred = if effects.contains(preferred) {
            preferred
        } else {
            effects.iter().next().unwrap_or(DropEffect::Copy)
        };
        self.effects = effects;
        self
    }

    /// The same payload, preferring `effect` within the effects it already permits.
    pub fn preferring(mut self, effect: DropEffect) -> Self {
        self.effects = self.effects.union(DropEffects::from_effect(effect));
        self.preferred = effect;
        self
    }

    /// The effect this payload means with `modifiers` held, or `None` when it permits none.
    pub fn negotiate(&self, modifiers: Modifiers) -> Option<DropEffect> {
        negotiate_effect(self.effects, self.preferred, modifiers)
    }

    /// The MIME types this payload offers, best first.
    pub fn types(&self) -> impl Iterator<Item = &str> {
        self.items.iter().map(|item| item.mime.as_str())
    }

    /// The bytes for `mime`, by exact type.
    pub fn item(&self, mime: &str) -> Option<&[u8]> {
        self.items
            .iter()
            .find(|item| item.mime.eq_ignore_ascii_case(mime))
            .map(|item| item.data.as_slice())
    }

    /// The payload as text, when it offers `text/plain`.
    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(self.item("text/plain")?).ok()
    }

    /// Whether any offered type matches what a target accepts.
    pub fn accepted_by(&self, accepted: &[String]) -> bool {
        offered_matches(accepted, self.types())
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// What a drop does to the payload: copy it, move it, or link to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DropEffect {
    #[default]
    Copy,
    Move,
    Link,
}

impl DropEffect {
    /// The bit this effect occupies in [`DropEffects`].
    pub const fn bit(self) -> u8 {
        match self {
            DropEffect::Copy => 1,
            DropEffect::Move => 2,
            DropEffect::Link => 4,
        }
    }
}

/// A set of effects a payload permits.
///
/// The protocol sides are bitmasks (`DROPEFFECT_*`, `wl_data_device_manager.dnd_action`);
/// keeping the set in the engine lets both speak one vocabulary and keeps "this payload
/// may be copied or moved" expressible without inventing a fourth `DropEffect`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DropEffects(u8);

impl DropEffects {
    /// No effect at all: a payload no target may take.
    pub const NONE: Self = Self(0);
    pub const COPY: Self = Self(DropEffect::Copy.bit());
    pub const MOVE: Self = Self(DropEffect::Move.bit());
    pub const LINK: Self = Self(DropEffect::Link.bit());
    /// Every effect this engine knows.
    pub const ALL: Self =
        Self(DropEffect::Copy.bit() | DropEffect::Move.bit() | DropEffect::Link.bit());

    /// The set holding exactly `effect`.
    pub const fn from_effect(effect: DropEffect) -> Self {
        Self(effect.bit())
    }

    /// Whether `effect` is permitted.
    pub const fn contains(self, effect: DropEffect) -> bool {
        self.0 & effect.bit() != 0
    }

    /// Whether nothing is permitted.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Both sets' effects.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The permitted effects, in Copy, Move, Link order.
    pub fn iter(self) -> impl Iterator<Item = DropEffect> {
        [DropEffect::Copy, DropEffect::Move, DropEffect::Link]
            .into_iter()
            .filter(move |effect| self.contains(*effect))
    }
}

impl From<DropEffect> for DropEffects {
    fn from(effect: DropEffect) -> Self {
        Self::from_effect(effect)
    }
}

impl std::ops::BitOr for DropEffects {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        self.union(other)
    }
}

impl std::ops::BitOrAssign for DropEffects {
    fn bitor_assign(&mut self, other: Self) {
        *self = self.union(other);
    }
}

/// The effect a payload with `effects` permits and `preferred` prefers, given the
/// modifiers held during the drag.
///
/// This is the wayland convention, which matches Windows apart from the Command key:
/// Ctrl copies, Shift moves, Ctrl+Shift links, and no modifier means the source's own
/// preference. A modifier asks for an effect the payload does not permit; the preferred
/// effect is the answer then, because the source's intent outranks a modifier that cannot
/// be honoured. `None` means the payload permits nothing.
pub fn negotiate_effect(
    effects: DropEffects,
    preferred: DropEffect,
    modifiers: Modifiers,
) -> Option<DropEffect> {
    let wanted = match (modifiers.ctrl, modifiers.shift) {
        (true, true) => DropEffect::Link,
        (false, true) => DropEffect::Move,
        (true, false) => DropEffect::Copy,
        (false, false) => preferred,
    };
    if effects.contains(wanted) {
        Some(wanted)
    } else if effects.contains(preferred) {
        Some(preferred)
    } else {
        effects.iter().next()
    }
}

/// What a drop target was told about the release that landed on it.
///
/// One value instead of five `Change::DroppedOn` fields: the target almost always reads
/// the payload, and only the menus care about the rest.
#[derive(Clone, Debug, PartialEq)]
pub struct DropReport {
    /// The payload as the platform decoded it, or as the source handed it over.
    pub data: DragData,
    /// The effect the release means, already negotiated with the modifiers.
    pub effect: DropEffect,
    /// The modifiers held at the release.
    pub modifiers: Modifiers,
    /// The button that started the drag; `Secondary` is a right drag, which asks.
    pub button: PointerButton,
    /// Which side carries the effect out. A Wayland target must copy a move and leave the
    /// originals to the source's own cleanup; a Windows target moves them itself.
    pub performer: Performer,
    /// Whether the drag began in another application.
    ///
    /// A payload from this app states its own preference; another application's says only
    /// which effects it permits, so a target's own rule — same volume moves, otherwise
    /// copies — fills the gap.
    pub foreign: bool,
    /// Where the release landed, in surface coordinates.
    pub pos: Point,
}

/// How a drag this app started ended, as the platform reports it.
///
/// The source needs both halves: a Wayland move is the *source's* to perform after
/// `dnd_finished`, while a Windows or in-app move is already done by the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropOutcome {
    /// The final action; the target's answer, not the source's preference.
    pub action: DropEffect,
    /// Who carried it out.
    pub performed_by: Performer,
}

/// Which side performed a [`DropOutcome`]'s action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Performer {
    /// The drop target moved, copied or linked the data; the source does nothing.
    Target,
    /// The source must carry the action out itself (Wayland hands a move to the source).
    Source,
}

/// Whether one MIME pattern accepts one offered type.
///
/// `"*"` and `"*/*"` take anything, `"text/*"` takes a whole family, and anything else is an
/// exact type. Types compare ASCII-insensitively, as MIME types do.
pub fn mime_matches(pattern: &str, offered: &str) -> bool {
    if pattern == "*" || pattern == "*/*" {
        return true;
    }
    match pattern.strip_suffix("/*") {
        Some(family) => {
            offered.len() > family.len()
                && offered.as_bytes().get(family.len()) == Some(&b'/')
                && offered[..family.len()].eq_ignore_ascii_case(family)
        }
        None => pattern.eq_ignore_ascii_case(offered),
    }
}

/// Whether any offered type satisfies any accepted pattern.
pub fn offered_matches<'a>(accepted: &[String], offered: impl Iterator<Item = &'a str>) -> bool {
    offered
        .into_iter()
        .any(|ty| accepted.iter().any(|pattern| mime_matches(pattern, ty)))
}

/// One phase of a drag that crosses a surface, as a platform reports it.
///
/// An app's own drag does not produce these: the engine derives its phases from the pointer.
/// A native drag from another window does, in this order: `Entered`, `Over` (many), then
/// either `Dropped` or `Left`.
#[derive(Clone, Debug, PartialEq)]
pub enum DndEvent {
    /// A drag entered the surface, offering these types and permitting these effects.
    Entered {
        pos: Point,
        types: Vec<String>,
        effects: DropEffects,
    },
    /// The drag moved within the surface.
    Over { pos: Point },
    /// The drag left the surface without dropping.
    Left,
    /// The drag was released on the surface; `data` is what the platform decoded.
    ///
    /// An operating-system drag reveals its payload only here, so a target highlights from
    /// `Entered` and reads the bytes from this. `button` is the one that started the drag:
    /// a release reported while it was still held names `Secondary` for a right drag.
    /// `performer` says which side carries the action out: on Windows the target performs a
    /// move itself, while on Wayland the source finishes it after `dnd_finished`.
    Dropped {
        pos: Point,
        data: DragData,
        button: PointerButton,
        performer: Performer,
    },
    /// A native drag this app started has ended.
    ///
    /// `None` means no target took it. Otherwise the action and the side that performed
    /// it are both named, because the two platforms split the work differently.
    Finished { outcome: Option<DropOutcome> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_family_pattern_takes_its_members_and_nothing_neighboring() {
        // The whole point of a pattern is that `text/*` must not swallow `textual/plain`,
        // and `*` must.
        assert!(mime_matches("text/*", "text/plain"));
        assert!(
            mime_matches("TEXT/*", "text/plain"),
            "MIME is case-insensitive"
        );
        assert!(!mime_matches("text/*", "textual/plain"));
        assert!(!mime_matches("text/*", "image/png"));
        assert!(mime_matches("*", "image/png"));
        assert!(mime_matches("*/*", "image/png"));
        assert!(mime_matches("text/plain", "text/plain"));
        assert!(!mime_matches("text/plain", "text/html"));
    }

    #[test]
    fn a_payload_is_taken_only_by_a_target_that_accepts_one_of_its_types() {
        let data = DragData::bytes("text/uri-list", b"file:///a.txt".to_vec());
        assert!(data.accepted_by(&["text/*".to_string()]));
        assert!(data.accepted_by(&["text/uri-list".to_string()]));
        assert!(!data.accepted_by(&["image/png".to_string()]));
        assert!(data.accepted_by(&["*".to_string()]));
        assert_eq!(data.item("text/uri-list"), Some(&b"file:///a.txt"[..]));
        assert_eq!(data.item("TEXT/URI-LIST"), Some(&b"file:///a.txt"[..]));
    }

    #[test]
    fn modifiers_pick_an_effect_the_payload_permits() {
        // The modifier table is the whole negotiation: none means the source's preference,
        // Ctrl copies, Shift moves and Ctrl+Shift links. Each choice is clamped to the set
        // the source permits, so a text payload cannot be "moved" by holding Shift.
        use crate::event::Modifiers;
        let files = DragData::bytes("text/uri-list", Vec::new())
            .with_effects(DropEffects::ALL, DropEffect::Move);
        let none = Modifiers::NONE;
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let both = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        };
        assert_eq!(files.negotiate(none), Some(DropEffect::Move));
        assert_eq!(files.negotiate(ctrl), Some(DropEffect::Copy));
        assert_eq!(files.negotiate(shift), Some(DropEffect::Move));
        assert_eq!(files.negotiate(both), Some(DropEffect::Link));

        let copy_only = DragData::bytes("text/plain", Vec::new());
        assert_eq!(copy_only.negotiate(shift), Some(DropEffect::Copy));

        let nothing = DragData::bytes("text/plain", Vec::new())
            .with_effects(DropEffects::NONE, DropEffect::Copy);
        assert_eq!(nothing.negotiate(none), None);
    }

    #[test]
    fn a_preferred_effect_outside_the_set_falls_back_to_a_permitted_one() {
        // The two fields describe one payload; a builder that let them disagree would make
        // the pointer show an effect the drop then refuses.
        let data = DragData::bytes("text/plain", Vec::new())
            .with_effects(DropEffects::LINK, DropEffect::Move);
        assert_eq!(data.preferred, DropEffect::Link);
        assert_eq!(data.effects, DropEffects::LINK);
        let data = data.preferring(DropEffect::Copy);
        assert_eq!(data.preferred, DropEffect::Copy);
        assert!(data.effects.contains(DropEffect::Copy));
        assert!(data.effects.contains(DropEffect::Link));
    }
}
