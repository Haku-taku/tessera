//! MD3 search bar: a pill that edits one query and offers its own actions.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Clip, Grow, Overflow, SizeClass};
use ui_core::{AnimSpec, Change, CursorShape, Decl, Id, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::paint::{role, spec_ink};
use crate::spec::{SearchBarFlags, SearchBarSpec, search_bar_spec};
use crate::text_field::{caret_scroll, typed_input};

/// Height of the bar, from the export.
pub const SEARCH_BAR_HEIGHT: f32 = 56.0;
/// Side of an action's touch target, matching the bar's own height budget.
pub(crate) const ACTION_BOX: f32 = 48.0;

const HOVER: AnimSpec = AnimSpec::new(0.167, crate::motion::FAST_OUT_SLOW_IN);

/// Gap between the pill and its supporting line, matching the text field's.
const SUPPORT_GAP: f32 = 4.0;
/// The supporting line's leading inset. The export states none; M3 aligns it with the bar's
/// own 16dp edge, which the contained variant's 4dp padding does not carry.
const SUPPORT_INSET: f32 = 16.0;

/// One trailing action: the node it reports as, and how it draws itself.
#[derive(Clone, Copy)]
pub struct SearchAction {
    pub id: Id,
    pub icon: IconSource,
}

/// Everything a search bar is built with; the app owns the query.
pub struct SearchBarParams<'a> {
    pub id: Id,
    pub text: &'a str,
    /// Shown while the query is empty.
    pub hint: &'a str,
    /// Full width of the pill.
    pub width: f32,
    /// The contained variant, which hugs its contents at either end.
    pub contained: bool,
    pub disabled: bool,
    pub leading: Option<IconSource>,
    pub actions: Vec<SearchAction>,
    /// A helper line under the pill; empty means none.
    pub supporting: &'a str,
    /// The trailing avatar at the export's 30dp; the app builds the mark.
    pub avatar: Option<Decl>,
}

impl<'a> SearchBarParams<'a> {
    /// A docked bar of `width` with no hint, icon or actions.
    pub fn new(id: Id, width: f32) -> Self {
        Self {
            id,
            text: "",
            hint: "",
            width,
            contained: false,
            disabled: false,
            leading: None,
            actions: Vec::new(),
            supporting: "",
            avatar: None,
        }
    }

    pub fn text(mut self, text: &'a str) -> Self {
        self.text = text;
        self
    }

    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    pub fn contained(mut self, contained: bool) -> Self {
        self.contained = contained;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn leading(mut self, icon: IconSource) -> Self {
        self.leading = Some(icon);
        self
    }

    pub fn action(mut self, id: Id, icon: IconSource) -> Self {
        self.actions.push(SearchAction { id, icon });
        self
    }

    pub fn supporting(mut self, text: &'a str) -> Self {
        self.supporting = text;
        self
    }

    pub fn avatar(mut self, content: Decl) -> Self {
        self.avatar = Some(content);
        self
    }
}

/// What a search bar reports besides the edits the engine sends as `Change::Text`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchBarChange {
    /// One of the trailing actions was clicked.
    Action(Id),
}

/// One bar, resolved against the pack.
pub struct Md3SearchBar {
    id: Id,
    width: f32,
    text: std::sync::Arc<str>,
    hint: String,
    disabled: bool,
    pad_l: f32,
    pad_r: f32,
    leading: Option<(IconSource, Color)>,
    actions: Vec<(SearchAction, Color)>,
    avatar: Option<Decl>,
    avatar_size: f32,
    /// The avatar's touch target; `avatar.size` where the export states no target.
    avatar_target: f32,
    supporting: String,
    support_ink: Color,
    support_size: f32,
    /// The supporting line's own height, from the export's `line-height`.
    support_line: f32,
    support_weight: Weight,
    fill: Color,
    layer: Option<Color>,
    ring: Option<(Color, f32)>,
    input_ink: Color,
    hint_ink: Color,
    caret_ink: Color,
    input_size: f32,
    input_weight: Weight,
    caret: Option<usize>,
    painted: Option<ui_core::SelectionPrim>,
    preedit: Option<ui_core::Preedit>,
    fired: Option<Id>,
}

