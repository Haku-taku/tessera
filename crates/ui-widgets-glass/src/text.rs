//! The one label shape every control in this pack draws.

use ui_core::geom::Color;
use ui_core::{Decl, Id};
use ui_style::{Style, TextRole};

/// A label resolved to scalars, so `decl()` needs no style pack.
#[derive(Clone)]
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

    /// The same label scaled about its own type size, for the magnified tab.
    pub(crate) fn scaled(&self, k: f32) -> Self {
        Self {
            size: self.size * k,
            ..self.clone()
        }
    }

    /// The same label at an explicit size and weight, for a control the type ramp lacks.
    pub(crate) fn restyled(&self, size: f32, weight: ui_core::Weight) -> Self {
        Self {
            size,
            weight,
            ..self.clone()
        }
    }

    /// The same label in another ink, for a selected/unselected crossfade.
    pub(crate) fn inked(&self, color: Color) -> Self {
        Self {
            color,
            ..self.clone()
        }
    }

    pub(crate) fn size(&self) -> f32 {
        self.size
    }

    pub(crate) fn weight(&self) -> ui_core::Weight {
        self.weight
    }

    /// The box this label needs, so a caller can centre a label beside an icon.
    pub(crate) fn height(&self) -> f32 {
        self.size * ui_widgets::LINE_HINT
    }

    /// The width this label takes on one line, measured as layout will measure it.
    pub(crate) fn width_in(&self, cx: &ui_widgets::Cx<'_, '_>) -> Option<f32> {
        ui_widgets::Text::new(
            Id::str("glass/measure"),
            self.text.clone(),
            self.size,
            self.color,
        )
        .weight(self.weight)
        .width_in(cx)
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
