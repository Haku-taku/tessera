//! The Fluent info bar: a severity-tinted strip with an icon, a message and a close button.

use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, AxisSwitch, Grow, SizeClass};
use ui_core::{Change, CursorShape, Decl, Id, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::{Icon, IconSource, NodeCx, Widget, WidgetState};

use crate::glyphs;
use crate::paint::role_of;

/// `InfoBarMinHeight`, `ControlCornerRadius`, `InfoBarBorderThickness`.
pub const INFO_BAR_MIN_HEIGHT: f32 = 48.0;
pub const INFO_BAR_RADIUS: f32 = 4.0;
pub const INFO_BAR_BORDER: f32 = 1.0;
/// `InfoBarContentRootPadding`: 16,0,0,0.
pub const INFO_BAR_PAD: Insets = Insets {
    left: 16.0,
    top: 0.0,
    right: 0.0,
    bottom: 0.0,
};
/// `InfoBarIconMargin` 0,16,14,16 around a 16 pt icon.
pub const INFO_BAR_ICON: f32 = 16.0;
/// The reference's symbols are glyphs cut for the 15 pt `F136` circle: their inks are 5×5
/// (error), 7×5 (success) and 1.5×6.5 (info, warning). The vendored face draws its inks at
/// most of the em, so each is sized to land at that same share of the disc.
pub const INFO_BAR_SYMBOL_CHECK: f32 = 9.5;
pub const INFO_BAR_SYMBOL_DISMISS: f32 = 7.5;
pub const INFO_BAR_SYMBOL_IMPORTANT: f32 = 8.7;
/// The composed informational "i": a 1.5 pt dot over a 4 pt stem, 1 pt apart.
pub const INFO_BAR_MARK: f32 = 1.5;
pub const INFO_BAR_STEM: f32 = 4.0;
pub const INFO_BAR_MARK_GAP: f32 = 1.0;
pub const INFO_BAR_ICON_MARGIN: Insets = Insets {
    left: 0.0,
    top: 16.0,
    right: 14.0,
    bottom: 16.0,
};
/// `InfoBarPanelMargin` 0,0,16,0.
/// `InfoBarPanelMargin`: 0,0,16,0.
pub const INFO_BAR_PANEL_MARGIN: Insets = Insets {
    left: 0.0,
    top: 0.0,
    right: 16.0,
    bottom: 0.0,
};
/// `InfoBarCloseButtonSize` 38, its 5 pt margin, and the 16 pt `Cancel` glyph.
pub const INFO_BAR_CLOSE: f32 = 38.0;
pub const INFO_BAR_CLOSE_MARGIN: f32 = 5.0;
pub const INFO_BAR_CLOSE_GLYPH: f32 = 16.0;
/// `InfoBarTitleFontSize` / `InfoBarMessageFontSize`.
pub const INFO_BAR_FONT: f32 = 14.0;

/// `InfoBarTitleHorizontalOrientationMargin` and its vertical twin: 0,14,0,0 either way.
const TITLE_MARGIN: Insets = Insets {
    left: 0.0,
    top: 14.0,
    right: 0.0,
    bottom: 0.0,
};
/// `InfoBarMessageHorizontalOrientationMargin` 12,14,0,0; vertical 0,4,0,0.
const MESSAGE_MARGIN: Insets = Insets {
    left: 12.0,
    top: 14.0,
    right: 0.0,
    bottom: 0.0,
};
/// `InfoBarPanelVerticalOrientationPadding` 0,14,0,18, folded into the panel's own edges.
pub const PANEL_PAD_TOP: f32 = 14.0;
pub const PANEL_PAD_BOTTOM: f32 = 18.0;
const MESSAGE_MARGIN_STACKED: Insets = Insets {
    left: 0.0,
    top: 4.0,
    right: 0.0,
    bottom: PANEL_PAD_BOTTOM,
};

/// The four `InfoBarSeverity` levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoBarSeverity {
    Informational,
    Success,
    Warning,
    Error,
}

