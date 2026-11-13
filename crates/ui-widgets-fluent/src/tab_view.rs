//! The Fluent tab strip: 32 dp headers over a hairline; the selected one joins the page.

use ui_core::draw::FocusVisual;
use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Clip, Grow, Len, LenInsets, Overflow, SizeClass};
use ui_core::{Change, CursorShape, Decl, Id, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::text::Text;
use ui_widgets::{Container, Element, IconSource, NodeCx, Widget, WidgetState};

use crate::glyphs;
use crate::paint::role_of;

/// `TabViewHeaderPadding`: the strip leaves room above the headers for the hairline.
pub const STRIP_TOP_PAD: f32 = 8.0;
/// `TabViewItemMinHeight` / `TabViewItemMinWidth` / `TabViewItemMaxWidth`.
pub const TAB_HEIGHT: f32 = 32.0;
pub const TAB_MIN_WIDTH: f32 = 100.0;
pub const TAB_MAX_WIDTH: f32 = 240.0;
/// `OverlayCornerRadius`, applied to the header's top corners only.
pub const TAB_RADIUS: f32 = 8.0;
/// `TabViewItemHeaderFontSize`; the selected header is `SemiBold` in the template.
pub const TAB_FONT: f32 = 12.0;
/// `TabViewItemHeaderIconSize` and `TabViewItemHeaderIconMargin` 0,0,10,0.
pub const TAB_ICON: f32 = 16.0;
pub const TAB_ICON_GAP: f32 = 10.0;
/// `TabViewItemHeaderPaddingWithCloseButton` 8,3,4,3 / `...WithoutCloseButton` 8,3,8,3.
pub const TAB_PAD_LEFT: f32 = 8.0;
pub const TAB_PAD_TOP: f32 = 3.0;
pub const TAB_PAD_RIGHT_CLOSE: f32 = 4.0;
pub const TAB_PAD_RIGHT: f32 = 8.0;
/// `TabViewItemHeaderCloseButtonWidth` / `...Height` and `TabViewItemHeaderCloseMargin`.
pub const CLOSE_WIDTH: f32 = 32.0;
pub const CLOSE_HEIGHT: f32 = 24.0;
pub const CLOSE_GAP: f32 = 4.0;
pub const CLOSE_FONT: f32 = 12.0;
/// `TabViewItemAddButtonWidth` / `...Height` and `TabViewItemAddButtonContainerPadding`.
pub const ADD_WIDTH: f32 = 32.0;
pub const ADD_HEIGHT: f32 = 24.0;
pub const ADD_PAD_LEFT: f32 = 3.0;
pub const ADD_PAD_BOTTOM: f32 = 3.0;
/// `TabViewItemSeparatorMargin` 0,8,0,8 leaves a 16 dp hairline in a 32 dp header.
pub const SEPARATOR_HEIGHT: f32 = 16.0;
/// `TabViewItemHeaderCloseButtonSize`: the glyph inside the 32×24 button.
pub const CLOSE_GLYPH_SIZE: f32 = 16.0;

/// `TabViewItemScrollButtonWidth` / `...Height` / `...ButonFontSize` and the 50 dp step.
pub const SCROLL_WIDTH: f32 = 32.0;
pub const SCROLL_HEIGHT: f32 = 24.0;
pub const SCROLL_FONT: f32 = 8.0;
pub const SCROLL_STEP: f32 = 50.0;
/// The buttons are `RepeatButton`s with `Delay="50" Interval="100"` in the reference.
pub const SCROLL_REPEAT_DELAY: f32 = 0.050;
pub const SCROLL_REPEAT_INTERVAL: f32 = 0.100;

/// How many steps a press has earned by holding for `held` seconds.
///
/// A `RepeatButton` clicks once as it is pressed, again after `Delay`, then every `Interval`.
pub fn scroll_repeat_steps(held: f32) -> f32 {
    if held < SCROLL_REPEAT_DELAY {
        1.0
    } else {
        2.0 + ((held - SCROLL_REPEAT_DELAY) / SCROLL_REPEAT_INTERVAL).floor()
    }
}
/// `TabViewItemLeftScrollButtonContainerPadding` 8,0,3,3 / `...Right...` 3,0,8,3.
pub const SCROLL_PAD_LEFT: Insets = Insets {
    left: 8.0,
    top: 0.0,
    right: 3.0,
    bottom: 3.0,
};
pub const SCROLL_PAD_RIGHT: Insets = Insets {
    left: 3.0,
    top: 0.0,
    right: 8.0,
    bottom: 3.0,
};

/// What a tab reports besides a selection: its close button, or where it was dragged to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabViewChange {
    Closed,
    /// A header was dragged from `from` into slot `to`, both in the current order.
    /// `to == count` is the gap after the last header; the app owns the order and moves it.
    Dropped {
        from: usize,
        to: usize,
    },
}

