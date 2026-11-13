//! The Fluent text box: a 32 dp field with a header, a placeholder and a clear button.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Clip, Grow, Overflow, SizeClass};
use ui_core::{CursorShape, Decl, Id, NodeKind, Sense, Weight};
use ui_style::{Style, TextRole};
use ui_widgets::text::LINE_HINT;
use ui_widgets::{Text, Widget, WidgetState};

use crate::glyphs;
use crate::paint::*;
use crate::text::LabelText;

/// `TextControlThemeMinHeight` / `TextControlThemeMinWidth` (generic.xaml).
/// `TextControlThemeMinHeight` / `TextControlThemeMinWidth`(generic.xaml).
pub const TEXT_BOX_HEIGHT: f32 = 32.0;
pub const TEXT_BOX_MIN_WIDTH: f32 = 64.0;
/// `ControlCornerRadius`.
pub const TEXT_BOX_RADIUS: f32 = 4.0;
/// `TextControlThemePadding`: 10,5,6,6.
pub const TEXT_BOX_PAD_LEFT: f32 = 10.0;
pub const TEXT_BOX_PAD_TOP: f32 = 5.0;
pub const TEXT_BOX_PAD_RIGHT: f32 = 6.0;
pub const TEXT_BOX_PAD_BOTTOM: f32 = 6.0;
/// `TextBoxTopHeaderMargin`: 0,0,0,8.
pub const TEXT_BOX_HEADER_GAP: f32 = 8.0;
/// The delete button: 30 wide with `TextBoxInnerButtonMargin` 0,4,4,4.
pub const CLEAR_WIDTH: f32 = 30.0;
pub const CLEAR_MARGIN: f32 = 4.0;
/// `TextBoxIconFontSize`.
pub const CLEAR_ICON: f32 = 12.0;
/// The caret's width; WinUI draws a 1 px caret, rounded here to the raster grid.
pub const CARET_WIDTH: f32 = 1.0;

/// Everything a text box is built with; the app owns the buffer.
pub struct TextBoxParams<'a> {
    pub id: Id,
    pub text: &'a str,
    /// The label above the field; empty means none.
    pub header: &'a str,
    pub placeholder: &'a str,
    pub disabled: bool,
}

impl<'a> TextBoxParams<'a> {
    pub fn new(id: Id, text: &'a str) -> Self {
        Self {
            id,
            text,
            header: "",
            placeholder: "",
            disabled: false,
        }
    }

    pub fn header(mut self, header: &'a str) -> Self {
        self.header = header;
        self
    }

    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// One Fluent text box; the engine owns the caret and the selection, the app the buffer.
pub struct FluentTextBox {
    id: Id,
    text: std::sync::Arc<str>,
    placeholder: String,
    header: Option<LabelText>,
    fill: Color,
    stroke: Stroke,
    border_w: f32,
    ink: Color,
    placeholder_ink: Color,
    caret_ink: Color,
    /// `TextControlButtonForeground`: the clear glyph's ink.
    clear_ink: Color,
    /// The near stop of the focused border brush, for the extra bottom point.
    focused_edge: Color,
    /// The selection inside the buffer, painted by the pack's own painter.
    painted: Option<ui_core::SelectionPrim>,
    caret: Option<usize>,
    preedit: Option<ui_core::ime::Preedit>,
    /// Whether the field holds keyboard focus, which thickens the bottom edge.
    focused: bool,
    /// Whether the control takes no input at all.
    disabled: bool,
    /// Whether this frame's click landed on the clear button.
    clear_clicked: std::cell::Cell<bool>,
    /// `TextControlButtonBackgroundPointerOver` then `...Pressed`.
    clear_fill: [Color; 2],
    /// `TextControlButtonForegroundPointerOver` then `...Pressed`.
    clear_ink_state: [Color; 2],
}

impl FluentTextBox {
    pub fn new(params: TextBoxParams<'_>, ctx: &mut ui_widgets::NodeCx<'_>, pack: &Style) -> Self {
        let TextBoxParams {
            id,
            text,
            header,
            placeholder,
            disabled: off,
        } = params;
        let state = ctx.state();
        let disabled = off || state.disabled;
        // WinUI's `CommonStates` are exclusive, and focus outranks the pointer.
        let focused = state.focused && !disabled;
        let slot = if disabled {
            3
        } else if focused {
            2
        } else if state.hovered {
            1
        } else {
            0
        };
        let fill = role_of(
            pack,
            [
                "TextControlBackground",
                "TextControlBackgroundPointerOver",
                "TextControlBackgroundFocused",
                "TextControlBackgroundDisabled",
            ][slot],
        );
        let stroke_name = [
            "TextControlBorderBrush",
            "TextControlBorderBrushPointerOver",
            "TextControlBorderBrushFocused",
            "TextControlBorderBrushDisabled",
        ][slot];
        let ink = role_of(
            pack,
            [
                "TextControlForeground",
                "TextControlForegroundPointerOver",
                "TextControlForegroundFocused",
                "TextControlForegroundDisabled",
            ][slot],
        );
        let placeholder_ink = role_of(
            pack,
            [
                "TextControlPlaceholderForeground",
                "TextControlPlaceholderForegroundPointerOver",
                "TextControlPlaceholderForegroundFocused",
                "TextControlPlaceholderForegroundDisabled",
            ][slot],
        );
        // A focused field's border is 1,1,1,2; the extra point is drawn as a bottom strip.
        let border_w = 1.0;
        let caret = ctx.caret();
        let preedit = ctx.preedit().cloned();
        let painted = ctx
            .selection()
            .filter(|selection| !selection.is_collapsed())
            .map(|selection| ui_core::SelectionPrim {
                range: selection.range(),
                fill: pack
                    .ink(ui_widgets::text::roles::SELECTION)
                    .unwrap_or(Color::TRANSPARENT),
                painter: pack.selection,
            });
        // WinUI names the header colour through the legacy `TextControlHeaderForeground`;
        // the WinUI 3 dictionary leaves it to generic.xaml, whose value is the primary text.
        let header = LabelText::new(
            pack,
            TextRole::Body,
            header,
            role_of(
                pack,
                if disabled {
                    "TextFillColorDisabledBrush"
                } else {
                    "TextFillColorPrimaryBrush"
                },
            ),
            f32::INFINITY,
        );
        Self {
            id,
            text: text.into(),
            placeholder: placeholder.to_string(),
            header,
            fill,
            stroke: stroke_of(pack, stroke_name),
            border_w,
            ink,
            placeholder_ink,
            caret_ink: ink,
            clear_ink: role_of(pack, "TextControlButtonForeground"),
            clear_fill: [
                role_of(pack, "TextControlButtonBackgroundPointerOver"),
                role_of(pack, "TextControlButtonBackgroundPressed"),
            ],
            clear_ink_state: [
                role_of(pack, "TextControlButtonForegroundPointerOver"),
                role_of(pack, "TextControlButtonForegroundPressed"),
            ],
            clear_clicked: std::cell::Cell::new(false),
            focused_edge: match stroke_of(pack, "TextControlBorderBrushFocused") {
                Stroke::Solid(c) => c,
                Stroke::Gradient(g) => g.from,
            },
            painted,
            caret,
            preedit,
            focused,
            disabled,
        }
    }

