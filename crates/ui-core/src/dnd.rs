//! Drag and drop: what a drag carries, and the phases an operating-system drag reports.
//!
//! [`crate::drag`] is the gesture — a press that becomes a movement. This module is the
//! payload and the vocabulary a drop target negotiates with, so an element can declare what
//! it offers and what it takes without knowing which platform moves the bytes.

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

/// What a drag carries, and what accepting it means.
#[derive(Clone, Debug, PartialEq)]
pub struct DragData {
    pub items: Vec<DragItem>,
    /// The effect a target performs when it takes this payload.
    pub effect: DropEffect,
}

impl DragData {
    /// A payload offering `items`, copied by a target that takes it.
    pub fn new(items: impl IntoIterator<Item = DragItem>) -> Self {
        Self {
            items: items.into_iter().collect(),
            effect: DropEffect::Copy,
        }
    }

    /// A payload with one representation, copied by a target that takes it.
    pub fn bytes(mime: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        Self::new([DragItem {
            mime: mime.into(),
            data: data.into(),
        }])
    }

    /// The same payload, performed as `effect` where it lands.
    pub fn with_effect(mut self, effect: DropEffect) -> Self {
        self.effect = effect;
        self
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
    /// A drag entered the surface, offering these types.
    Entered { pos: Point, types: Vec<String> },
    /// The drag moved within the surface.
    Over { pos: Point },
    /// The drag left the surface without dropping.
    Left,
    /// The drag was released on the surface; `data` is what the platform decoded.
    ///
    /// An operating-system drag reveals its payload only here, so a target highlights from
    /// `Entered` and reads the bytes from this.
    Dropped { pos: Point, data: DragData },
    /// A native drag this app started has ended; `dropped` is whether a target took it.
    Finished { dropped: bool },
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
}
