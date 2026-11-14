//! Cached declarations: what a build read, and whether it still reads the same.

use crate::collections::FastMap;
use std::rc::Rc;

use crate::geom::Point;
use crate::id::Id;
use crate::tree::Decl;
use crate::{AnimSpec, AnimStore};

/// One fact a build read, together with the value it saw.
#[derive(Clone, Debug, PartialEq)]
pub enum Dep {
    Hovered(Option<Id>),
    Pressed(Option<Id>),
    Focused(Option<Id>),
    FocusVisible(bool),
    /// The popup a press outside it dismissed this frame.
    Dismissed(Option<Id>),
    /// The chord that fired this frame and the node that declared it.
    Shortcut(Option<(Id, crate::shortcut::ShortcutId)>),
    Caret(Option<(Id, usize)>),
    CaretInText(Option<(Id, f32)>),
    Selection(Rc<[(Id, crate::selection::TextSelection)]>),
    Preedit(Option<(Id, crate::ime::Preedit)>),
    HoveredPath(Vec<Id>),
    PressedPath(Vec<Id>),
    HoveredWithin(Id, bool),
    PressedWithin(Id, bool),
    ClickedWithin(Id, bool),
    ActivatedWithin(Id, bool),
    DroppedWithin(Id, bool),
    /// A live drag is over this node or a descendant and the node takes what it offers.
    DragOverWithin(Id, bool),
    /// The MIME types the live drag offers.
    DragTypes(Option<Rc<Vec<String>>>),
    DraggingWithin(Id, bool),
    DragStarted(Id, bool),
    Disabled(Id, bool),
    Local(Option<Point>),
    PressOrigin(Option<Point>),
    DragStep(Option<Point>),
    /// The held modifiers, as four bits; a cached control must not keep a stale preview.
    Modifiers(u8),
    /// An animation call, replayed on validation so the instance keeps advancing.
    Anim {
        id: Id,
        target: f32,
        spec: AnimSpec,
        value: f32,
    },
    AnimColour {
        id: Id,
        target: crate::geom::Color,
        spec: AnimSpec,
        value: crate::geom::Color,
    },
    AnimStops {
        id: Id,
        stops: Vec<crate::anim::Stop>,
        spec: AnimSpec,
        value: f32,
    },
    Value {
        id: Id,
        value: f32,
    },
    /// A scroll container's geometry a build read, both axes: content extent, viewport,
    /// offset, and whether a wheel moved it since the previous frame.
    Scroll {
        id: Id,
        extent_w: f32,
        extent_h: f32,
        viewport_w: f32,
        viewport_h: f32,
        offset_x: f32,
        offset_y: f32,
        active: bool,
    },
    Status {
        id: Id,
        status: Option<crate::anim::Status>,
    },
    /// A time-driven read: the element wants every frame, and this one's clock at build.
    Age {
        id: Id,
        age: f32,
    },
    /// Bit patterns, so a value that only moves in the low bits still invalidates.
    Now(u64),
    Size(u32, u32),
    Scale(u32),
    Dt(u32),
    Frame(u64),
    Pointer(Option<Point>),
    /// A fact the engine cannot replay, so the entry never survives to the next frame.
    Volatile,
}

/// The stack of dependency frames currently being recorded.
#[derive(Default)]
pub struct Recorder {
    frames: Vec<Vec<Dep>>,
    /// Per recording frame: the ids of the element children built under it. Only these can
    /// have cache entries; a declaration child never does.
    kids: Vec<Vec<Id>>,
    /// Per recording frame: a child was rebuilt rather than reused, so this subtree changed.
    rebuilt_child: Vec<bool>,
    muted: bool,
    /// Facts read outside every recording frame this frame, so no entry can own them.
    unattributed: Vec<Dep>,
    /// Some live element asked for the clock this frame: the loop must keep running.
    time_driven: bool,
    /// Some live element read the raw pointer position: any move can change what it draws.
    pointer_read: bool,
    /// Some live element read the held modifiers: a modifier change can change what it
    /// draws, even with no key press.
    modifier_read: bool,
}

