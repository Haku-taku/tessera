//! The glass text field: one editable line on a glass face, with an icon at either end.

use std::cell::Cell;

use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, OverlayAlign, SizeClass};
use ui_core::{AnimSpec, Change, CursorShape, Decl, Easing, Id, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::text::LineEdit;
use ui_widgets::{Cx, IconSource, Widget};

use crate::paint::role_of;
use crate::shared::focus_ring;

/// Everything a text field is built with; the app owns the buffer.
pub struct GlassTextFieldParams<'a> {
    pub id: Id,
    pub text: &'a str,
    pub placeholder: &'a str,
    /// The box's width; a single line needs it before layout to measure its own scroll.
    pub width: f32,
    pub leading: Option<IconSource>,
    pub trailing: Option<IconSource>,
    /// Whether the trailing icon is on screen; the field cross-fades it over 180 ms.
    pub trailing_shown: bool,
    /// The trailing icon is a button: a click reports `Selected(Some(trailing id))`.
    pub trailing_action: bool,
    pub trailing_size: f32,
    /// A fixed height (the search variant's 44); `None` hugs the line plus padding.
    pub height: Option<f32>,
    pub padding: Insets,
    pub icon_spacing: f32,
    pub radius: f32,
    pub disabled: bool,
    /// A secret buffer: drawn as marks, not copyable, and hidden from the input method.
    pub secure: bool,
}

impl<'a> GlassTextFieldParams<'a> {
    pub fn new(id: Id, text: &'a str, width: f32) -> Self {
        Self {
            id,
            text,
            placeholder: "",
            width,
            leading: None,
            trailing: None,
            trailing_shown: true,
            trailing_action: false,
            trailing_size: FIELD_ICON,
            height: None,
            padding: Insets {
                left: FIELD_PAD_X,
                top: FIELD_PAD_Y,
                right: FIELD_PAD_X,
                bottom: FIELD_PAD_Y,
            },
            icon_spacing: FIELD_ICON_GAP,
            radius: FIELD_RADIUS,
            disabled: false,
            secure: false,
        }
    }

    /// `GlassTextField.search`: 44 high, 12/8 inset, icons 8 from the line, radius 22.
    pub fn search(id: Id, text: &'a str, width: f32) -> Self {
        Self {
            padding: Insets {
                left: FIELD_SEARCH_PAD_X,
                top: FIELD_SEARCH_PAD_Y,
                right: FIELD_SEARCH_PAD_X,
                bottom: FIELD_SEARCH_PAD_Y,
            },
            height: Some(FIELD_SEARCH_H),
            icon_spacing: FIELD_SEARCH_ICON_GAP,
            radius: FIELD_SEARCH_RADIUS,
            ..Self::new(id, text, width)
        }
    }

    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    pub fn leading(mut self, icon: IconSource) -> Self {
        self.leading = Some(icon);
        self
    }

    pub fn trailing(mut self, icon: IconSource) -> Self {
        self.trailing = Some(icon);
        self
    }

    /// A trailing icon that acts and fades in with the text (the search bar's clear ×).
    pub fn trailing_button(mut self, icon: IconSource, size: f32, shown: bool) -> Self {
        self.trailing = Some(icon);
        self.trailing_size = size;
        self.trailing_shown = shown;
        self.trailing_action = true;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Hide the value: marks on screen, no copy, and no surrounding text for an input method.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }
}

/// One glass text field; the buffer and its caret come from the engine each frame.
pub struct GlassTextField {
    id: Id,
    width: f32,
    height: Option<f32>,
    padding: Insets,
    icon_spacing: f32,
    radius: f32,
    leading: Option<IconSource>,
    trailing: Option<IconSource>,
    trailing_size: f32,
    trailing_t: f32,
    trailing_action: bool,
    trailing_clicked: Cell<bool>,
    text: std::sync::Arc<str>,
    placeholder: String,
    caret: Option<usize>,
    preedit: Option<ui_core::Preedit>,
    painted: Option<ui_core::SelectionPrim>,
    focused: bool,
    disabled: bool,
    secure: bool,
    ink: Color,
    placeholder_ink: Color,
    caret_ink: Color,
    focus: ui_core::draw::FocusVisual,
    /// The field's own face: a system fill, since a field is content rather than chrome.
    fill: Color,
}

