//! MD3 shortcut field: an outlined box that captures the next chord and shows it.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{CursorShape, Decl, Id, KeyChord, Modifiers, NodeKind};
use ui_style::Style;
use ui_widgets::{Cx, Text, Widget, modifiers_label, shortcut_label};

use crate::paint::role;

/// The box's height, matching the text field it usually sits beside.
pub const SHORTCUT_FIELD_HEIGHT: f32 = 56.0;
/// `rounded_corners` on the MD3 shape scale.
const RADIUS: f32 = 4.0;
const PAD: f32 = 16.0;
const LABEL_SIZE: f32 = 16.0;

/// Everything a shortcut field is built with; the app owns the bound chord.
pub struct ShortcutFieldParams<'a> {
    pub id: Id,
    /// The chord currently bound; `None` shows the placeholder.
    pub chord: Option<KeyChord>,
    pub placeholder: &'a str,
    pub width: f32,
    pub disabled: bool,
}

/// One MD3 shortcut field; focus it and the next chord pressed is captured.
pub struct Md3ShortcutField {
    id: Id,
    chord: Option<KeyChord>,
    placeholder: String,
    width: f32,
    disabled: bool,
    outline: Color,
    accent: Color,
    ink: Color,
    hint: Color,
}

impl Md3ShortcutField {
    pub fn new(params: ShortcutFieldParams<'_>, pack: &Style) -> Self {
        Self {
            id: params.id,
            chord: params.chord,
            placeholder: params.placeholder.to_string(),
            width: params.width,
            disabled: params.disabled,
            outline: role(pack, "outline"),
            accent: role(pack, "primary"),
            ink: role(pack, "on_surface"),
            hint: role(pack, "on_surface_variant"),
        }
    }

    /// What the box reads right now: the bound chord, or the modifiers being held.
    fn shown(&self, focused: bool, held: Modifiers, cx: &Cx<'_, '_>) -> (String, bool) {
        if focused && !held.is_empty() {
            // Live preview: the chord under construction, before any key lands.
            let mut out = modifiers_label(held, cx);
            out.push('…');
            return (out, false);
        }
        match &self.chord {
            Some(chord) => (shortcut_label(chord, cx), false),
            None => (self.placeholder.clone(), true),
        }
    }
}

impl Widget for Md3ShortcutField {
    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let state = cx.state(self.id);
        let focused = state.focused;
        let (shown, is_hint) = self.shown(focused, state.modifiers, cx);
        let edge = if focused { self.accent } else { self.outline };
        let width = if focused { 2.0 } else { 1.0 };
        let text = Text::new(
            self.id.with("value"),
            shown,
            LABEL_SIZE,
            if is_hint { self.hint } else { self.ink },
        );
        Decl::container(self.id, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::fixed(SHORTCUT_FIELD_HEIGHT),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: PAD,
                top: 0.0,
                right: PAD,
                bottom: 0.0,
            })
            .background(Color::TRANSPARENT, Corners::all(RADIUS))
            .border(edge, width)
            // Capture, not navigation: the editor must see the chord it is about to bind.
            .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE | ui_core::Sense::CAPTURE_KEYS)
            .cursor(CursorShape::Text)
            .disabled(self.disabled)
            .child(text.resolve())
    }
}
