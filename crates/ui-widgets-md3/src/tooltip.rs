//! MD3 tooltips: an inverse-surface plain panel with a `body-small` label, and the rich
//! surface with a subhead, supporting text and text-button actions. The app declares a
//! tooltip only while it shows; the engine fades both ways.

use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{
    Anchor, AxisSize, Grow, OverlayAlign, OverlayPlacement, Presentation, Side, SizeClass,
};
use ui_core::tree::{Presence, ShadowSpec};
use ui_core::{AnimSpec, Change, Decl, Easing, Id, NodeKind, Weight};
use ui_style::Style;
use ui_widgets::text::Text;
use ui_widgets::{Cx, Widget, WidgetState};

use crate::paint::spec_ink;
use crate::spec::{RichTooltipFlags, plain_tooltip_spec, rich_tooltip_spec};
use crate::{ButtonParams, ButtonSize, ButtonStyle, Md3Button, Md3Options};

/// `PaddingValues(horizontal = 8.dp, vertical = 4.dp)` from Compose's plain tooltip.
const PAD: Insets = Insets {
    left: 8.0,
    top: 4.0,
    right: 8.0,
    bottom: 4.0,
};
/// `corner.extra-small`; the export names the shape `rounded_corners` without a number.
const CORNER: f32 = 4.0;
/// A plain tooltip wraps its label inside 200dp; it is never a full-width surface.
const MAX_WIDTH: f32 = 200.0;
/// The room between a label and the chord beside it, in points.
const HINT_GAP: f32 = 12.0;
/// Flutter's `Tooltip` defaults: 150 ms in, 75 ms out, both plain fades.
const FADE_IN: AnimSpec = AnimSpec::new(0.15, Easing::Linear);
const FADE_OUT: AnimSpec = AnimSpec::new(0.075, Easing::Linear);

/// The label and the anchor a plain tooltip hangs from.
pub struct ToolTipParams<'a> {
    pub id: Id,
    pub text: &'a str,
    /// A chord the label's action answers, shown beside it in a quieter ink.
    pub hint: Option<&'a str>,
    pub anchor: Anchor,
    /// Plain tooltips sit above the anchor; the engine flips when there is no room.
    pub side: Side,
}

impl<'a> ToolTipParams<'a> {
    pub fn new(id: Id, text: &'a str, anchor: Anchor) -> Self {
        Self {
            id,
            text,
            hint: None,
            anchor,
            side: Side::Above,
        }
    }

    /// Show `hint` beside the label, as a platform tooltip shows the chord it answers.
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }

    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }
}

/// One plain tooltip panel.
pub struct Md3ToolTip {
    id: Id,
    text: String,
    hint: Option<String>,
    ink: Color,
    container: Color,
    corners: Corners,
    text_size: f32,
    text_weight: Weight,
    anchor: Anchor,
    side: Side,
}

impl Md3ToolTip {
    /// Declare the panel only while it shows: removal is what runs the exit fade.
    pub fn new(params: ToolTipParams<'_>, pack: &Style) -> Self {
        let s = plain_tooltip_spec();
        Self {
            id: params.id,
            text: params.text.to_string(),
            hint: params.hint.map(str::to_string),
            ink: spec_ink(pack, s.supporting_text.color, 1.0),
            container: spec_ink(pack, s.container.color, 1.0),
            corners: Corners::all(CORNER),
            text_size: s.supporting_text.size,
            text_weight: Weight(s.supporting_text.weight as u16),
            anchor: params.anchor,
            side: params.side,
        }
    }
}

impl Widget for Md3ToolTip {
    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let placement = OverlayPlacement {
            anchor: self.anchor,
            side: self.side,
            align: OverlayAlign::Center,
            offset: 0.0,
            flip: true,
            match_anchor_width: false,
            match_anchor_height: false,
            min_width: 0.0,
            presentation: Presentation::Parent,
        };
        Decl::container(self.id, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::hug(0.0, MAX_WIDTH),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(PAD)
            .corners(self.corners)
            .background(self.container, self.corners)
            .overlay(placement)
            .enter(Presence::fade(FADE_IN, 0.0))
            .exit(Presence::fade(FADE_OUT, 0.0))
            .child(self.label())
    }
}

