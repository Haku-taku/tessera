//! MD3 badge: a 6dp dot, or a 16dp pill carrying a short label.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeKind, Weight};
use ui_style::Style;
use ui_widgets::{Cx, Widget};

use crate::paint::role;
use crate::spec::badge_spec;

/// One badge, resolved from the exported `md.comp.badge` tokens.
pub struct Md3Badge {
    id: Id,
    label: Option<String>,
    container: Color,
    label_colour: Color,
    label_size: f32,
    label_weight: Weight,
    dot: f32,
    pill_height: f32,
}

impl Md3Badge {
    /// The dot: no label, `smallSize` across, error-coloured.
    pub fn dot(id: Id, pack: &Style) -> Self {
        Self::new(id, None, pack)
    }

    /// The pill: a label inside the large badge's 16dp container.
    pub fn with_label(id: Id, label: impl Into<String>, pack: &Style) -> Self {
        Self::new(id, Some(label.into()), pack)
    }

    fn new(id: Id, label: Option<String>, pack: &Style) -> Self {
        let large = label.is_some();
        let s = badge_spec(large);
        Self {
            id,
            label,
            container: role(pack, s.root.color),
            // The dot has no label, and its spec carries no label role; resolving "" would panic.
            label_colour: if large {
                role(pack, s.label_text.color)
            } else {
                Color::TRANSPARENT
            },
            label_size: s.label_text.size,
            label_weight: Weight(s.label_text.weight as u16),
            dot: s.root.size,
            // `large.size` is the pill's height; the label sits inside it.
            pill_height: s.root.size,
        }
    }
}

impl Widget for Md3Badge {
    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let Some(label) = &self.label else {
            return Decl::container(self.id, NodeKind::Box)
                .size(SizeClass::fixed(self.dot, self.dot))
                .background(self.container, Corners::all(self.dot * 0.5));
        };
        // `Text` reports `size * 1.4` as its line box; the pill pads the difference evenly.
        let vertical_inset = ((self.pill_height - self.label_size * 1.4) * 0.5).max(0.0);
        // A badge is not a control: it never takes input, so it declares no `Sense`.
        Decl::container(self.id, NodeKind::Row)
            .size(SizeClass {
                // `minWidth: largeSize`: a one-character label still reads as a pill.
                width: AxisSize::hug(self.pill_height, 0.0),
                height: AxisSize::fixed(self.pill_height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // 4dp each side; the vertical inset centres the label's line box in the pill.
            .padding(Insets {
                left: 4.0,
                top: vertical_inset,
                right: 4.0,
                bottom: vertical_inset,
            })
            .background(self.container, Corners::all(self.pill_height * 0.5))
            .child(ui_widgets::text::leaf(
                self.id.with("label"),
                label.clone(),
                self.label_size,
                self.label_colour,
                self.label_weight,
                f32::INFINITY,
                0.0,
                0.0,
            ))
    }
}