impl Recorder {
    pub fn begin(&mut self) {
        self.frames.push(Vec::new());
        self.kids.push(Vec::new());
        self.rebuilt_child.push(false);
    }

    pub fn end(&mut self) -> Vec<Dep> {
        self.rebuilt_child.pop();
        self.kids.pop();
        self.frames.pop().unwrap_or_default()
    }

    /// Close the frame: what it read, whether every element child was reused, and those
    /// children's ids.
    pub fn end_whole(&mut self) -> (Vec<Dep>, bool, Vec<Id>) {
        let rebuilt = self.rebuilt_child.pop().unwrap_or(true);
        let kids = self.kids.pop().unwrap_or_default();
        (self.frames.pop().unwrap_or_default(), !rebuilt, kids)
    }

    /// Record an element child built under the current frame, so validation can find it.
    pub fn note_element_child(&mut self, id: Id) {
        if let Some(top) = self.kids.last_mut() {
            top.push(id);
        }
    }

    /// Note that the child just built was rebuilt, not reused: its parent is no longer whole.
    pub fn note_rebuilt_child(&mut self) {
        if let Some(top) = self.rebuilt_child.last_mut() {
            *top = true;
        }
    }

    pub fn push(&mut self, dep: Dep) {
        if self.muted {
            return;
        }
        if matches!(dep, Dep::Age { .. } | Dep::Now(_)) {
            self.time_driven = true;
        }
        if matches!(dep, Dep::Pointer(_)) {
            self.pointer_read = true;
        }
        if matches!(dep, Dep::Modifiers(_)) {
            self.modifier_read = true;
        }
        match self.frames.last_mut() {
            Some(top) => top.push(dep),
            None => self.unattributed.push(dep),
        }
    }

    /// Whether a build in this frame asked for the clock; resets the flag.
    pub fn take_time_driven(&mut self) -> bool {
        std::mem::take(&mut self.time_driven)
    }

    /// Whether a build in this frame read the raw pointer position; resets the flag.
    pub fn take_pointer_read(&mut self) -> bool {
        std::mem::take(&mut self.pointer_read)
    }

    /// Whether a build in this frame read the held modifiers; resets the flag.
    pub fn take_modifier_read(&mut self) -> bool {
        std::mem::take(&mut self.modifier_read)
    }

    /// Facts read outside every build since the last call. They belong to no entry, so the
    /// cache can only keep itself when they read the same as last frame; `DeclCache::
    /// settle_outside` makes that call, once per frame, and takes them with it.
    pub fn take_unattributed(&mut self) -> Vec<Dep> {
        std::mem::take(&mut self.unattributed)
    }

    /// Silence recording while the cache re-reads its own dependencies.
    pub fn mute(&mut self) -> bool {
        std::mem::replace(&mut self.muted, true)
    }

    pub fn unmute(&mut self, was: bool) {
        self.muted = was;
    }
}

/// What an element offers to be cached under: its identity and a fingerprint of its own
/// inputs. The fingerprint is not a [`Dep`] — validation could not answer for it; only the
/// element knows what it was given this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedKey {
    pub id: Id,
    pub props: u64,
}

impl CachedKey {
    pub fn new(id: Id, props: u64) -> Self {
        Self { id, props }
    }
}

/// One cached declaration and the facts it was built from.
pub struct Entry {
    pub decl: Rc<Decl>,
    pub deps: Vec<Dep>,
    /// Fingerprint of the element's own inputs when this declaration was built.
    pub props: u64,
    /// Every element child was itself a validated entry, so this declaration is only still
    /// true while all of them still validate. A subtree with a rebuilt child is not whole.
    pub whole: bool,
    /// The element children this declaration was built with, in build order. A child that
    /// is a declaration inside it is not listed: it has no entry and its inputs are the
    /// entry's own.
    pub kids: Vec<Id>,
    /// Frame in which the entry was last validated, so a subtree is walked once per frame.
    pub checked: u64,
}

