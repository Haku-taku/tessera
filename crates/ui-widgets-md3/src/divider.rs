//! MD3 divider: a hairline that fills the cross axis.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeKind};
use ui_style::Style;
use ui_widgets::{Cx, Widget};

use crate::paint::role;
use crate::spec::divider_spec;

/// Which way the hairline runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DividerAxis {
    Horizontal,
    Vertical,
}

/// One divider, resolved from the exported `md.comp.divider` tokens.
pub struct Md3Divider {
    id: Id,
    axis: DividerAxis,
    inset: Insets,
    colour: Color,
    thickness: f32,
}

impl Md3Divider {
    pub fn new(id: Id, axis: DividerAxis, inset: Insets, pack: &Style) -> Self {
        let s = divider_spec();
        Self {
            id,
            axis,
            inset,
            colour: role(pack, s.root.color),
            thickness: s.root.thickness.max(0.0),
        }
    }
}

impl Widget for Md3Divider {
    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let horizontal = self.axis == DividerAxis::Horizontal;
        let size = if horizontal {
            SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.thickness),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            }
        } else {
            SizeClass {
                width: AxisSize::fixed(self.thickness),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::Yes,
            }
        };
        let line = Decl::container(self.id, NodeKind::Box)
            .size(size)
            .background(self.colour, Corners::ZERO);
        if self.inset == Insets::ZERO {
            return line;
        }
        // Padding shrinks the child, never the background, so an inset hairline needs a box.
        Decl::container(self.id, NodeKind::Box)
            .size(size)
            .padding(self.inset)
            .child(
                Decl::container(self.id.with("line"), NodeKind::Box)
                    .size(SizeClass {
                        width: AxisSize::fill(0.0, 0.0),
                        height: AxisSize::fill(0.0, 0.0),
                        grow_x: Grow::Yes,
                        grow_y: Grow::Yes,
                    })
                    .background(self.colour, Corners::ZERO),
            )
    }
}