/// One header's inputs.
pub struct TabViewItemParams<'a> {
    pub id: Id,
    pub label: &'a str,
    pub icon: Option<IconSource>,
    pub closable: bool,
}

impl<'a> TabViewItemParams<'a> {
    pub fn new(id: Id, label: &'a str) -> Self {
        Self {
            id,
            label,
            icon: None,
            closable: false,
        }
    }

    pub fn icon(mut self, icon: IconSource) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn closable(mut self, on: bool) -> Self {
        self.closable = on;
        self
    }
}

/// The whole strip's inputs; the app owns the selected index and the tab list.
pub struct TabViewParams<'a> {
    pub id: Id,
    pub tabs: Vec<TabViewItemParams<'a>>,
    pub selected: Option<usize>,
    pub add_button: bool,
}

impl<'a> TabViewParams<'a> {
    pub fn new(id: Id, tabs: Vec<TabViewItemParams<'a>>) -> Self {
        Self {
            id,
            tabs,
            selected: None,
            add_button: false,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = Some(index);
        self
    }

    pub fn add_button(mut self, on: bool) -> Self {
        self.add_button = on;
        self
    }
}

/// One tab header; its close button is resolved here, its selection reported here.
pub struct FluentTabViewItem {
    id: Id,
    label: String,
    icon: Option<IconSource>,
    closable: bool,
    selected: bool,
    disabled: bool,
    fill: Color,
    ink: Color,
    icon_ink: Color,
    weight: Weight,
    close_fill: Color,
    close_ink: Color,
    /// The hairline at this header's right edge, already faded when it must not show.
    separator: Color,
    ring: FocusVisual,
    /// Where a dragged header is drawn: the sum of the gesture's own steps, because the
    /// press point travels with the node and would otherwise stop measuring.
    drag: ui_core::geom::Point,
    /// A header being dragged floats above the ones it crosses.
    lifted: bool,
    /// The dragged header's centre in slot coordinates, read on the frame it lands.
    drop_center: Option<f32>,
    /// Where this header sits in the strip, and how far a live drag pushes it aside.
    place: TabPlace,
}

/// Where a header sits in the strip: its own slot, every slot, and a live push.
///
/// Slots are cumulative widths from the first header, so they are the *layout* boundaries:
/// an offset moves a header on screen without moving its slot.
#[derive(Clone)]
pub struct TabPlace {
    pub index: usize,
    /// This header's slot start, measured from the first header's.
    pub start: f32,
    /// This header's own slot width.
    pub span: f32,
    /// Every slot boundary, `count + 1` entries; the last is the content width.
    pub edges: std::sync::Arc<[f32]>,
    /// How far a live drag pushes this header aside, in the strip's own pixels.
    pub slide: f32,
}

/// The slot a strip-content x names; the last slot owns everything past its end.
pub fn slot_at(edges: &[f32], x: f32) -> usize {
    let mut k = 0;
    while k + 1 < edges.len() && x >= edges[k + 1] {
        k += 1;
    }
    k
}

/// The close button's own pointer state; it is a `Decl` child, so the strip reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloseState {
    pub hovered: bool,
    pub pressed: bool,
}

