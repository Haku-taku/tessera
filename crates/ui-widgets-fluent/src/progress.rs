//! The Fluent progress bar: a 3 dp slot, a 1 dp track and a rounded indicator.

use ui_core::geom::{Color, Corners};
use ui_core::layout::{AxisSize, Clip, Grow, Len, LenInsets, Overflow, SizeClass};
use ui_core::{Decl, Easing, Id, NodeKind};
use ui_style::Style;
use ui_widgets::{NodeCx, Widget};

use crate::paint::role_of;

/// `ProgressBarMinHeight`: the slot is 3 dp tall.
pub const BAR_HEIGHT: f32 = 3.0;
/// `ProgressBarTrackHeight`: the empty track is one hairline.
pub const TRACK_HEIGHT: f32 = 1.0;
/// `ProgressBarCornerRadius` / `ProgressBarTrackCornerRadius`.
pub const BAR_RADIUS: f32 = 1.5;
pub const TRACK_RADIUS: f32 = 0.5;
/// The indeterminate storyboard runs on a two-second clock.
pub const CYCLE: f32 = 2.0;
/// The first bar is 40% of the width; the second 60% (`ProgressBar.cpp`).
pub const FIRST_WIDTH: f32 = 40.0;
pub const SECOND_WIDTH: f32 = 60.0;
/// `0.4,0.0,0.6,1.0` is the key spline both sweeps use.
pub const SWEEP: Easing = Easing::Cubic(0.4, 0.0, 0.6, 1.0);

/// WinUI's `ShowError` / `ShowPaused`, or neither.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProgressStatus {
    #[default]
    Normal,
    Error,
    Paused,
}

/// One progress bar's inputs; the value is the caller's, the bar reports nothing.
#[derive(Clone, Copy, Debug)]
pub struct ProgressBarParams {
    pub id: Id,
    /// `0.0..=1.0`; the caller clamps, the bar paints what it is handed.
    pub value: f32,
    pub indeterminate: bool,
    pub status: ProgressStatus,
}

impl ProgressBarParams {
    pub fn new(id: Id, value: f32) -> Self {
        Self {
            id,
            value: value.clamp(0.0, 1.0),
            indeterminate: false,
            status: ProgressStatus::Normal,
        }
    }

    pub fn indeterminate(mut self, on: bool) -> Self {
        self.indeterminate = on;
        self
    }

    pub fn status(mut self, status: ProgressStatus) -> Self {
        self.status = status;
        self
    }
}

/// One Fluent progress bar; the sweep phase is read here so `decl()` stays pure.
pub struct FluentProgressBar {
    id: Id,
    value: f32,
    indeterminate: bool,
    fill: Color,
    track: Color,
    /// Left edges as percentages of the width, in the reference's own units.
    first_x: f32,
    second_x: f32,
}

impl FluentProgressBar {
    pub fn new(params: ProgressBarParams, ctx: &mut NodeCx<'_>, pack: &Style) -> Self {
        let ProgressBarParams {
            id,
            value,
            indeterminate,
            status,
        } = params;
        // An indeterminate bar is the only case that owns a clock; a determinate one must
        // let the frame loop rest, so it never asks for the time.
        let age = if indeterminate {
            ctx.age(id.with("phase"))
        } else {
            0.0
        };
        let fill = match status {
            ProgressStatus::Normal => role_of(pack, "ProgressBarForeground"),
            ProgressStatus::Error => role_of(pack, "ProgressBarErrorForegroundColor"),
            ProgressStatus::Paused => role_of(pack, "ProgressBarPausedForegroundColor"),
        };
        Self {
            id,
            value,
            indeterminate,
            fill,
            track: role_of(pack, "ProgressBarBackground"),
            first_x: first_sweep(age),
            second_x: second_sweep(age),
        }
    }
}

impl Widget for FluentProgressBar {
    fn handles_disabled(&self) -> bool {
        // WinUI's template has no disabled state; nothing else may fade the bar either.
        true
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let id = self.id;
        let slot = SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fixed(BAR_HEIGHT),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        };
        // The root clips to its own rect, exactly as `TemplateSettings.ClipRect` does.
        let root = Decl::container(id, NodeKind::Box)
            // The root is the containing block the percentages resolve against.
            .relative()
            .size(slot)
            .corners(Corners::all(BAR_RADIUS))
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            });
        if self.indeterminate {
            return root
                .child(bar(id.with("first"), self.fill, self.first_x, FIRST_WIDTH))
                .child(bar(
                    id.with("second"),
                    self.fill,
                    self.second_x,
                    SECOND_WIDTH,
                ));
        }
        // The track is a hairline centred in the slot; the indicator fills its height.
        let centre = (BAR_HEIGHT - TRACK_HEIGHT) * 0.5;
        let track = Decl::container(id.with("track"), NodeKind::Box)
            .absolute()
            .inset(LenInsets::new(
                Len::Px(0.0),
                Len::Px(centre),
                Len::Px(0.0),
                Len::Auto,
            ))
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::fixed(TRACK_HEIGHT),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .background(self.track, Corners::all(TRACK_RADIUS));
        root.child(track)
            .child(bar(id.with("value"), self.fill, 0.0, self.value * 100.0))
    }
}

/// One rounded bar: left edge at `x_pct` of the slot, `width_pct` wide, full height.
///
/// Both edges are percentages, so the bar stretches between them without the layout ever
/// needing to know the slot's pixel width — the CSS way to place by fraction.
fn bar(id: Id, fill: Color, x_pct: f32, width_pct: f32) -> Decl {
    Decl::container(id, NodeKind::Box)
        .absolute()
        .inset(LenInsets::new(
            Len::Pct(x_pct),
            Len::Px(0.0),
            Len::Pct(100.0 - x_pct - width_pct),
            Len::Px(0.0),
        ))
        .size(SizeClass {
            width: AxisSize::hug(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .background(fill, Corners::all(BAR_RADIUS))
}

/// Seconds into the two-second loop.
pub fn phase(age: f32) -> f32 {
    ui_core::anim::phase(age, CYCLE) * CYCLE
}

/// First bar's left edge: -40% to 120% over 1.5 s, then held until the cycle ends.
pub fn first_sweep(age: f32) -> f32 {
    let t = (phase(age) / 1.5).clamp(0.0, 1.0);
    FIRST_WIDTH * (-1.0 + 4.0 * SWEEP.apply(t))
}

/// Second bar: holds at -90% for 0.75 s, then reaches 99.6% at the cycle's end.
pub fn second_sweep(age: f32) -> f32 {
    let t = phase(age);
    if t < 0.75 {
        -1.5 * SECOND_WIDTH
    } else {
        let p = ((t - 0.75) / 1.25).clamp(0.0, 1.0);
        SECOND_WIDTH * (-1.5 + 3.16 * SWEEP.apply(p))
    }
}
