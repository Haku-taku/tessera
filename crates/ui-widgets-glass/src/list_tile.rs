//! The iOS grouped list row: leading glyph, title/subtitle and trailing glyph on a system fill.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, OverlayAlign, SizeClass};
use ui_core::{AnimSpec, Change, CursorShape, Decl, Id, NodeKind, Sense, Weight};
use ui_style::{Style, TextRole};
use ui_widgets::build::IMMEDIATE;
use ui_widgets::{Cx, IconSource, Widget};

use crate::paint::role_of;
use crate::text::LabelText;

/// Everything a list tile is built with; the row knows nothing of its neighbours.
pub struct GlassListTileParams<'a> {
    pub id: Id,
    pub title: &'a str,
    pub subtitle: Option<&'a str>,
    pub leading: Option<IconSource>,
    pub trailing: Option<IconSource>,
    /// Whether the row acts: a still row takes no pointer and paints no wash.
    pub action: bool,
    /// Draws its own row surface; a row inside a card leaves that to the card.
    pub standalone: bool,
}

impl<'a> GlassListTileParams<'a> {
    pub fn new(id: Id, title: &'a str) -> Self {
        Self {
            id,
            title,
            subtitle: None,
            leading: None,
            trailing: None,
            action: true,
            standalone: false,
        }
    }

    pub fn subtitle(mut self, subtitle: &'a str) -> Self {
        self.subtitle = Some(subtitle);
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

    /// The iOS disclosure chevron as the trailing glyph (`GlassListTile.chevron`).
    pub fn chevron(mut self) -> Self {
        self.trailing = Some(IconSource::sf("chevron_right"));
        self
    }

    pub fn still(mut self) -> Self {
        self.action = false;
        self
    }

    pub fn standalone(mut self) -> Self {
        self.standalone = true;
        self
    }
}

/// One glass list row; the highlight fades in `new`, `decl()` is pure.
pub struct GlassListTile {
    id: Id,
    title: Option<LabelText>,
    subtitle: Option<LabelText>,
    leading: Option<IconSource>,
    trailing: Option<IconSource>,
    action: bool,
    standalone: bool,
    /// 0..=1: the press/focus wash this frame.
    highlight: f32,
    ink: Color,
    trail_ink: Color,
    /// The wash's own colour, black in light and white in dark.
    wash: Color,
    /// The row's own face: Apple's grouped cell, never glass — a row is content.
    surface: Color,
}

impl GlassListTile {
    pub fn new(params: GlassListTileParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let GlassListTileParams {
            id,
            title,
            subtitle,
            leading,
            trailing,
            action,
            standalone,
        } = params;
        let state = cx.state(id);
        let ink = role_of(pack, "textFillColorPrimary");
        let sub_ink = Color {
            a: LIST_SUBTITLE_ALPHA,
            ..ink
        };
        let trail_ink = Color {
            a: LIST_TRAILING_ALPHA,
            ..ink
        };
        let dark = pack.appearance() == Some(ui_design::Appearance::Dark);
        let wash = Color {
            a: LIST_HIGHLIGHT,
            ..if dark { Color::WHITE } else { Color::BLACK }
        };

        // ── Press and keyboard focus both raise the wash; it drops over 150 ms.
        let target = f32::from(action && (state.pressed || state.focus_visible));
        let spec = if target > 0.5 {
            IMMEDIATE
        } else {
            HIGHLIGHT_OUT
        };
        let highlight = {
            let mut node = cx.at(id);
            node.animate(id.with("highlight"), target, spec)
        };

        let surface = if standalone {
            role_of(pack, "glassRowFill")
        } else {
            Color::TRANSPARENT
        };

        Self {
            id,
            title: LabelText::new(pack, TextRole::Body, title, ink, f32::INFINITY)
                .map(|l| l.restyled(LIST_TITLE, Weight::MEDIUM)),
            subtitle: subtitle.and_then(|s| {
                LabelText::new(pack, TextRole::Body, s, sub_ink, f32::INFINITY)
                    .map(|l| l.restyled(LIST_SUBTITLE, Weight::REGULAR))
            }),
            leading,
            trailing,
            action,
            standalone,
            highlight,
            ink,
            trail_ink,
            wash,
            surface,
        }
    }
}

impl Widget for GlassListTile {
    fn handles_disabled(&self) -> bool {
        true
    }