    /// The buffer with the composition spliced in, plus where the caret sits in it.
    fn shown(&self) -> (String, Option<std::ops::Range<usize>>, Option<usize>) {
        match (&self.preedit, self.caret) {
            (Some(p), Some(caret)) => {
                let at = caret.min(self.text.len());
                let mut shown = String::with_capacity(self.text.len() + p.text.len());
                shown.push_str(&self.text[..at]);
                shown.push_str(&p.text);
                shown.push_str(&self.text[at..]);
                let range = at..at + p.text.len();
                (shown, Some(range), Some(at + p.text.len()))
            }
            (Some(p), None) => {
                let at = self.text.len();
                let mut shown = self.text.to_string();
                shown.push_str(&p.text);
                (shown, Some(at..at + p.text.len()), Some(at + p.text.len()))
            }
            _ => (self.text.to_string(), None, self.caret),
        }
    }
}

impl Widget for FluentTextBox {
    fn handles_disabled(&self) -> bool {
        // WinUI paints its own disabled colours; the adapter must not fade them again.
        true
    }

    fn change(
        &self,
        _state: &WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        // Edits reach the app as `Change::Text` from the engine; the one action the field
        // itself reports is its clear button, named by its own id.
        self.clear_clicked
            .get()
            .then(|| ui_core::Change::Selected(Some(self.id.with("clear"))))
    }