impl FluentTabViewItem {
    pub fn new(
        params: TabViewItemParams<'_>,
        selected: bool,
        separator_visible: bool,
        close: CloseState,
        place: TabPlace,
        ctx: &mut NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let TabViewItemParams {
            id,
            label,
            icon,
            closable,
        } = params;
        let state = ctx.state();
        let disabled = state.disabled;
        // `CommonStates` is exclusive, and a selected header outranks hover and press.
        let slot = if disabled {
            3
        } else if selected {
            2
        } else if state.pressed {
            4
        } else if state.hovered {
            1
        } else {
            0
        };
        let fill = role_of(
            pack,
            [
                "TabViewItemHeaderBackground",
                "TabViewItemHeaderBackgroundPointerOver",
                "TabViewItemHeaderBackgroundSelected",
                "TabViewItemHeaderBackgroundDisabled",
                "TabViewItemHeaderBackgroundPressed",
            ][slot],
        );
        let ink = role_of(
            pack,
            [
                "TabViewItemHeaderForeground",
                "TabViewItemHeaderForegroundPointerOver",
                "TabViewItemHeaderForegroundSelected",
                "TabViewItemHeaderForegroundDisabled",
                "TabViewItemHeaderForegroundPressed",
            ][slot],
        );
        let icon_ink = role_of(
            pack,
            [
                "TabViewItemIconForeground",
                "TabViewItemIconForegroundPointerOver",
                "TabViewItemIconForegroundSelected",
                "TabViewItemIconForegroundDisabled",
                "TabViewItemIconForegroundPressed",
            ][slot],
        );
        // The close button answers its own pointer, not the header's: WinUI lights it with
        // `...CloseButtonBackgroundPointerOver` only while the pointer is on the button.
        let close_slot = if disabled {
            3
        } else if close.pressed {
            1
        } else if close.hovered {
            2
        } else {
            0
        };
        let close_ink = role_of(
            pack,
            [
                "TabViewItemHeaderCloseButtonForeground",
                "TabViewItemHeaderCloseButtonForegroundPressed",
                "TabViewItemHeaderCloseButtonForegroundPointerOver",
                "TabViewItemHeaderDisabledCloseButtonForeground",
            ][close_slot],
        );
        let close_fill = role_of(
            pack,
            [
                "TabViewItemHeaderCloseButtonBackground",
                "TabViewItemHeaderCloseButtonBackgroundPressed",
                "TabViewItemHeaderCloseButtonBackgroundPointerOver",
                "TabViewItemHeaderDisabledCloseButtonBackground",
            ][close_slot],
        );
        let separator = role_of(pack, "TabViewItemSeparator");
        // The reference hides the hairline on hover, on the selected header, and on the one
        // just to its left (`SetTabSeparatorOpacity`).
        let separator = if separator_visible && !(state.hovered || state.pressed) {
            separator
        } else {
            Color::TRANSPARENT
        };
        // Past the click threshold the header lifts and follows the finger; before that the
        // press is still a candidate click and nothing moves.
        let lifted = !disabled && state.dragging && state.drag_started;
        let x_id = id.with("drag-x");
        let y_id = id.with("drag-y");
        let center_id = id.with("drag-center");
        let held = ui_core::geom::Point::new(ctx.value(x_id), ctx.value(y_id));
        let drag = if lifted {
            let step = state.drag_step.unwrap_or_default();
            let x = ctx.animate(x_id, held.x + step.x, ui_widgets::build::IMMEDIATE);
            let y = ctx.animate(y_id, held.y + step.y, ui_widgets::build::IMMEDIATE);
            ui_core::geom::Point::new(x, y)
        } else {
            ctx.animate(x_id, 0.0, ui_widgets::build::IMMEDIATE);
            ctx.animate(y_id, 0.0, ui_widgets::build::IMMEDIATE);
            ui_core::geom::Point::new(0.0, 0.0)
        };
        // The drop follows the header's own centre, not the pointer: a header is grabbed
        // anywhere, and its offset is already zero on the landing frame.
        let drop_center = if lifted {
            let center = place.start + place.span * 0.5 + drag.x;
            ctx.animate(center_id, center, ui_widgets::build::IMMEDIATE);
            Some(center)
        } else {
            let center = ctx.value(center_id);
            ctx.animate(center_id, -1.0, ui_widgets::build::IMMEDIATE);
            // The band test lives in `change`: only it sees the release point in this node's
            // own space, while a build sees the drag owner's.
            (state.dropped && center >= 0.0).then_some(center)
        };
        Self {
            id,
            label: label.to_string(),
            icon,
            closable,
            selected,
            disabled,
            fill,
            ink,
            icon_ink,
            weight: if selected {
                Weight::SEMIBOLD
            } else {
                Weight::REGULAR
            },
            close_fill,
            close_ink,
            separator,
            drag,
            lifted,
            drop_center,
            place,
            ring: FocusVisual {
                outer: role_of(pack, "FocusStrokeColorOuterBrush"),
                outer_width: 2.0,
                inner: role_of(pack, "FocusStrokeColorInnerBrush"),
                inner_width: 1.0,
                margin: Insets::ZERO,
                radius: 4.0,
            },
        }
    }