impl GlassTextField {
    pub fn new(params: GlassTextFieldParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let GlassTextFieldParams {
            id,
            text,
            placeholder,
            width,
            leading,
            trailing,
            trailing_shown,
            trailing_action,
            trailing_size,
            height,
            padding,
            icon_spacing,
            radius,
            disabled,
            secure,
        } = params;
        let state = cx.state(id);
        let disabled = disabled || state.disabled;
        // Cupertino greys a disabled field's text; the same 0.38 the other glass controls use.
        let ink = Color {
            a: FIELD_INK_ALPHA * if disabled { FIELD_DISABLED_ALPHA } else { 1.0 },
            ..role_of(pack, "textFillColorPrimary")
        };
        let placeholder_ink = Color {
            a: FIELD_PLACEHOLDER_ALPHA,
            ..ink
        };
        let caret_ink = role_of(pack, "systemBlue");
        let focused = state.focused && !disabled;
        let caret = cx.core().caret_for(id);
        let preedit = cx.core().preedit_for(id);
        let painted = cx
            .core()
            .selection_for(id)
            .filter(|selection| !selection.is_collapsed())
            .map(|selection| ui_core::SelectionPrim {
                range: selection.range(),
                fill: pack
                    .ink(ui_widgets::text::roles::SELECTION)
                    .unwrap_or(Color::TRANSPARENT),
                painter: pack.selection,
            });
        // AnimatedSwitcher's 180 ms linear cross-fade, driving the trailing icon's ink and
        // size; the slot itself stays, so the text never reflows mid-fade.
        let trailing_t = match trailing {
            Some(_) => {
                let spec = AnimSpec::new(FIELD_TRAILING_FADE, Easing::Linear);
                cx.at(id).animate(
                    id.with("trailing-fade"),
                    f32::from(trailing_shown && !disabled),
                    spec,
                )
            }
            None => 0.0,
        };
        let fill = role_of(pack, "glassFieldFill");
        Self {
            id,
            width,
            height,
            padding,
            icon_spacing,
            radius,
            leading,
            trailing,
            trailing_size,
            trailing_t,
            trailing_action,
            trailing_clicked: Cell::new(false),
            text: text.into(),
            placeholder: placeholder.to_string(),
            caret,
            preedit,
            painted,
            focused,
            disabled,
            secure,
            ink,
            placeholder_ink,
            caret_ink,
            focus: focus_ring(pack),
            fill,
        }
    }

    /// The line's own width: the box less its insets and the icon slots with their gaps.
    fn line_width(&self) -> f32 {
        let slot = |size: Option<f32>| size.map_or(0.0, |size| size + self.icon_spacing);
        (self.width
            - self.padding.left
            - self.padding.right
            - slot(self.leading.map(|_| FIELD_ICON))
            - slot(self.trailing.map(|_| self.trailing_size)))
        .max(0.0)
    }
}

impl Widget for GlassTextField {
    fn handles_disabled(&self) -> bool {
        true
    }

