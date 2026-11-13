//! The one label shape every control in this pack draws.

use ui_core::geom::Color;
use ui_core::{Decl, Id};
use ui_style::{Style, TextRole};

/// A label resolved to scalars, so `decl()` needs no style pack.
pub(crate) struct LabelText {
    text: String,
    size: f32,
    color: Color,
    weight: ui_core::Weight,
    max_width: f32,
    line_gap: f32,
    indent: f32,
}

impl LabelText {
    pub(crate) fn new(
        pack: &Style,
        role: TextRole,
        text: &str,
        color: Color,
        max_width: f32,
    ) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        let t = pack.type_table.get(role);
        Some(Self {
            text: text.into(),
            size: t.size,
            color,
            weight: t.weight,
            max_width,
            line_gap: t.line_gap,
            indent: t.indent,
        })
    }

    pub(crate) fn decl(&self, id: Id) -> Decl {
        ui_widgets::text::leaf(
            id,
            self.text.clone(),
            self.size,
            self.color,
            self.weight,
            self.max_width,
            self.line_gap,
            self.indent,
        )
    }
}