impl InfoBarSeverity {
    /// The three brushes WinUI's theme dictionary keys by severity.
    fn brushes(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Informational => (
                "InfoBarInformationalSeverityBackgroundBrush",
                "InfoBarInformationalSeverityIconBackground",
                "InfoBarInformationalSeverityIconForeground",
            ),
            Self::Success => (
                "InfoBarSuccessSeverityBackgroundBrush",
                "InfoBarSuccessSeverityIconBackground",
                "InfoBarSuccessSeverityIconForeground",
            ),
            Self::Warning => (
                "InfoBarWarningSeverityBackgroundBrush",
                "InfoBarWarningSeverityIconBackground",
                "InfoBarWarningSeverityIconForeground",
            ),
            Self::Error => (
                "InfoBarErrorSeverityBackgroundBrush",
                "InfoBarErrorSeverityIconBackground",
                "InfoBarErrorSeverityIconForeground",
            ),
        }
    }
}

/// Everything an info bar is built with; the app owns whether it is open.
pub struct InfoBarParams<'a> {
    pub id: Id,
    pub title: &'a str,
    pub message: &'a str,
    pub severity: InfoBarSeverity,
    /// `IsClosable`: whether the trailing `X` is there.
    pub closable: bool,
    /// `IsOpen`: a closed bar takes no space and paints nothing.
    pub open: bool,
}

impl<'a> InfoBarParams<'a> {
    pub fn new(id: Id, title: &'a str, message: &'a str) -> Self {
        Self {
            id,
            title,
            message,
            severity: InfoBarSeverity::Informational,
            closable: true,
            open: true,
        }
    }

    pub fn severity(mut self, severity: InfoBarSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }
}

/// One info bar, resolved against the pack in force where it was built.
pub struct FluentInfoBar {
    id: Id,
    title: Option<Label>,
    message: Option<Label>,
    fill: Color,
    stroke: Color,
    icon_fill: Color,
    icon_ink: Color,
    /// The bare symbol drawn inside the severity disc, and the em size that gives its ink
    /// the reference's share of the disc.
    symbol: Option<IconSource>,
    symbol_size: f32,
    closable: bool,
    open: bool,
    /// The close button's brushes, resolved from its own hover/press state.
    close_fill: Color,
    close_ink: Color,
}

/// A resolved label: WinUI's 14 pt with the weight its slot asks for.
struct Label {
    text: String,
    ink: Color,
    weight: Weight,
}

impl FluentInfoBar {
    pub fn new(
        params: InfoBarParams<'_>,
        node: &mut NodeCx<'_>,
        close: WidgetState,
        pack: &Style,
    ) -> Self {
        let (fill, icon_fill, icon_ink) = params.severity.brushes();
        let state = node.state();
        let slot = if state.disabled {
            3
        } else if close.pressed {
            2
        } else if close.hovered {
            1
        } else {
            0
        };
        let label = |text: &str, ink: Color, weight: Weight| {
            (!text.is_empty()).then(|| Label {
                text: text.into(),
                ink,
                weight,
            })
        };
        // No bare "i" in the vendored face; `informational_mark` draws it instead.
        let (symbol, symbol_size) = match params.severity {
            InfoBarSeverity::Error => (glyphs::icon("dismiss_16_regular"), INFO_BAR_SYMBOL_DISMISS),
            InfoBarSeverity::Warning => (
                glyphs::icon("important_16_regular"),
                INFO_BAR_SYMBOL_IMPORTANT,
            ),
            InfoBarSeverity::Success => {
                (glyphs::icon("checkmark_16_regular"), INFO_BAR_SYMBOL_CHECK)
            }
            InfoBarSeverity::Informational => (None, 0.0),
        };
        Self {
            id: params.id,
            title: label(
                params.title,
                role_of(pack, "InfoBarTitleForeground"),
                Weight::SEMIBOLD,
            ),
            message: label(
                params.message,
                role_of(pack, "InfoBarMessageForeground"),
                Weight::REGULAR,
            ),
            fill: role_of(pack, fill),
            stroke: role_of(pack, "InfoBarBorderBrush"),
            icon_fill: role_of(pack, icon_fill),
            icon_ink: role_of(pack, icon_ink),
            symbol,
            symbol_size,
            closable: params.closable,
            open: params.open,
            close_fill: role_of(
                pack,
                [
                    "SubtleButtonBackground",
                    "SubtleButtonBackgroundPointerOver",
                    "SubtleButtonBackgroundPressed",
                    "SubtleButtonBackgroundDisabled",
                ][slot],
            ),
            close_ink: role_of(
                pack,
                [
                    "SubtleButtonForeground",
                    "SubtleButtonForegroundPointerOver",
                    "SubtleButtonForegroundPressed",
                    "SubtleButtonForegroundDisabled",
                ][slot],
            ),
        }
    }