impl Md3SearchBar {
    /// Build one bar. The caret and the selection come from the engine.
    pub fn new_with(
        params: Md3Options,
        content: SearchBarParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let SearchBarParams {
            id,
            text,
            hint,
            width,
            contained,
            disabled: off,
            leading,
            actions,
            supporting,
            avatar,
        } = content;
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
        let mut node = cx.at(id);
        let state = node.state();
        let disabled = params.disabled || off || state.disabled;
        let focused = state.focused && !disabled;
        let has_actions = !actions.is_empty();
        let rest = parts(contained, has_actions, state.hovered, state.pressed);
        let hover = parts(contained, has_actions, true, false);
        let hover_t = node.animate(id.with(param::HOVER), f32::from(state.hovered), HOVER);

        let layer = (!hover.state_layer.color.is_empty())
            .then(|| {
                let ink = role(pack, hover.state_layer.color);
                let strength = hover_t * hover.state_layer.opacity;
                Color {
                    a: ink.a * strength,
                    ..ink
                }
            })
            .filter(|c| c.a > 0.0 && !disabled);
        let ring = focused.then(|| {
            (
                spec_ink(pack, rest.focus_indicator.color, 1.0),
                rest.focus_indicator.thickness,
            )
        });
        let icon_ink = spec_ink(pack, rest.leading_icon.color, 1.0);
        let action_ink = spec_ink(pack, rest.trailing_icon.color, 1.0);
        let mut fired = None;
        for action in &actions {
            if cx.state(action.id).activation() {
                fired = Some(action.id);
            }
        }
        Self {
            id,
            width,
            text: text.into(),
            hint: hint.to_string(),
            disabled,
            pad_l: rest.root.leading_space,
            pad_r: rest.root.trailing_space,
            leading: leading.map(|f| (f, icon_ink)),
            actions: actions.into_iter().map(|a| (a, action_ink)).collect(),
            avatar,
            avatar_size: rest.avatar.size,
            avatar_target: rest.avatar.target_size.max(rest.avatar.size),
            supporting: supporting.to_string(),
            support_ink: spec_ink(pack, rest.supporting_text.color, 1.0),
            support_size: rest.supporting_text.size,
            support_line: rest.supporting_text.line_height,
            support_weight: Weight(rest.supporting_text.weight as u16),
            fill: spec_ink(pack, rest.container.color, 1.0),
            layer,
            ring,
            input_ink: spec_ink(pack, rest.input_text.color, 1.0),
            hint_ink: spec_ink(pack, rest.leading_icon.color, 0.7),
            caret_ink: spec_ink(pack, rest.focus_indicator.color, 1.0),
            input_size: rest.input_text.size,
            input_weight: Weight(rest.input_text.weight as u16),
            caret,
            painted,
            preedit,
            fired,
        }
    }

    /// The height the bar occupies: the pill plus its supporting line, when it has one.
    pub fn height(&self) -> f32 {
        SEARCH_BAR_HEIGHT + self.support_band()
    }

    fn support_band(&self) -> f32 {
        if self.supporting.is_empty() {
            0.0
        } else {
            SUPPORT_GAP + self.support_line
        }
    }

    /// The pill: fill, state layer and, while focused, a ring on the bar's own edge.
    fn shapes(&self) -> Vec<Shape> {
        let corners = Corners::all(SEARCH_BAR_HEIGHT * 0.5);
        let mut shapes = vec![Shape::Box {
            corners,
            fill: self.fill,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        }];
        if let Some(layer) = self.layer {
            shapes.push(Shape::Box {
                corners,
                fill: layer,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            });
        }
        // The export offsets the ring outward; a declaration cannot know the final rect, so
        // it is drawn on the bar's own edge instead, as the list and carousel items do.
        if let Some((ink, width)) = self.ring {
            shapes.push(Shape::Box {
                corners,
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((ink, width)),
                shadow: None,
                image: None,
            });
        }
        shapes
    }
}

/// One 24dp icon in a 48dp touch box, centred in the header of height `height`.
pub(crate) fn icon_box(id: Id, source: IconSource, ink: Color, height: f32) -> Decl {
    Decl::container(id, NodeKind::Column)
        .size(SizeClass::fixed(ACTION_BOX, height))
        .padding(Insets {
            left: 0.0,
            right: 0.0,
            top: (height - ui_widgets::icon_extent(24.0)) * 0.5,
            bottom: 0.0,
        })
        .cross_align(0.0)
        .cross_pos(0.5)
        .child(source.resolve(id.with("icon"), ink))
}

/// The trailing avatar: app-drawn content at `size`, centred in its touch target.
///
/// The export gives the slot a circular shape and a `target_size`; the app builds the mark,
/// exactly as the app bar's leading avatar does.
fn avatar_slot(id: Id, avatar: Decl, size: f32, target: f32) -> Decl {
    Decl::container(id, NodeKind::Column)
        .size(SizeClass::fixed(target, SEARCH_BAR_HEIGHT))
        .padding(Insets {
            left: 0.0,
            top: (SEARCH_BAR_HEIGHT - size) * 0.5,
            right: 0.0,
            bottom: 0.0,
        })
        .cross_align(0.0)
        .cross_pos(0.5)
        .child(avatar)
}

/// The editable query: shown text, caret, selection, composing run and the hint behind them.
pub(crate) struct QueryField<'a> {
    pub id: Id,
    pub text: &'a str,
    pub caret: Option<usize>,
    pub preedit: Option<&'a ui_core::Preedit>,
    pub painted: Option<ui_core::SelectionPrim>,
    pub size: f32,
    pub weight: Weight,
    pub ink: Color,
    pub hint: &'a str,
    pub hint_ink: Color,
    pub caret_ink: Color,
    pub width: f32,
}