    fn decl(&self, cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let id = self.id;
        let clear_id = id.with("clear");
        let clear_state = cx.state(clear_id);
        self.clear_clicked.set(clear_state.clicked);
        // The field fills its parent's width; only the height and the minimum are ours.
        let field_size = SizeClass {
            width: AxisSize::fill(TEXT_BOX_MIN_WIDTH, TEXT_BOX_MIN_WIDTH),
            height: AxisSize::fixed(TEXT_BOX_HEIGHT),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        };
        let (shown, composing, caret_at) = self.shown();
        let mut input = Text::new(id.with("input"), shown.clone(), 14.0, self.ink)
            .weight(Weight::REGULAR)
            .nowrap()
            .composing(composing)
            .painting(self.painted.clone());
        if let Some(offset) = caret_at.filter(|_| !shown.is_empty()) {
            input = input.caret(offset, self.caret_ink);
        }
        let input = input.resolve();
        // The text column sits between the padding and the clear button's slot.
        let mut content = Decl::container(id.with("content"), NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                // The cap covers the line and the padding the presenter carries, so the box
                // still hugs one line instead of the caret inflating it.
                height: AxisSize::hug(
                    0.0,
                    LINE_HINT * 14.0 + TEXT_BOX_PAD_TOP + TEXT_BOX_PAD_BOTTOM,
                ),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            // `TextControlThemePadding` belongs to the content presenter, not the field: the
            // clear button is a sibling stretched to the field's height.
            .padding(Insets {
                left: TEXT_BOX_PAD_LEFT,
                top: TEXT_BOX_PAD_TOP,
                right: TEXT_BOX_PAD_RIGHT,
                bottom: TEXT_BOX_PAD_BOTTOM,
            })
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Visible,
            })
            .child(input);
        if self.text.is_empty() && self.preedit.is_none() && !self.placeholder.is_empty() {
            content = content.child(
                Text::new(
                    id.with("placeholder"),
                    self.placeholder.clone(),
                    14.0,
                    self.placeholder_ink,
                )
                .weight(Weight::REGULAR)
                .nowrap()
                .resolve(),
            );
        }
        if self.text.is_empty() && self.preedit.is_none() && caret_at.is_some() {
            content = content.child(
                Decl::container(id.with("caret"), NodeKind::Box)
                    .size(SizeClass::fixed(CARET_WIDTH, 14.0 * 1.4))
                    .background(self.caret_ink, Corners::ZERO),
            );
        }
        let mut row = Decl::container(id.with("box"), NodeKind::Row)
            .size(field_size)
            .cross_align(0.0)
            .cross_pos(0.5)
            .background(self.fill, Corners::all(TEXT_BOX_RADIUS))
            .cursor(CursorShape::Text)
            // The field is the containing block for the focused bottom strip.
            .relative()
            .child(content);
        // WinUI hides the delete button on a disabled box (`ButtonCollapsed`).
        if !self.text.is_empty()
            && !self.disabled
            && let Some(glyph) = glyphs::codepoint("dismiss_12_regular")
        {
            let slot = if clear_state.pressed {
                1
            } else if clear_state.hovered {
                0
            } else {
                usize::MAX
            };
            let (fill, ink) = if slot == usize::MAX {
                (Color::TRANSPARENT, self.clear_ink)
            } else {
                (self.clear_fill[slot], self.clear_ink_state[slot])
            };
            // The button is 30 wide and stretches to the field; its fill is the inner
            // grid, inset by `TextBoxInnerButtonMargin` and rounded like the control.
            row = row.child(
                Decl::container(clear_id, NodeKind::Stack)
                    .size(SizeClass::fixed(CLEAR_WIDTH, TEXT_BOX_HEIGHT))
                    .sense(Sense::HOVER | Sense::CLICK)
                    .cursor(CursorShape::Arrow)
                    .relative()
                    .child(
                        Decl::container(id.with("clear-fill"), NodeKind::Stack)
                            .absolute()
                            .inset(ui_core::layout::LenInsets::new(
                                ui_core::layout::Len::Px(0.0),
                                ui_core::layout::Len::Px(CLEAR_MARGIN),
                                ui_core::layout::Len::Px(CLEAR_MARGIN),
                                ui_core::layout::Len::Px(CLEAR_MARGIN),
                            ))
                            .stack_align(ui_core::layout::OverlayAlign::Center)
                            .background(fill, Corners::all(TEXT_BOX_RADIUS))
                            .child(
                                Text::new(
                                    id.with("clear-glyph"),
                                    glyph.to_string(),
                                    CLEAR_ICON,
                                    ink,
                                )
                                .family(ui_core::Family::Named(glyphs::ICON_FAMILY))
                                .resolve(),
                            ),
                    ),
            );
        }
        // The root hugs header plus field; the 32 dp minimum belongs to the field alone,
        // and a hint here would cap the root and let the field overflow onto the next row.
        let mut root = Decl::container(id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(TEXT_BOX_MIN_WIDTH, TEXT_BOX_MIN_WIDTH),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .gap(TEXT_BOX_HEADER_GAP)
            // The control is the whole box, header included, as in WinUI's template; the
            // engine reports edits on this id, which is the one the app names.
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(CursorShape::Text)
            .text_input(self.text.clone())
            .disabled(self.disabled);
        if let Some(header) = &self.header {
            root = root.child(header.decl(id.with("header")));
        }
        // The focused border is 1,1,1,2: the bottom edge gains a second point.
        if self.focused {
            row = row.child(
                Decl::container(id.with("underline"), NodeKind::Box)
                    .absolute()
                    .inset(ui_core::layout::LenInsets::new(
                        ui_core::layout::Len::Px(0.0),
                        ui_core::layout::Len::Auto,
                        ui_core::layout::Len::Px(0.0),
                        ui_core::layout::Len::Px(0.0),
                    ))
                    .size(SizeClass {
                        width: AxisSize::hug(0.0, 0.0),
                        height: AxisSize::fixed(self.border_w + 1.0),
                        grow_x: Grow::No,
                        grow_y: Grow::No,
                    })
                    .background(self.focused_edge, Corners::ZERO),
            );
        }
        let row = self.stroke.on(row, self.border_w);
        root = root.child(row);
        root
    }
}