    /// The close button's box in the header's own space, from the right edge inwards.
    fn close_box(bounds: Rect) -> Rect {
        Rect::new(
            bounds.size.w - CLOSE_GAP - CLOSE_WIDTH,
            (bounds.size.h - CLOSE_HEIGHT) * 0.5,
            CLOSE_WIDTH,
            CLOSE_HEIGHT,
        )
    }
}

impl Widget for FluentTabViewItem {
    fn handles_disabled(&self) -> bool {
        // The template paints its own disabled inks; the adapter must not fade twice.
        true
    }

    fn change(&self, state: &WidgetState, bounds: Rect) -> Option<Change> {
        if !self.disabled && state.dropped {
            // Released outside the strip's own band: the gesture is abandoned, not dropped.
            let at = state.local?;
            if at.y < 0.0 || at.y >= TAB_HEIGHT {
                return None;
            }
            // The landing comes from the dragged header's centre over the slot geometry:
            // the pointer may be anywhere on the header, and a release inside the open gap
            // lands *between* headers, where no node takes a drop.
            let to = slot_at(&self.place.edges, self.drop_center?);
            return (to != self.place.index).then(|| {
                Change::Data(Box::new(TabViewChange::Dropped {
                    from: self.place.index,
                    to,
                }))
            });
        }
        if self.disabled || !state.clicked {
            return None;
        }
        let at = state.local?;
        // The close button sits at a fixed offset from the right edge, so the press point
        // names it without a lookup.
        if self.closable && Self::close_box(bounds).contains(at) {
            return Some(Change::Data(Box::new(TabViewChange::Closed)));
        }
        // Re-picking the header that is already selected is not a change.
        (!self.selected).then_some(Change::Selected(Some(self.id)))
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let id = self.id;
        let right = if self.closable {
            TAB_PAD_RIGHT_CLOSE
        } else {
            TAB_PAD_RIGHT
        };
        let height = TAB_HEIGHT;
        let mut root = Decl::container(id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(TAB_MIN_WIDTH, TAB_MAX_WIDTH),
                height: AxisSize::fixed(height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: TAB_PAD_LEFT,
                top: TAB_PAD_TOP,
                right,
                bottom: TAB_PAD_TOP,
            })
            .corners(Corners {
                tl: TAB_RADIUS,
                tr: TAB_RADIUS,
                br: 0.0,
                bl: 0.0,
                ..Corners::ZERO
            })
            .background(
                self.fill,
                Corners {
                    tl: TAB_RADIUS,
                    tr: TAB_RADIUS,
                    br: 0.0,
                    bl: 0.0,
                    ..Corners::ZERO
                },
            )
            // A header can be dragged to a new place and is itself a place to drop one.
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE | Sense::DRAG | Sense::DROP)
            .cursor(CursorShape::Arrow)
            .cross_align(0.0)
            .cross_pos(0.5)
            .focus_visual(Some(self.ring))
            // An offset, not a margin: the slots must stay where they were laid out, or the
            // reflow would feed its own preview back into the next frame's geometry.
            .offset(self.drag.x + self.place.slide, self.drag.y)
            // The header is the containing block of its own overlays: the strip is the
            // nearest positioned ancestor otherwise, and the separator would hang off its
            // edge instead of this header's.
            .relative();
        // A dragged header keeps its slot but paints above the strip, clear of its scroll clip.
        if self.lifted {
            root = root.lifted();
        }
        // The hairline is the first child so every later layer paints over it.
        root = root.child(
            Decl::container(id.with("sep"), NodeKind::Box)
                .absolute()
                .inset(LenInsets::new(
                    Len::Auto,
                    Len::Px((height - SEPARATOR_HEIGHT) * 0.5),
                    Len::Px(0.0),
                    Len::Auto,
                ))
                .size(SizeClass::fixed(1.0, SEPARATOR_HEIGHT))
                .background(self.separator, Corners::ZERO),
        );
        if let Some(icon) = self.icon {
            root = root.child(
                icon.resolve_with_size(id.with("icon"), self.icon_ink, TAB_ICON)
                    .margin(Insets {
                        right: TAB_ICON_GAP,
                        ..Insets::ZERO
                    }),
            );
        }
        // The label is the grid's `*` column: it takes the slack a `TAB_MIN_WIDTH` header
        // leaves, which is what keeps the close button against the right edge.
        let mut label = Text::new(id.with("label"), self.label.clone(), TAB_FONT, self.ink)
            .weight(self.weight)
            .nowrap()
            .resolve();
        label.size_class.grow_x = Grow::Yes;
        root = root.child(label);
        if self.closable
            && let Some(glyph) = glyphs::codepoint("dismiss_12_regular")
        {
            let close = Decl::container(id.with("close"), NodeKind::Stack)
                .size(SizeClass::fixed(CLOSE_WIDTH, CLOSE_HEIGHT))
                .margin(Insets {
                    left: CLOSE_GAP,
                    ..Insets::ZERO
                })
                .corners(Corners::all(4.0))
                .background(self.close_fill, Corners::all(4.0))
                .sense(Sense::HOVER | Sense::CLICK)
                .cursor(CursorShape::Arrow)
                // The glyph is smaller than the 32x24 slot: a Stack centres it on both
                // axes, so no padding arithmetic can drift it off centre.
                .stack_align(ui_core::layout::OverlayAlign::Center)
                .child(
                    Text::new(
                        id.with("close-glyph"),
                        glyph.to_string(),
                        CLOSE_FONT,
                        self.close_ink,
                    )
                    .family(ui_core::Family::Named(glyphs::ICON_FAMILY))
                    .resolve(),
                );
            root = root.child(close);
        }
        // No bottom overlay: the header's own fill already covers the strip's hairline under
        // it, and an extra 1 dp bar only drew a second line in the header's colour.
        root
    }
}