    /// The resolved fill, icon disc and icon ink, for tests that pin the severity table.
    #[cfg(test)]
    pub(crate) fn severity_colours(&self) -> (Color, Color, Color) {
        (self.fill, self.icon_fill, self.icon_ink)
    }

    /// The 38 pt close button's box inside the bar, for turning a press into a dismissal.
    fn close_box(width: f32) -> Rect {
        Rect::new(
            width - INFO_BAR_CLOSE_MARGIN - INFO_BAR_CLOSE,
            INFO_BAR_CLOSE_MARGIN,
            INFO_BAR_CLOSE,
            INFO_BAR_CLOSE,
        )
    }

    fn label_decl(&self, slot: &'static str, label: &Label, margin: Insets) -> Decl {
        let mut text = ui_widgets::text::leaf(
            self.id.with(slot),
            label.text.clone(),
            INFO_BAR_FONT,
            label.ink,
            label.weight,
            f32::INFINITY,
            0.0,
            0.0,
        );
        text = text.margin(margin);
        text
    }

    /// The severity disc: `StatusCircleOuter` filled, with the symbol reversed out of it.
    fn icon_decl(&self) -> Decl {
        let mut disc = Decl::container(self.id.with("icon"), NodeKind::Stack)
            .size(SizeClass::fixed(INFO_BAR_ICON, INFO_BAR_ICON))
            .stack_align(ui_core::layout::OverlayAlign::Center)
            .margin(INFO_BAR_ICON_MARGIN)
            .background(self.icon_fill, Corners::all(INFO_BAR_ICON * 0.5));
        disc = match self.symbol {
            Some(source) => disc.child(
                Icon {
                    id: self.id.with("glyph"),
                    source,
                    size: self.symbol_size,
                    ink: self.icon_ink,
                }
                .resolve(),
            ),
            None => disc.child(self.informational_mark()),
        };
        disc
    }

