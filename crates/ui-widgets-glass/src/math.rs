//! Glass formulae: the pack's body ink and size around the shared math control.

use ui_core::geom::Color;
use ui_core::{Decl, Id};
use ui_style::{Style, TextRole};
use ui_widgets::{Cx, Math, MathErrorStyle, Widget};

use crate::paint::role_of;

/// Cupertino's colours for a source the layout refused.
fn error_style(pack: &Style) -> MathErrorStyle {
    MathErrorStyle {
        border: role_of(pack, "systemRed"),
        panel: role_of(pack, "glassFieldFill"),
        panel_ink: role_of(pack, "textFillColorPrimary"),
    }
}

/// A formula set in Glass's body size and primary text ink.
///
/// The HIG has no math control, so the pack contributes the text role and its ink; the face
/// and the layout stay the host's, through `ui_widgets::Math`.
pub struct GlassMath {
    math: Math,
}

impl GlassMath {
    pub fn new(id: Id, source: impl Into<String>, pack: &Style) -> Self {
        Self {
            math: Math::new(
                id,
                source,
                pack.type_table.get(TextRole::Body).size,
                role_of(pack, "textFillColorPrimary"),
            )
            .error(error_style(pack)),
        }
    }

    /// Builder: another size from the type ramp.
    pub fn size(mut self, size: f32) -> Self {
        self.math.size = size;
        self
    }

    /// Builder: an ink other than the primary text ink.
    pub fn ink(mut self, ink: Color) -> Self {
        self.math.ink = ink;
        self
    }

    /// Builder: the math face, by family name; `None` asks the host for its default.
    pub fn family(mut self, family: impl Into<String>) -> Self {
        self.math = self.math.family(family);
        self
    }

    /// Builder: display style rather than inline style.
    pub fn display(mut self) -> Self {
        self.math = self.math.display();
        self
    }

    /// Builder: edit the formula by structure rather than reading it.
    pub fn editable(mut self) -> Self {
        self.math = self.math.editable();
        self
    }

    pub fn resolve_in(&self, cx: &Cx<'_, '_>) -> Decl {
        self.math.resolve_in(cx)
    }

    /// This formula as an inline object, for a paragraph that embeds it.
    pub fn object_in(&self, cx: &Cx<'_, '_>) -> Option<ui_core::InlineObject> {
        self.math.object_in(cx)
    }
}

impl Widget for GlassMath {
    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        self.math.decl(cx)
    }
}