    fn change(
        &self,
        state: &ui_widgets::WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<Change> {
        (self.action && state.activation()).then_some(Change::Activated)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let id = self.id;
        let corners = if self.standalone {
            Corners::all(LIST_RADIUS)
        } else {
            Corners::ZERO
        };
        let wash = Color {
            a: self.wash.a * self.highlight,
            ..self.wash
        };
        // The press wash stacks over the row face, so the face can stay opaque.
        let mut root = Decl::container(id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: LIST_PAD_X,
                top: LIST_PAD_Y,
                right: LIST_PAD_X,
                bottom: LIST_PAD_Y,
            })
            .corners(corners)
            .background(self.surface, corners)
            .shapes(vec![ui_core::draw::Shape::Box {
                corners,
                fill: wash,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }])
            .cross_align(0.0)
            .cross_pos(0.5);
        if self.action {
            root = root
                .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
                .cursor(CursorShape::PointingHand);
        }

        if let Some(icon) = self.leading {
            // A minimum (not fixed) width in the reference, so a 44 pt avatar keeps its size;
            // an icon is centred in that slot rather than hung from its left edge.
            root = root.child(
                Decl::container(id.with("leading"), NodeKind::Stack)
                    .size(SizeClass::fixed(LIST_LEADING_MIN, LIST_LEADING))
                    .margin(Insets {
                        right: LIST_GAP,
                        ..Insets::ZERO
                    })
                    .stack_align(OverlayAlign::Center)
                    .child(
                        ui_widgets::Icon {
                            id: id.with("leading").with("glyph"),
                            source: icon,
                            size: LIST_LEADING,
                            ink: self.ink,
                        }
                        .resolve(),
                    ),
            );
        }

        // The text column is the reference's `Expanded`: it takes what the glyphs leave.
        let mut text = Decl::container(id.with("text"), NodeKind::Column)
            .gap(LIST_TEXT_GAP)
            .cross_align(0.0);
        if let Some(title) = &self.title {
            text = text.child(title.decl(id.with("title")));
        }
        if let Some(subtitle) = &self.subtitle {
            text = text.child(subtitle.decl(id.with("subtitle")));
        }
        text.size_class.grow_x = Grow::Yes;
        root = root.child(text);

        if let Some(icon) = self.trailing {
            root = root.child(
                Decl::container(id.with("trailing"), NodeKind::Stack)
                    .size(SizeClass::fixed(LIST_TRAILING, LIST_TRAILING))
                    .margin(Insets {
                        left: LIST_TRAILING_GAP,
                        ..Insets::ZERO
                    })
                    .stack_align(OverlayAlign::Center)
                    .child(
                        ui_widgets::Icon {
                            id: id.with("trailing").with("glyph"),
                            source: icon,
                            size: LIST_TRAILING,
                            ink: self.trail_ink,
                        }
                        .resolve(),
                    ),
            );
        }
        root
    }
}

/// `GlassListTile.contentPadding` and the row's own metrics (`glass_list_tile.dart:78-81`).
pub const LIST_PAD_X: f32 = 16.0;
pub const LIST_PAD_Y: f32 = 12.0;
/// The leading glyph, the slot it sits in (`minWidth: 28`), and the gap after it.
pub const LIST_LEADING: f32 = 22.0;
pub const LIST_LEADING_MIN: f32 = 28.0;
pub const LIST_GAP: f32 = 12.0;
/// Title 16 pt w500, subtitle 13 pt at 65 %, and the 2 dp between them.
pub const LIST_TITLE: f32 = 16.0;
pub const LIST_SUBTITLE: f32 = 13.0;
pub const LIST_SUBTITLE_ALPHA: f32 = 0.65;
pub const LIST_TEXT_GAP: f32 = 2.0;
/// The trailing glyph: 20 pt at 54 %, with 8 dp before it.
pub const LIST_TRAILING: f32 = 20.0;
pub const LIST_TRAILING_ALPHA: f32 = 0.54;
pub const LIST_TRAILING_GAP: f32 = 8.0;
/// The press/focus wash: 8 % black in light, 8 % white in dark (`:934-936`).
pub const LIST_HIGHLIGHT: f32 = 0.08;
/// A standalone tile's own face, `LiquidRoundedSuperellipse(borderRadius: 12)`.
pub const LIST_RADIUS: f32 = 12.0;
/// The wash leaves over 150 ms easeOutCubic; it arrives with no tween (`:929-932`).
const HIGHLIGHT_OUT: AnimSpec = AnimSpec::new(0.150, ui_style::motion::CSS_EASE_OUT);