impl Md3ToolTip {
    /// The label, with the chord beside it when the control answers one.
    fn label(&self) -> Decl {
        let text = Text::new(
            self.id.with("text"),
            self.text.clone(),
            self.text_size,
            self.ink,
        )
        .weight(self.text_weight)
        .max_width(MAX_WIDTH - PAD.left - PAD.right)
        .resolve();
        let Some(hint) = &self.hint else {
            return text;
        };
        // MD3's plain tooltip carries no hint row; a platform tooltip names the chord, so
        // the chord sits beside the label in the same box, quieter than the words.
        let hint = Text::new(
            self.id.with("hint"),
            hint.clone(),
            self.text_size,
            Color {
                a: self.ink.a * 0.7,
                ..self.ink
            },
        )
        .weight(self.text_weight)
        .nowrap()
        .resolve();
        Decl::container(self.id.with("row"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, MAX_WIDTH),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .gap(HINT_GAP)
            .cross_align(0.0)
            .cross_pos(0.5)
            .child(text)
            .child(hint)
    }
}

/// AndroidX Compose Material3 1.4's rich tooltip: 16dp sides, a 320dp cap, 40/24 minimums.
const RICH_SIDE: f32 = 16.0;
const RICH_MAX_WIDTH: f32 = 320.0;
const RICH_MIN_WIDTH: f32 = 40.0;
const RICH_MIN_HEIGHT: f32 = 24.0;
/// `corner.medium`; the export names the shape `rounded_corners` without a number.
const RICH_CORNER: f32 = 12.0;
/// Compose sets the subhead's first baseline 28dp below the top edge; with a 20dp line
/// box that puts the glyphs about 12dp down.
const RICH_SUBHEAD_TOP: f32 = 12.0;
/// The supporting text's baseline sits 24dp below the subhead's, so two 20dp lines leave a
/// 4dp gap; without a subhead the same baseline lands about 10dp below the top edge.
const RICH_TEXT_GAP: f32 = 4.0;
const RICH_TEXT_TOP: f32 = 10.0;
const RICH_TEXT_BOTTOM: f32 = 16.0;
/// The action band keeps 8dp below itself; two actions keep 8dp apart.
const RICH_ACTION_BOTTOM: f32 = 8.0;
const RICH_ACTION_GAP: f32 = 8.0;

/// One action: a text button the tooltip reports by id.
pub struct RichToolTipAction<'a> {
    pub id: Id,
    pub label: &'a str,
}

impl<'a> RichToolTipAction<'a> {
    pub fn new(id: Id, label: &'a str) -> Self {
        Self { id, label }
    }
}

/// The content and anchor a rich tooltip hangs from.
pub struct RichToolTipParams<'a> {
    pub id: Id,
    /// The optional `title-small` line above the supporting text.
    pub subhead: Option<&'a str>,
    pub text: &'a str,
    pub actions: &'a [RichToolTipAction<'a>],
    pub anchor: Anchor,
    pub side: Side,
}

impl<'a> RichToolTipParams<'a> {
    pub fn new(id: Id, text: &'a str, anchor: Anchor) -> Self {
        Self {
            id,
            subhead: None,
            text,
            actions: &[],
            anchor,
            side: Side::Above,
        }
    }

    pub fn subhead(mut self, subhead: &'a str) -> Self {
        self.subhead = Some(subhead);
        self
    }

    pub fn actions(mut self, actions: &'a [RichToolTipAction<'a>]) -> Self {
        self.actions = actions;
        self
    }

    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }
}

/// One rich tooltip panel: a surface with a subhead, supporting text and actions.
pub struct Md3RichToolTip {
    id: Id,
    subhead: Option<String>,
    text: String,
    actions: Vec<Md3Button>,
    /// The action whose button took this frame's click.
    fired: Option<Id>,
    container: Color,
    subhead_ink: Color,
    text_ink: Color,
    corners: Corners,
    shadow: Option<ShadowSpec>,
    subhead_size: f32,
    subhead_weight: Weight,
    text_size: f32,
    text_weight: Weight,
    anchor: Anchor,
    side: Side,
}

