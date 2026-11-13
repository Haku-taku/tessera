//! Named slots: a container declares a fixed vocabulary, the app fills what it has.

use std::rc::Rc;

use crate::id::Id;
use crate::tree::Decl;

/// One slot's name, chosen by the container and fixed for its lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotName(&'static str);

impl SlotName {
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// The name as the container wrote it.
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// The salt this name contributes to its slot's node id.
    pub(crate) const fn salt(self) -> (&'static str, &'static str) {
        ("slot", self.0)
    }
}

/// The node id a named slot takes under `container`; derived, never app-chosen.
pub fn slot_id(container: Id, name: SlotName) -> Id {
    container.with(name.salt())
}

/// The app's fills, keyed by name; filling one name twice keeps the last.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlotFills {
    fills: Vec<(SlotName, Rc<Decl>)>,
}

impl SlotFills {
    pub const fn new() -> Self {
        Self { fills: Vec::new() }
    }

    /// Fill one slot; a later fill of the same name replaces the earlier one.
    pub fn fill(mut self, name: SlotName, content: impl Into<Rc<Decl>>) -> Self {
        self.insert(name, content);
        self
    }

    /// The in-place form of [`SlotFills::fill`], for a container merging in its defaults.
    pub fn insert(&mut self, name: SlotName, content: impl Into<Rc<Decl>>) {
        let content = content.into();
        match self.fills.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = content,
            None => self.fills.push((name, content)),
        }
    }

    pub fn get(&self, name: SlotName) -> Option<&Rc<Decl>> {
        self.fills.iter().find(|(n, _)| *n == name).map(|(_, d)| d)
    }

    pub fn is_empty(&self) -> bool {
        self.fills.is_empty()
    }

    pub fn len(&self) -> usize {
        self.fills.len()
    }

    /// The first fill naming a slot outside `vocabulary`, so a container can reject it.
    pub fn outside(&self, vocabulary: &[SlotName]) -> Option<SlotName> {
        self.fills
            .iter()
            .map(|(n, _)| *n)
            .find(|n| !vocabulary.contains(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SizeClass;
    use crate::tree::{NodeKind, Tree};

    const LEADING: SlotName = SlotName::new("leading");
    const TRAILING: SlotName = SlotName::new("trailing");

    fn leaf(id: &'static str, w: f32, h: f32) -> Decl {
        Decl::container(crate::id::Id::str(id), NodeKind::Box).size(SizeClass::fixed(w, h))
    }

    fn row() -> Decl {
        Decl::container(crate::id::Id::str("row"), NodeKind::Row)
    }

    #[test]
    fn a_slot_keeps_its_id_when_its_content_changes() {
        // A slot's identity is (container, name). If the id followed the content instead,
        // every content swap would retire the node and the element cache would never hold.
        let frame = |content: &'static str| row().slot(LEADING, leaf(content, 4.0, 4.0));
        let mut tree = Tree::new();
        tree.apply(&frame("a"));
        let slot = tree
            .find_by_id(super::slot_id(crate::id::Id::str("row"), LEADING))
            .expect("the slot node");
        let stats = tree.apply(&frame("b"));
        assert_eq!(
            tree.find_by_id(super::slot_id(crate::id::Id::str("row"), LEADING)),
            Some(slot),
            "the slot must keep its node when its content is replaced"
        );
        assert_eq!(stats.reused, 2, "the row and the slot survive the swap");
        assert_eq!(stats.created, 1, "only the new content node is created");
        assert_eq!(stats.retired, 1, "only the old content node is retired");
    }

    #[test]
    fn a_fill_outside_the_vocabulary_is_reported() {
        // The vocabulary is the container's; a name it never declared must be caught, not
        // silently placed under whatever position the app guessed.
        let fills = SlotFills::new()
            .fill(LEADING, leaf("a", 1.0, 1.0))
            .fill(SlotName::new("sideways"), leaf("b", 1.0, 1.0));
        assert_eq!(
            fills.outside(&[LEADING, TRAILING]),
            Some(SlotName::new("sideways"))
        );
        assert_eq!(fills.len(), 2);
    }
}