/// One query line, clipped to `width` and scrolled just far enough to keep the caret in view.
pub(crate) fn query_field(cx: &Cx<'_, '_>, f: QueryField<'_>) -> Decl {
    let (shown, composing, caret_at) = typed_input(f.text, f.caret, f.preedit);
    let mut input = Text::new(f.id.with("input"), shown.clone(), f.size, f.ink)
        .weight(f.weight)
        .nowrap()
        .max_width(f.width)
        .painting(f.painted)
        .composing(composing);
    if let Some(offset) = caret_at.filter(|_| !shown.is_empty()) {
        input = input.caret(offset, f.caret_ink);
    }
    let scroll = caret_scroll(cx, f.id.with("input"), f.width);
    let input = input.resolve().offset(-scroll, 0.0);

    // The box hugs its line; the caller's row centres it with `cross_pos`, so the typeface's
    // line height decides the position, not a widget constant.
    let mut content = Decl::container(f.id.with("content"), NodeKind::Stack)
        .size(SizeClass {
            width: AxisSize::fixed(f.width),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        // A nowrap query runs past the field; cutting it here is what keeps the overflow
        // out of sight, and the renderer then skips the glyphs entirely.
        .clip(Clip {
            x: Overflow::Hidden,
            y: Overflow::Visible,
        })
        .child(input);
    if f.text.is_empty() && f.preedit.is_none() && !f.hint.is_empty() {
        content = content.child(
            Text::new(f.id.with("hint"), f.hint.to_string(), f.size, f.hint_ink)
                .weight(f.weight)
                .max_width(f.width)
                .resolve(),
        );
    }
    content
}

impl Widget for Md3SearchBar {
    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let lead_w = if self.leading.is_some() {
            ACTION_BOX
        } else {
            0.0
        };
        let actions_w = self.actions.len() as f32 * ACTION_BOX;
        let avatar_w = if self.avatar.is_some() {
            self.avatar_target
        } else {
            0.0
        };
        let input_w =
            (self.width - self.pad_l - self.pad_r - lead_w - actions_w - avatar_w).max(0.0);
        let content = query_field(
            cx,
            QueryField {
                id: self.id,
                text: self.text.as_ref(),
                caret: self.caret,
                preedit: self.preedit.as_ref(),
                painted: self.painted.clone(),
                size: self.input_size,
                weight: self.input_weight,
                ink: self.input_ink,
                hint: &self.hint,
                hint_ink: self.hint_ink,
                caret_ink: self.caret_ink,
                width: input_w,
            },
        );

        let mut bar = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass::fixed(self.width, SEARCH_BAR_HEIGHT))
            .padding(Insets {
                left: self.pad_l,
                top: 0.0,
                right: self.pad_r,
                bottom: 0.0,
            })
            .cross_align(0.0)
            // A row's cross axis is vertical: this centres the query line in the bar.
            .cross_pos(0.5)
            .shapes(self.shapes())
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::Text
            })
            .disabled(self.disabled)
            .text_input(self.text.clone());
        if let Some((source, ink)) = self.leading {
            bar = bar.child(icon_box(
                self.id.with("leading"),
                source,
                ink,
                SEARCH_BAR_HEIGHT,
            ));
        }
        bar = bar.child(content);
        for (action, ink) in &self.actions {
            bar = bar.child(
                icon_box(action.id, action.icon, *ink, SEARCH_BAR_HEIGHT).sense(Sense::CLICK),
            );
        }
        if let Some(avatar) = self.avatar.clone() {
            bar = bar.child(avatar_slot(
                self.id.with("avatar"),
                avatar,
                self.avatar_size,
                self.avatar_target,
            ));
        }
        if self.supporting.is_empty() {
            return bar;
        }
        let inner_w = (self.width - 2.0 * SUPPORT_INSET).max(0.0);
        Decl::container(self.id.with("root"), NodeKind::Column)
            .size(SizeClass::fixed(self.width, self.height()))
            .cross_align(0.0)
            .cross_pos(0.0)
            .child(bar)
            .child(
                Decl::container(self.id.with("support"), NodeKind::Column)
                    .size(SizeClass::fixed(self.width, self.support_band()))
                    .padding(Insets {
                        left: SUPPORT_INSET,
                        top: SUPPORT_GAP,
                        right: SUPPORT_INSET,
                        bottom: 0.0,
                    })
                    .cross_align(0.0)
                    .cross_pos(0.0)
                    .child(
                        Text::new(
                            self.id.with("supporting"),
                            self.supporting.clone(),
                            self.support_size,
                            self.support_ink,
                        )
                        .weight(self.support_weight)
                        .max_width(inner_w)
                        .resolve(),
                    ),
            )
    }

    fn change(&self, _state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        Some(Change::Data(Box::new(SearchBarChange::Action(self.fired?))))
    }
}

/// Resolve the export for one state; the contained variant also decides the end insets.
fn parts(contained: bool, has_actions: bool, hovered: bool, pressed: bool) -> SearchBarSpec {
    search_bar_spec(SearchBarFlags {
        contained,
        trailing_actions: has_actions,
        hovered,
        pressed,
        ..SearchBarFlags::NONE
    })
}