/// The add button at the strip's tail: a 32×24 tile, not a tab.
pub struct FluentTabAddButton {
    id: Id,
    fill: Color,
    ink: Color,
    glyph: Option<char>,
}

impl FluentTabAddButton {
    pub fn new(id: Id, ctx: &mut NodeCx<'_>, pack: &Style) -> Self {
        let state = ctx.state();
        let slot = if state.disabled {
            3
        } else if state.pressed {
            2
        } else if state.hovered {
            1
        } else {
            0
        };
        Self {
            id,
            fill: role_of(
                pack,
                [
                    "TabViewButtonBackground",
                    "TabViewButtonBackgroundPointerOver",
                    "TabViewButtonBackgroundPressed",
                    "TabViewButtonBackgroundDisabled",
                ][slot],
            ),
            ink: role_of(
                pack,
                [
                    "TabViewButtonForeground",
                    "TabViewButtonForegroundPointerOver",
                    "TabViewButtonForegroundPressed",
                    "TabViewButtonForegroundDisabled",
                ][slot],
            ),
            glyph: glyphs::codepoint("add_12_regular"),
        }
    }
}

impl Widget for FluentTabAddButton {
    fn handles_disabled(&self) -> bool {
        true
    }

    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<Change> {
        (!state.disabled && state.clicked).then_some(Change::Activated)
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let id = self.id;
        let mut button = Decl::container(id, NodeKind::Row)
            .size(SizeClass::fixed(ADD_WIDTH, ADD_HEIGHT))
            .margin(Insets {
                left: ADD_PAD_LEFT,
                // 8 dp of header padding plus the container's 3 dp bottom padding leaves the
                // 24 dp tile 13 dp below the strip's top edge.
                top: STRIP_TOP_PAD + TAB_HEIGHT - ADD_HEIGHT - ADD_PAD_BOTTOM,
                ..Insets::ZERO
            })
            .corners(Corners::all(4.0))
            .background(self.fill, Corners::all(4.0))
            .sense(Sense::HOVER | Sense::CLICK)
            .cursor(CursorShape::Arrow)
            .cross_align(0.0)
            .cross_pos(0.5);
        if let Some(glyph) = self.glyph {
            button = button.child(glyph_box(
                id.with("glyph"),
                glyph,
                CLOSE_FONT,
                self.ink,
                ADD_WIDTH,
            ));
        }
        button
    }
}

