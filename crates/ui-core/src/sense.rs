//! What kinds of interaction a node is sensitive to.

/// A set of interaction kinds, as a bit set.
///
/// Unlike egui's `Sense`, hover is an ordinary flag: some surfaces genuinely must not
/// react to the pointer, and treating it as an always-on floor would leave them no way
/// to say so.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Sense(u16);

impl Sense {
    /// Nothing at all: the node is inert for routing purposes.
    pub const NONE: Sense = Sense(0);
    /// The pointer is over this node.
    pub const HOVER: Sense = Sense(1 << 0);
    /// A press that never became a drag is delivered as an activation.
    pub const CLICK: Sense = Sense(1 << 1);
    /// The node may be the origin of a drag gesture.
    pub const DRAG: Sense = Sense(1 << 2);
    /// The node may accept a dropped item.
    pub const DROP: Sense = Sense(1 << 3);
    /// The node can hold keyboard focus.
    pub const FOCUSABLE: Sense = Sense(1 << 4);
    /// The node is a scroll container: wheel events route to it, not past it.
    pub const SCROLL: Sense = Sense(1 << 5);
    /// Hit testing passes through this node to whatever is beneath it.
    pub const PASS_THROUGH: Sense = Sense(1 << 6);
    /// A text node whose characters can be selected with the pointer.
    ///
    /// Opt-in: a label that defaulted to this would swallow the press of the control it
    /// sits in, and a text field's own text would start a read-only selection instead of
    /// taking focus.
    pub const SELECT: Sense = Sense(1 << 7);

    /// Takes raw chords while focused instead of letting the shortcut table read them.
    ///
    /// For a shortcut editor: it must see the very chord it is about to bind.
    pub const CAPTURE_KEYS: Sense = Sense(1 << 8);

    /// A scroll container whose offset the engine owns: a wheel routes here and the engine
    /// accumulates it into the node's `scroll_y`, clamped, instead of reporting `Value` up.
    pub const SCROLL_OWNED: Sense = Sense(1 << 9);

    /// The secondary button's press and release are reported here, as `Change::Secondary`.
    ///
    /// Opt-in: the button otherwise belongs to the engine's menu, which opens on the press.
    /// A node that asks for it moves that menu to the release of a short tap instead, so a
    /// held press can mean something else.
    pub const SECONDARY: Sense = Sense(1 << 10);

    /// The middle button's press and release are reported here, as `Change::Middle`.
    ///
    /// Opt-in like [`SECONDARY`](Self::SECONDARY): the button otherwise reaches no node,
    /// because the engine has no gesture of its own on it.
    pub const MIDDLE: Sense = Sense(1 << 11);

    /// A bit-wise union, so subscriptions compose with `|`.
    pub const fn union(self, other: Sense) -> Sense {
        Sense(self.0 | other.0)
    }

    /// Whether every flag in `other` is present.
    pub const fn contains(self, other: Sense) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether any flag in `other` is present; the cheap "is this subtree interesting".
    pub const fn any(self, other: Sense) -> bool {
        self.0 & other.0 != 0
    }

    /// The raw bits, for storage in a node.
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Rebuild from raw bits.
    pub const fn from_bits(bits: u16) -> Sense {
        Sense(bits)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for Sense {
    type Output = Sense;
    fn bitor(self, rhs: Sense) -> Sense {
        Sense(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for Sense {
    fn bitor_assign(&mut self, rhs: Sense) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for Sense {
    type Output = Sense;
    fn bitand(self, rhs: Sense) -> Sense {
        Sense(self.0 & rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subscription_composes_with_or_and_reports_membership() {
        // The point of a bit set: a control states its needs in one expression.
        let slider = Sense::HOVER | Sense::DRAG | Sense::FOCUSABLE;
        assert!(slider.contains(Sense::DRAG));
        assert!(!slider.contains(Sense::CLICK), "never asked for clicks");
        assert!(
            slider.contains(Sense::HOVER | Sense::DRAG),
            "a compound check needs every bit"
        );
        assert!(
            slider.any(Sense::CLICK | Sense::DRAG),
            "any-ness is separate"
        );
        // A half-satisfied compound must fail `contains` but pass `any`; without this the
        // two could be the same function and no test would notice.
        assert!(
            !slider.contains(Sense::DRAG | Sense::DROP),
            "drag alone is not drag+drop"
        );
        assert!(
            slider.any(Sense::DRAG | Sense::DROP),
            "but drag alone is enough for any"
        );
    }

    #[test]
    fn an_inert_node_says_so_instead_of_accepting_everything() {
        // Hover is an ordinary flag here, so "nothing" is expressible.
        assert!(Sense::NONE.is_empty());
        assert!(!Sense::NONE.contains(Sense::HOVER));
        assert!(Sense::HOVER.contains(Sense::HOVER));
        assert_ne!(Sense::NONE, Sense::HOVER);
    }
}