impl Md3RichToolTip {
    /// Build the panel; declare it only while it shows, since removal runs the exit fade.
    pub(crate) fn new_with(
        options: Md3Options,
        params: RichToolTipParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let spec = rich_tooltip_spec(RichTooltipFlags::NONE);
        // The action states belong to `md.comp.button`: the actions are text buttons and
        // resolve their own layer, so only this table's non-state values are read here.
        let fired = params
            .actions
            .iter()
            .find(|action| cx.state(action.id).activation())
            .map(|action| action.id);
        let actions = params
            .actions
            .iter()
            .map(|action| {
                Md3Button::new_with(
                    options,
                    ButtonParams::new(action.id, action.label)
                        .style(ButtonStyle::Text)
                        .size(ButtonSize::Base),
                    cx,
                    pack,
                )
            })
            .collect();
        let elevation = spec.container.elevation;
        Self {
            id: params.id,
            subhead: params.subhead.map(str::to_string),
            text: params.text.to_string(),
            actions,
            fired,
            container: spec_ink(pack, spec.container.color, 1.0),
            subhead_ink: spec_ink(pack, spec.subhead.color, 1.0),
            text_ink: spec_ink(pack, spec.supporting_text.color, 1.0),
            corners: Corners::all(RICH_CORNER),
            shadow: (elevation > 0.0)
                .then(|| ShadowSpec::black(elevation * 2.0, 0.18, elevation * 0.5)),
            subhead_size: spec.subhead.size,
            subhead_weight: Weight(spec.subhead.weight as u16),
            text_size: spec.supporting_text.size,
            text_weight: Weight(spec.supporting_text.weight as u16),
            anchor: params.anchor,
            side: params.side,
        }
    }

    fn wrap(&self) -> f32 {
        RICH_MAX_WIDTH - 2.0 * RICH_SIDE
    }

    /// One band. The line owns alignment, so `pos` places this band's child across it: the
    /// action row is the one that ends at the trailing edge.
    fn band(&self, which: &str, pad: Insets, align: f32, pos: f32) -> Decl {
        Decl::container(self.id.with(which), NodeKind::Column)
            .padding(pad)
            .cross_align(align)
            .cross_pos(pos)
    }
}

impl Widget for Md3RichToolTip {
    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> {
        self.fired.map(|id| Change::Selected(Some(id)))
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let placement = OverlayPlacement {
            anchor: self.anchor,
            side: self.side,
            align: OverlayAlign::Center,
            offset: 0.0,
            flip: true,
            match_anchor_width: false,
            match_anchor_height: false,
            min_width: 0.0,
            presentation: Presentation::Parent,
        };
        let mut root = Decl::container(self.id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::hug(RICH_MIN_WIDTH, RICH_MAX_WIDTH),
                height: AxisSize::hug(RICH_MIN_HEIGHT, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // Bands stretch to the surface, so the action band can reach its far edge.
            .cross_align(1.0)
            .corners(self.corners)
            .background(self.container, self.corners)
            .overlay(placement)
            .enter(Presence::fade(FADE_IN, 0.0))
            .exit(Presence::fade(FADE_OUT, 0.0));
        if let Some(shadow) = self.shadow {
            root = root.shadow(shadow);
        }
        if let Some(subhead) = &self.subhead {
            let pad = Insets {
                left: RICH_SIDE,
                top: RICH_SUBHEAD_TOP,
                right: RICH_SIDE,
                bottom: 0.0,
            };
            root = root.child(
                self.band("subhead", pad, 0.0, 0.0).child(
                    Text::new(
                        self.id.with("subhead-text"),
                        subhead.clone(),
                        self.subhead_size,
                        self.subhead_ink,
                    )
                    .weight(self.subhead_weight)
                    .max_width(self.wrap())
                    .resolve(),
                ),
            );
        }
        let top = if self.subhead.is_some() {
            RICH_TEXT_GAP
        } else {
            RICH_TEXT_TOP
        };
        let pad = Insets {
            left: RICH_SIDE,
            top,
            right: RICH_SIDE,
            bottom: RICH_TEXT_BOTTOM,
        };
        root = root.child(
            // The band cannot be called "text": the paragraph inside it is, and the tree keys
            // by id, so the two would collapse into one node.
            self.band("text-band", pad, 0.0, 0.0).child(
                Text::new(
                    self.id.with("text"),
                    self.text.clone(),
                    self.text_size,
                    self.text_ink,
                )
                .weight(self.text_weight)
                .max_width(self.wrap())
                .resolve(),
            ),
        );
        if !self.actions.is_empty() {
            let mut row = Decl::container(self.id.with("action-row"), NodeKind::Row)
                .gap(RICH_ACTION_GAP)
                .cross_align(0.0);
            for action in &self.actions {
                row = row.child(action.decl(cx));
            }
            let pad = Insets {
                left: RICH_SIDE,
                top: 0.0,
                right: RICH_SIDE,
                bottom: RICH_ACTION_BOTTOM,
            };
            root = root.child(self.band("actions", pad, 0.0, 1.0).child(row));
        }
        root
    }
}