    /// The informational "i", composed because the icon face only carries it inside a ring.
    fn informational_mark(&self) -> Decl {
        let bar = |slot: &'static str, h: f32| {
            Decl::container(self.id.with(slot), NodeKind::Box)
                .size(SizeClass::fixed(INFO_BAR_MARK, h))
                .background(self.icon_ink, Corners::all(INFO_BAR_MARK * 0.5))
        };
        // The reference's "i" ink is 1.5×6.5 in the 15 pt circle; the disc's own alignment
        // centres the pair.
        Decl::container(self.id.with("mark"), NodeKind::Column)
            .gap(INFO_BAR_MARK_GAP)
            .child(bar("dot", INFO_BAR_MARK))
            .child(bar("stem", INFO_BAR_STEM))
    }

    /// `InfoBarPanel`: title, message and action share a line until they no longer fit.
    fn panel_decl(&self) -> Decl {
        let mut panel = Decl::container(self.id.with("panel"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                // The grid's `*` column: the panel keeps the room the close button leaves.
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .margin(INFO_BAR_PANEL_MARGIN)
            .cross_align(0.0)
            .cross_pos(0.0)
            // WinUI stacks once a child is taller than the bar's own minimum line.
            .switch_axis(AxisSwitch {
                max_cross: Some(INFO_BAR_MIN_HEIGHT),
            });
        // `InfoBarPanel` drops the first child's leading margin and the last child's trailing
        // one, then adds the orientation padding; both are folded into these margin sets.
        let stacked_bottom = Insets {
            left: 0.0,
            top: PANEL_PAD_TOP,
            right: 0.0,
            bottom: PANEL_PAD_BOTTOM,
        };
        if let Some(title) = &self.title {
            // The title leads either way; only a title-only panel ends on it.
            let alt = if self.message.is_some() {
                Insets {
                    bottom: 0.0,
                    ..stacked_bottom
                }
            } else {
                stacked_bottom
            };
            panel = panel.child(
                self.label_decl("title", title, TITLE_MARGIN)
                    .margin_alt(alt),
            );
        }
        if let Some(message) = &self.message {
            // With a title ahead it keeps the vertical 4 pt gap; alone it leads the panel.
            let (margin, alt) = if self.title.is_some() {
                (MESSAGE_MARGIN, MESSAGE_MARGIN_STACKED)
            } else {
                (
                    Insets {
                        left: 0.0,
                        ..MESSAGE_MARGIN
                    },
                    stacked_bottom,
                )
            };
            panel = panel.child(self.label_decl("message", message, margin).margin_alt(alt));
        }
        panel
    }

    fn close_decl(&self) -> Decl {
        let mut button = Decl::container(self.id.with("close"), NodeKind::Stack)
            .size(SizeClass::fixed(INFO_BAR_CLOSE, INFO_BAR_CLOSE))
            .margin(Insets::all(INFO_BAR_CLOSE_MARGIN))
            // The glyph's em box is 16 of the button's 38; the slot centres it.
            .stack_align(ui_core::layout::OverlayAlign::Center)
            .corners(Corners::all(INFO_BAR_RADIUS))
            .background(self.close_fill, Corners::all(INFO_BAR_RADIUS))
            .sense(Sense::HOVER | Sense::CLICK)
            .cursor(CursorShape::Arrow);
        if let Some(source) = glyphs::icon("dismiss_16_regular") {
            button = button.child(
                Icon {
                    id: self.id.with("close-glyph"),
                    source,
                    size: INFO_BAR_CLOSE_GLYPH,
                    ink: self.close_ink,
                }
                .resolve(),
            );
        }
        button
    }
}

impl Widget for FluentInfoBar {
    fn change(&self, state: &WidgetState, bounds: Rect) -> Option<Change> {
        // The close button has no element of its own, so the press point names it.
        let at = state.local?;
        (self.closable && state.clicked && Self::close_box(bounds.size.w).contains(at))
            .then_some(Change::Activated)
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let id = self.id;
        // `IsOpen=false` collapses the bar: out of flow, so it takes no gap either.
        if !self.open {
            return Decl::container(id, NodeKind::Box)
                .absolute()
                .size(SizeClass::fixed(0.0, 0.0));
        }
        let corners = Corners::all(INFO_BAR_RADIUS);
        // WinUI's grid is `Auto | * | Auto`: the panel takes the slack, so the close button
        // ends up at the bar's right edge instead of right after the message.
        let mut row = Decl::container(id.with("row"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .cross_align(0.0)
            .cross_pos(0.0)
            .child(self.icon_decl())
            .child(self.panel_decl());
        if self.closable {
            row = row.child(self.close_decl());
        }
        Decl::container(id, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(INFO_BAR_MIN_HEIGHT, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(INFO_BAR_PAD)
            .corners(corners)
            .background(self.fill, corners)
            .border(self.stroke, INFO_BAR_BORDER)
            .child(row)
    }
}