/// Declarations kept across frames, keyed by the root id they were built for.
#[derive(Default)]
pub struct DeclCache {
    entries: FastMap<Id, Entry>,
    hits: u64,
    misses: u64,
    /// Facts the app read outside every build last frame, in order.
    outside: Vec<Dep>,
    /// Whether `outside` is a reading rather than the initial emptiness.
    observed: bool,
}

/// Beyond this many entries the cache is dropped wholesale; ids are unbounded in principle.
const MAX_ENTRIES: usize = 8192;

impl DeclCache {
    pub fn get(&self, key: Id) -> Option<&Entry> {
        self.entries.get(&key)
    }

    pub fn contains(&self, key: Id) -> bool {
        self.entries.contains_key(&key)
    }

    pub fn insert(&mut self, key: Id, entry: Entry) {
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.clear();
        }
        self.entries.insert(key, entry);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Whether entries may stand, given what the app read outside every build this frame.
    ///
    /// Such a read cannot make any single entry stale — it belongs to none of them — but the
    /// app may have derived its own state from it, which later builds read without recording.
    /// So the cache survives exactly when the reads say what they said last frame.
    pub fn settle_outside(&mut self, now: &[Dep]) -> bool {
        // A fact the engine cannot replay can never be proven unchanged.
        let replayable = !now.iter().any(|d| matches!(d, Dep::Volatile));
        // The first reading has nothing to compare against, so it proves nothing.
        let held = self.observed && replayable && now == self.outside;
        self.outside.clear();
        self.outside.extend_from_slice(now);
        self.observed = true;
        held
    }

    /// Record that `key` passed validation this frame.
    pub fn mark_checked(&mut self, key: Id, frame: u64) {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.checked = frame;
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Record one lookup outcome, so a profile run can tell reuse from rebuilding.
    pub fn note(&mut self, hit: bool) {
        if hit {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
    }

    /// Entries held, and the running (hits, misses) totals.
    pub fn stats(&self) -> (usize, u64, u64) {
        (self.entries.len(), self.hits, self.misses)
    }
}

/// Whether a recorded animation call still reads the same value when replayed.
pub fn anim_still_matches(anim: &mut AnimStore, dep: &Dep, now: f64) -> bool {
    match dep {
        Dep::Anim {
            id,
            target,
            spec,
            value,
        } => anim.animate(*id, *target, *spec, now) == *value,
        Dep::AnimColour {
            id,
            target,
            spec,
            value,
        } => anim.animate_colour(*id, *target, *spec, now) == *value,
        Dep::AnimStops {
            id,
            stops,
            spec,
            value,
        } => anim.animate_stops(*id, stops, *spec, now) == *value,
        Dep::Value { id, value } => anim.value(*id, now).unwrap_or(0.0) == *value,
        // A time-driven element is stale as soon as the clock moves, by design.
        Dep::Age { id, age } => anim.age_of(*id, now).is_some_and(|a| a as f32 == *age),
        Dep::Status { id, status } => anim.status(*id, now) == *status,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_stand_only_while_the_apps_outside_reads_repeat() {
        let a = Id::of("a");
        let b = Id::of("b");
        let mut cache = DeclCache::default();
        let quiet = [Dep::Hovered(None), Dep::Pressed(None)];
        // The first reading has nothing to compare against, so it proves nothing.
        assert!(!cache.settle_outside(&quiet));
        assert!(cache.settle_outside(&quiet));
        assert!(!cache.settle_outside(&[Dep::Hovered(Some(a)), Dep::Pressed(None)]));
        assert!(!cache.settle_outside(&[Dep::Hovered(Some(b)), Dep::Pressed(None)]));
        // A fact the engine cannot replay never proves itself unchanged.
        assert!(!cache.settle_outside(&[Dep::Volatile]));
        assert!(!cache.settle_outside(&[Dep::Volatile]));
    }
}