/// `TabViewItemScrollButtonPadding` 7,3,7,3: the glyph's box inside the 32×24 button.
const SCROLL_GLYPH_PAD: f32 = 7.0;
const SCROLL_GLYPH_TOP: f32 = 3.0;

/// A glyph centred in a box of `width`; a bare text child would sit on its line box's top.
fn glyph_box(id: Id, glyph: char, size: f32, ink: Color, width: f32) -> Decl {
    Decl::container(id, NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fixed(width),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .cross_align(0.0)
        .cross_pos(0.5)
        .child(
            Text::new(id.with("text"), glyph.to_string(), size, ink)
                .family(ui_core::Family::Named(glyphs::ICON_FAMILY))
                .resolve(),
        )
}

/// A strip scroll button; it appears only while the headers overflow the strip.
pub struct FluentTabScrollButton {
    id: Id,
    left: bool,
    enabled: bool,
    /// The strip's scrolling list: the button sits beside it, not inside it, so the
    /// request must name it instead of leaning on the nearest owning ancestor.
    target: Id,
    fill: Color,
    ink: Color,
    /// Steps this frame owes: one at the press, then the repeat's cadence while held.
    fresh: f32,
}

impl FluentTabScrollButton {
    pub fn new(
        id: Id,
        target: Id,
        left: bool,
        enabled: bool,
        ctx: &mut NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let state = ctx.state();
        let slot = if !enabled || state.disabled {
            3
        } else if state.pressed {
            2
        } else if state.hovered {
            1
        } else {
            0
        };
        // A held button repeats; the age of its own id is the hold's clock, and reading it
        // asks for a frame each pass, so the cadence runs without pointer movement.
        let held_id = id.with("held");
        let paid_id = id.with("paid");
        let paid = ctx.value(paid_id);
        let live = enabled && !state.disabled;
        let fresh = if live && state.pressed {
            let steps = scroll_repeat_steps(ctx.age(held_id));
            let fresh = (steps - paid).max(0.0);
            if fresh > 0.0 {
                ctx.animate(paid_id, steps, ui_widgets::build::IMMEDIATE);
            }
            fresh
        } else {
            // A tap that began and ended within one frame has no press frame to report from.
            let owed = (live && state.clicked && paid <= 0.0) as u8 as f32;
            ctx.forget_age(held_id);
            if paid > 0.0 {
                ctx.animate(paid_id, 0.0, ui_widgets::build::IMMEDIATE);
            }
            owed
        };
        Self {
            id,
            left,
            enabled,
            target,
            fresh,
            fill: role_of(
                pack,
                [
                    "TabViewScrollButtonBackground",
                    "TabViewScrollButtonBackgroundPointerOver",
                    "TabViewScrollButtonBackgroundPressed",
                    "TabViewScrollButtonBackgroundDisabled",
                ][slot],
            ),
            ink: role_of(
                pack,
                [
                    "TabViewScrollButtonForeground",
                    "TabViewScrollButtonForegroundPointerOver",
                    "TabViewScrollButtonForegroundPressed",
                    "TabViewScrollButtonForegroundDisabled",
                ][slot],
            ),
        }
    }
}

impl Widget for FluentTabScrollButton {
    fn handles_disabled(&self) -> bool {
        true
    }

    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<Change> {
        if !self.enabled || state.disabled || self.fresh <= 0.0 {
            return None;
        }
        let step = if self.left { -SCROLL_STEP } else { SCROLL_STEP };
        Some(Change::ScrollBy {
            dx: f64::from(step * self.fresh),
            dy: 0.0,
            owner: Some(self.target),
        })
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let id = self.id;
        let pad = if self.left {
            SCROLL_PAD_LEFT
        } else {
            SCROLL_PAD_RIGHT
        };
        let inner = SCROLL_WIDTH - 2.0 * SCROLL_GLYPH_PAD;
        let mut button = Decl::container(id, NodeKind::Row)
            .size(SizeClass::fixed(SCROLL_WIDTH, SCROLL_HEIGHT))
            // The template's container padding becomes a margin: the strip has none.
            .margin(Insets {
                left: pad.left,
                top: STRIP_TOP_PAD + TAB_HEIGHT - SCROLL_HEIGHT - pad.bottom,
                right: pad.right,
                bottom: 0.0,
            })
            .padding(Insets {
                left: SCROLL_GLYPH_PAD,
                top: SCROLL_GLYPH_TOP,
                right: SCROLL_GLYPH_PAD,
                bottom: SCROLL_GLYPH_TOP,
            })
            .corners(Corners::all(4.0))
            .background(self.fill, Corners::all(4.0))
            .sense(Sense::HOVER | Sense::CLICK)
            .cursor(CursorShape::Arrow)
            .disabled(!self.enabled)
            .cross_align(0.0)
            .cross_pos(0.5);
        let name = if self.left {
            "chevron_left_12_regular"
        } else {
            "chevron_right_12_regular"
        };
        if let Some(glyph) = glyphs::codepoint(name) {
            button = button.child(glyph_box(
                id.with("glyph"),
                glyph,
                SCROLL_FONT,
                self.ink,
                inner,
            ));
        }
        button
    }
}

/// Compose the strip: the hairline, the headers in order, and the optional add button.
///
/// Returns an element rather than one widget because each header reports its own click;
/// a single sensed root would have to guess which header the point fell in.
pub fn tab_view(
    params: TabViewParams<'_>,
    cx: &mut ui_widgets::Cx<'_, '_>,
    pack: &Style,
) -> Box<dyn Element> {
    let TabViewParams {
        id,
        tabs,
        selected,
        add_button,
    } = params;
    let count = tabs.len();
    // The strip itself only positions; the headers live in a row that scrolls sideways when
    // they overflow, exactly as the reference's `ScrollViewer` does.
    let mut strip = Container::new(id, NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fixed(STRIP_TOP_PAD + TAB_HEIGHT),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        })
        .cross_align(0.0)
        .cross_pos(0.0)
        .map(|d| d.relative());
    strip = strip.child(ui_widgets::from_decl(
        Decl::container(id.with("line"), NodeKind::Box)
            .absolute()
            .inset(LenInsets::new(
                Len::Px(0.0),
                Len::Auto,
                Len::Px(0.0),
                Len::Px(0.0),
            ))
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::fixed(1.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .background(role_of(pack, "TabViewBorderBrush"), Corners::ZERO),
    ));
    // The buttons exist only while the list overflows, and their enablement follows how far
    // it has scrolled; the reference drives both from the scroll viewer's own state.
    let list_id = id.with("tabs");
    let geom = cx.core().scroll_of(list_id);
    let overflowing = geom.is_some_and(|g| g.extent_w > g.viewport_w + 0.5);
    let (offset, max_scroll) = geom
        .map(|g| (g.offset_x, (g.extent_w - g.viewport_w).max(0.0)))
        .unwrap_or((0.0, 0.0));
    // The list is as wide as its headers and no wider: the add button sits right after the
    // last one until the headers overflow, where the list shrinks and scrolls instead.
    let mut tabs_row = Container::new(list_id, NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fixed(STRIP_TOP_PAD + TAB_HEIGHT),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .padding(Insets {
            top: STRIP_TOP_PAD,
            ..Insets::ZERO
        })
        .cross_align(0.0)
        .cross_pos(0.0)
        .map(|d| {
            d.clip(Clip {
                x: Overflow::Scroll,
                y: Overflow::Hidden,
            })
            .sense(Sense::HOVER | Sense::SCROLL_OWNED)
        });
    // A live drag reflows the strip around the dragged header's centre: the header floats,
    // the headers between its slot and that centre slide over, and the gap opens there.
    let dragged = tabs.iter().position(|t| {
        let s = cx.state(t.id);
        s.dragging && s.drag_started
    });
    // Geometry is read while a header floats and on the frame one lands, and no other time:
    // it is a volatile dependency, and a strip at rest must keep its cached decl.
    let landing = tabs.iter().any(|t| cx.state(t.id).dropped);
    let edges: std::sync::Arc<[f32]> = if dragged.is_some() || landing {
        let mut edges: Vec<f32> = Vec::with_capacity(count + 1);
        edges.push(0.0);
        for t in &tabs {
            let w = cx.rect_of(t.id).map_or(0.0, |r| r.size.w);
            let last = edges.last().copied().unwrap_or(0.0);
            edges.push(last + w);
        }
        edges.into()
    } else {
        std::sync::Arc::from(Vec::new())
    };
    let target = dragged.and_then(|from| {
        // The dragged header's own centre names the gap, and it travels with the offset the
        // header holds right now: the stored one plus this frame's step.
        let applied = cx.core().value(tabs[from].id.with("drag-x"));
        let step = cx.state(tabs[from].id).drag_step.unwrap_or_default();
        let start = edges.get(from).copied().unwrap_or(0.0);
        let span = edges.get(from + 1).copied().unwrap_or(start) - start;
        let to = slot_at(&edges, start + span * 0.5 + applied + step.x);
        (to != from).then_some(to)
    });
    // Every header between the old slot and the new one moves over by the dragged header's
    // own width, which is exactly the width of the gap it leaves behind.
    let dragged_w = dragged
        .map(|from| {
            edges.get(from + 1).copied().unwrap_or(0.0) - edges.get(from).copied().unwrap_or(0.0)
        })
        .unwrap_or(0.0);
    let env = cx.env_handle().clone();
    for (k, item) in tabs.into_iter().enumerate() {
        let slide = match (dragged, target) {
            (Some(from), Some(to)) if k > from && k <= to => -dragged_w,
            (Some(from), Some(to)) if k >= to && k < from => dragged_w,
            _ => 0.0,
        };
        let is_selected = selected == Some(k);
        // `SetTabSeparatorOpacity`: hidden on the selected header and the one before it.
        let separator = !is_selected && selected != Some(k + 1) && k + 1 < count;
        let tab_id = item.id;
        // The close button is a `Decl` child, so its pointer state is read here, off the
        // strip's own context, rather than invented from the header's state.
        let close_id = tab_id.with("close");
        let close = CloseState {
            hovered: cx.core().hovered() == Some(close_id),
            pressed: cx.core().pressed() == Some(close_id),
        };
        let mut node = cx.at(tab_id);
        let start = edges.get(k).copied().unwrap_or(0.0);
        let place = TabPlace {
            index: k,
            start,
            span: edges.get(k + 1).copied().unwrap_or(start) - start,
            edges: edges.clone(),
            slide,
        };
        let header =
            FluentTabViewItem::new(item, is_selected, separator, close, place, &mut node, pack);
        drop(node);
        tabs_row = tabs_row.child(Box::new(ui_widgets::Styled::new(
            Box::new(header),
            env.clone(),
        )));
    }
    // Left button, list, right button: the same order as the reference's grid columns.
    if overflowing {
        let scroll_id = id.with("scroll-left");
        let mut node = cx.at(scroll_id);
        let button =
            FluentTabScrollButton::new(scroll_id, list_id, true, offset > 0.1, &mut node, pack);
        drop(node);
        strip = strip.child(Box::new(ui_widgets::Styled::new(
            Box::new(button),
            env.clone(),
        )));
    }
    strip = strip.child(Box::new(tabs_row));
    if overflowing {
        let scroll_id = id.with("scroll-right");
        let mut node = cx.at(scroll_id);
        let button = FluentTabScrollButton::new(
            scroll_id,
            list_id,
            false,
            offset < max_scroll - 0.1,
            &mut node,
            pack,
        );
        drop(node);
        strip = strip.child(Box::new(ui_widgets::Styled::new(
            Box::new(button),
            env.clone(),
        )));
    }
    if add_button {
        let add_id = id.with("add");
        let mut node = cx.at(add_id);
        let button = FluentTabAddButton::new(add_id, &mut node, pack);
        drop(node);
        strip = strip.child(Box::new(ui_widgets::Styled::new(Box::new(button), env)));
    }
    Box::new(strip)
}