    fn change(&self, _state: &ui_widgets::WidgetState, _bounds: Rect) -> Option<Change> {
        // Edits reach the app as `Change::Text` from the engine; the field itself only
        // reports its trailing button, named by that button's own id.
        self.trailing_clicked
            .get()
            .then(|| Change::Selected(Some(self.id.with("trailing"))))
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let corners = Corners::all(self.radius);
        // A fixed height strips the vertical inset and centres the row instead; a hugging
        // field keeps the inset and lets the line decide the height.
        let padding = match self.height {
            Some(_) => Insets {
                top: 0.0,
                bottom: 0.0,
                ..self.padding
            },
            None => self.padding,
        };
        let mut field = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: self.height.map_or(AxisSize::hug(0.0, 0.0), AxisSize::fixed),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(padding)
            .corners(corners)
            .background(self.fill, corners)
            .cross_align(0.0)
            .cross_pos(0.5)
            .text_input(self.text.clone())
            .focus_visual(Some(self.focus));
        if self.secure {
            field = field.text_input_hidden();
        }
        if !self.disabled {
            field = field
                .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
                .cursor(CursorShape::Text);
        }
        let slot = |id: Id,
                    source: IconSource,
                    box_size: f32,
                    icon_size: f32,
                    ink: Color,
                    margin: Insets| {
            Decl::container(id, NodeKind::Stack)
                .size(SizeClass::fixed(box_size, box_size))
                .margin(margin)
                .stack_align(OverlayAlign::Center)
                .child(
                    ui_widgets::Icon {
                        id: id.with("glyph"),
                        source,
                        size: icon_size,
                        ink,
                    }
                    .resolve(),
                )
        };
        if let Some(source) = self.leading {
            field = field.child(slot(
                self.id.with("leading"),
                source,
                FIELD_ICON,
                FIELD_ICON,
                self.ink,
                Insets {
                    right: self.icon_spacing,
                    ..Insets::ZERO
                },
            ));
        }
        field = field.child(
            LineEdit {
                id: self.id,
                text: self.text.as_ref(),
                caret: self.caret,
                preedit: self.preedit.as_ref(),
                painted: self.painted.clone(),
                placeholder: &self.placeholder,
                obscure: self.secure,
                hint_always: true,
                focused: self.focused,
                size: FIELD_FONT,
                weight: Weight::REGULAR,
                ink: self.ink,
                placeholder_ink: self.placeholder_ink,
                caret_ink: self.caret_ink,
                caret_width: FIELD_CARET_W,
                slack: FIELD_SCROLL_SLACK,
                width: self.line_width(),
                left_inset: 0.0,
                right_inset: 0.0,
            }
            .resolve(cx),
        );
        if let Some(source) = self.trailing {
            let t = self.trailing_t.clamp(0.0, 1.0);
            let trailing_id = self.id.with("trailing");
            let mut marker = slot(
                trailing_id,
                source,
                self.trailing_size,
                self.trailing_size * (FIELD_TRAILING_SCALE + (1.0 - FIELD_TRAILING_SCALE) * t),
                Color {
                    a: self.ink.a * t,
                    ..self.ink
                },
                Insets {
                    left: self.icon_spacing,
                    ..Insets::ZERO
                },
            );
            // Only a mostly-shown icon takes the pointer: a fading-out one must not eat it.
            if self.trailing_action && t > 0.5 && !self.disabled {
                marker = marker
                    .sense(Sense::HOVER | Sense::CLICK)
                    .cursor(CursorShape::PointingHand);
                self.trailing_clicked.set(cx.state(trailing_id).clicked);
            }
            field = field.child(marker);
        }
        field
    }
}

/// The field's own shape, `LiquidRoundedRectangle`'s default 10 dp.
pub const FIELD_RADIUS: f32 = 10.0;
/// `GlassTextField.padding` 16/12 and `iconSpacing` 12.
pub const FIELD_PAD_X: f32 = 16.0;
pub const FIELD_PAD_Y: f32 = 12.0;
pub const FIELD_ICON_GAP: f32 = 12.0;
/// `GlassTextField.search`: 44 high, 12/8 inset, icons 8 from the line.
pub const FIELD_SEARCH_H: f32 = 44.0;
pub const FIELD_SEARCH_PAD_X: f32 = 12.0;
pub const FIELD_SEARCH_PAD_Y: f32 = 8.0;
pub const FIELD_SEARCH_ICON_GAP: f32 = 8.0;
pub const FIELD_SEARCH_RADIUS: f32 = 22.0;
/// 16 pt text at 90 % ink, placeholder at 50 %; the icon's em box.
pub const FIELD_FONT: f32 = 16.0;
pub const FIELD_ICON: f32 = 20.0;
pub const FIELD_INK_ALPHA: f32 = 0.9;
pub const FIELD_PLACEHOLDER_ALPHA: f32 = 0.5;
pub const FIELD_DISABLED_ALPHA: f32 = 0.38;
/// Cupertino's caret is 2 dp wide; the field keeps 2 dp of slack past the edge it crosses.
pub const FIELD_CARET_W: f32 = 2.0;
pub const FIELD_SCROLL_SLACK: f32 = 2.0;
/// `AnimatedSwitcher`: 180 ms linear, the icon scaling up from half size.
pub const FIELD_TRAILING_FADE: f32 = 0.180;
pub const FIELD_TRAILING_SCALE: f32 = 0.5;
