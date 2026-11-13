//! MD3 slider, rebuilt from Flutter's `GappedSliderTrackShape` + `HandleThumbShape` + `trackGap`:
//! track segments split around each handle, a stop dot, tick dots, a state layer.

use ui_core::draw::Shape;
use ui_core::geom::{Color, CornerShape, Corners, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{AnimSpec, CursorShape, Decl, Easing, Id, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::{Cx, NodeCx, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::state_disc;
use crate::paint::{role, spec_ink};
use crate::spec::{SliderFlags, SliderSize, SliderSpec, slider_spec};

/// The trait path's width; a direct caller passes its own.
pub const SLIDER_WIDTH: f32 = 240.0;
/// `MaterialTapTargetSize.padded`: the least a slider may be tall.
pub const SLIDER_TOUCH: f32 = 48.0;
/// `RoundSliderOverlayShape`'s diameter (radius 24).
pub const SLIDER_OVERLAY: f32 = 48.0;

/// The resting state of the base size: its handle width is the gap's reference.
const REST: SliderSpec = slider_spec(
    SliderFlags {
        active: true,
        ..SliderFlags::NONE
    },
    SliderSize::Base,
);

/// `SliderThemeData.trackGap`: the gap from the handle's centre to each track segment.
const GAP: f32 = 6.0;
/// `HandleThumbShape.getPreferredSize().width` at rest.
pub(crate) const HANDLE_W: f32 = REST.handle.width;
/// `trackInsideCornerRadius`: the corner facing the gap.
const INNER_CORNER: f32 = 2.0;

/// `with-tick-marks.container.size`: the tick dot's diameter. The export deprecates the
/// token without shipping its replacement, so this is the only number it carries.
const TICK: f32 = 2.0;

/// The handle's width animation: pressed and focused narrow it from 4 to 2.
const NARROW: AnimSpec = AnimSpec::new(0.1, Easing::Linear);

/// A slider's non-visual options: discrete stops and a second handle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SliderOpts {
    /// The value quantum in 0..=1 units; `None` is continuous.
    pub step: Option<f32>,
    /// Draw a dot at every interior stop; only meaningful with `step`.
    pub ticks: bool,
    /// The trailing handle's value; `Some` makes this a range slider and the leading
    /// handle `value`.
    pub end: Option<f32>,
}

/// A range slider's two values, reported as [`ui_core::Change::Data`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Md3Range {
    pub start: f32,
    pub end: f32,
}

/// The gap for a handle `w` wide: Flutter keeps the visible space at 4dp by shrinking it.
fn gap_for(w: f32) -> f32 {
    GAP - (HANDLE_W - w) * 0.5
}

/// Resolve one state set at a size.
fn resolve(active: bool, lit: bool, pressed: bool, disabled: bool, size: SliderSize) -> SliderSpec {
    slider_spec(
        SliderFlags {
            active,
            hovered: lit,
            focused: lit,
            pressed,
            disabled,
        },
        size,
    )
}

/// One MD3 slider; every animation is advanced in `new_with`, so `decl` stays pure.
///
/// Purity is what makes the fingerprint sound: these fields are everything `decl` reads.
pub struct Md3Slider {
    id: Id,
    value: f32,
    /// The trailing handle's value; `None` is a single-handle slider.
    end: Option<f32>,
    /// The value quantum; `None` is continuous.
    step: Option<f32>,
    ticks: bool,
    width: f32,
    height: f32,
    track_h: f32,
    inset_l: f32,
    inset_r: f32,
    handle: (f32, f32),
    gap: f32,
    /// Size, trailing space, and the two inks of the stop dot.
    stop: (f32, f32, Color, Color),
    /// The tick dot's inks: on the filled span and outside it.
    tick_ink: (Color, Color),
    active_track: Color,
    inactive_track: Color,
    handle_colour: Color,
    overlay: Option<(Color, f32)>,
    disabled: bool,
}

impl Md3Slider {
    /// The unprefixed form: the trait path's width and the smallest named size.
    pub fn new(id: Id, value: f32, _label: &str, pack: &Style) -> Self {
        Self::with_opts(
            id,
            value,
            SLIDER_WIDTH,
            SliderSize::XSmall,
            Md3Options::default(),
            SliderOpts::default(),
            None,
            pack,
        )
    }

    /// Same control with explicit parameters, so the semantic path and a direct caller
    /// produce identical `Decl`s. `width` is the whole control, insets included.
    #[allow(clippy::too_many_arguments)]
    pub fn with_width(
        id: Id,
        value: f32,
        width: f32,
        size: SliderSize,
        params: Md3Options,
        ctx: Option<&mut NodeCx<'_>>,
        pack: &Style,
    ) -> Self {
        Self::with_opts(
            id,
            value,
            width,
            size,
            params,
            SliderOpts::default(),
            ctx,
            pack,
        )
    }

    /// The same control with discrete stops and/or a trailing handle.
    #[allow(clippy::too_many_arguments)]
    pub fn with_opts(
        id: Id,
        value: f32,
        width: f32,
        size: SliderSize,
        params: Md3Options,
        opts: SliderOpts,
        ctx: Option<&mut NodeCx<'_>>,
        pack: &Style,
    ) -> Self {
        let v = value.clamp(0.0, 1.0);
        let width = width.max(0.0);
        let step = opts.step.filter(|s| *s > 0.0);
        let end = opts.end.map(|e| e.clamp(v, 1.0));
        let state = ctx.as_ref().map(|c| c.state()).unwrap_or_default();
        let disabled = state.disabled || params.disabled;
        let lit = state.hovered || state.focus_visible;
        let pressed = state.pressed || state.dragging;
        let on = resolve(true, lit, pressed, disabled, size);
        let off = resolve(false, lit, pressed, disabled, size);

        let mut handle_w = on.handle.width;
        let mut overlay = None;
        if let Some(ctx) = ctx {
            handle_w = ctx.animate(id.with(param::WIDTH), on.handle.width, NARROW);
            // The disc is pinned to the thumb and follows it, so it grows in place rather than
            // splatting at the pointer the way a button's ripple does.
            overlay = state_disc(
                ctx,
                id,
                SLIDER_OVERLAY,
                disabled,
                pressed,
                |hovered, focused, pressed| {
                    // The export carries no state layer for the slider; these are Flutter's M3
                    // `overlayColor` values.
                    let primary = role(pack, "primary");
                    let a = if pressed || focused {
                        0.1
                    } else if hovered {
                        0.08
                    } else {
                        0.0
                    };
                    Color {
                        a: primary.a * a,
                        ..primary
                    }
                },
            );
        }
        let handle_h = on.handle.height;
        Self {
            id,
            value: v,
            end,
            step,
            ticks: opts.ticks && step.is_some(),
            width,
            height: handle_h.max(SLIDER_TOUCH),
            track_h: on.track.height,
            // The export splits the two sides across the active and inactive states; they agree.
            inset_l: on.track_shape.leading,
            inset_r: off.track_shape.trailing,
            handle: (handle_w, handle_h),
            gap: gap_for(handle_w),
            stop: (
                off.stop_indicator.size,
                off.stop_indicator.trailing_space,
                spec_ink(
                    pack,
                    off.stop_indicator_container.color,
                    off.stop_indicator_container.opacity,
                ),
                spec_ink(
                    pack,
                    on.stop_indicator_container.color,
                    on.stop_indicator_container.opacity,
                ),
            ),
            tick_ink: if disabled {
                // `with-tick-marks.disabled.container.*`: onSurface at 38%, both sides.
                (tick_ink(pack, "onSurface"), tick_ink(pack, "onSurface"))
            } else {
                // `with-tick-marks.{active,inactive}.container.*` in the export.
                (
                    tick_ink(pack, "onPrimary"),
                    tick_ink(pack, "onSurfaceVariant"),
                )
            },
            active_track: spec_ink(pack, on.track.color, on.track.opacity),
            inactive_track: spec_ink(pack, off.track.color, off.track.opacity),
            handle_colour: spec_ink(pack, on.handle.color, on.handle.opacity),
            overlay,
            disabled,
        }
    }

    /// The value a pointer at `x` (slider-local) selects, before quantising.
    pub fn value_at(&self, x: f32) -> f32 {
        let travel = self.width - self.inset_l - self.inset_r;
        if travel <= 0.0 {
            return 0.0;
        }
        ((x - self.inset_l) / travel).clamp(0.0, 1.0)
    }

    /// `v` snapped to the nearest stop when the slider is discrete.
    fn quantize(&self, v: f32) -> f32 {
        match self.step {
            Some(s) => ((v / s).round() * s).clamp(0.0, 1.0),
            None => v,
        }
    }

    /// Where a value's handle centre sits, in the slider's own space.
    fn x_of(&self, v: f32) -> f32 {
        self.inset_l + (self.width - self.inset_l - self.inset_r) * v
    }

    /// Where the leading handle's centre sits, in the slider's own space.
    pub fn handle_x(&self) -> f32 {
        self.x_of(self.value)
    }
}

/// A tick dot's ink: a contrasting role at 38%, as the tick-mark tokens specify.
fn tick_ink(pack: &Style, role_name: &'static str) -> Color {
    let c = role(pack, role_name);
    Color { a: c.a * 0.38, ..c }
}

impl Md3Slider {
    /// One dot per interior stop, skipping the ends (the rounded caps swallow them) and any
    /// dot a handle would cover.
    fn push_ticks(
        &self,
        shapes: &mut Vec<Shape>,
        centre: f32,
        trailing: Option<f32>,
        left: f32,
        right: f32,
    ) {
        let Some(step) = self.step.filter(|_| self.ticks) else {
            return;
        };
        let divisions = (1.0 / step).round();
        if divisions < 1.0 {
            return;
        }
        // Flutter skips the whole set when the dots would crowd together.
        if (right - left) / divisions < TICK * 3.0 {
            return;
        }
        let (hw, _) = self.handle;
        let covered = hw * 0.5 + TICK * 0.5;
        let mid = self.height * 0.5;
        for i in 1..divisions as i32 {
            let x = self.x_of(i as f32 * step);
            if (x - centre).abs() < covered || trailing.is_some_and(|t| (x - t).abs() < covered) {
                continue;
            }
            let inside = match trailing {
                None => x < centre,
                Some(t) => x > centre && x < t,
            };
            let fill = if inside {
                self.tick_ink.0
            } else {
                self.tick_ink.1
            };
            shapes.push(Shape::Rect {
                rect: Rect::new(x - TICK * 0.5, mid - TICK * 0.5, TICK, TICK),
                corners: Corners::all(TICK * 0.5),
                fill,
                gradient: None,
                border: None,
            });
        }
    }
}

impl Widget for Md3Slider {
    fn handles_disabled(&self) -> bool {
        // The token table paints disabled (12%/38%) itself; the adapter must not fade again.
        true
    }

    fn cache_key(&self) -> Option<ui_core::CachedKey> {
        // Every field `decl` reads. A field added here and forgotten below is a change
        // that never reaches the screen.
        let fp = ui_widgets::fingerprint!(
            self.value,
            self.end,
            self.step,
            self.ticks,
            self.width,
            self.height,
            self.track_h,
            self.inset_l,
            self.inset_r,
            self.handle,
            self.gap,
            self.stop,
            self.tick_ink,
            self.active_track,
            self.inactive_track,
            self.handle_colour,
            self.overlay,
            self.disabled,
        );
        Some(ui_core::CachedKey::new(self.id, fp))
    }

    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<ui_core::Change> {
        if self.disabled || !(state.pressed || state.dragging) {
            return None;
        }
        // The press point arrives in this slider's own space, so the control maps it to a
        // value itself; the app only ever sees the result.
        let x = state.local?.x;
        let picked = self.quantize(self.value_at(x));
        let Some(prev_end) = self.end else {
            return ((picked - self.value).abs() > 1e-4)
                .then_some(ui_core::Change::Value(picked as f64));
        };
        // The nearer handle takes the pointer; a moving handle stays nearest to itself, so
        // the choice is stable for the whole drag without any state of our own.
        let (start, end) = if (x - self.x_of(self.value)).abs() <= (x - self.x_of(prev_end)).abs() {
            (picked.min(prev_end), prev_end)
        } else {
            (self.value, picked.max(self.value))
        };
        ((start - self.value).abs() > 1e-4 || (end - prev_end).abs() > 1e-4)
            .then_some(ui_core::Change::Data(Box::new(Md3Range { start, end })))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let h = self.track_h;
        let mid = self.height * 0.5;
        let top = mid - h * 0.5;
        let left = self.inset_l;
        let right = (self.width - self.inset_r).max(left);
        let centre = self.handle_x();
        let trailing = self.end.map(|e| self.x_of(e));
        let gap = self.gap;
        let outer = h * 0.5;
        let mut shapes = Vec::with_capacity(8);

        // Each segment stops a gap short of a handle's centre, with a circular outer corner
        // and a 2dp inner one. Only the span between the handles is active: for a range the
        // leading segment is outside the selection, so it takes the inactive ink.
        let leading = if trailing.is_some() {
            self.inactive_track
        } else {
            self.active_track
        };
        let mut segs: Vec<(f32, f32, Color, bool, bool)> = vec![
            (left, centre - gap, leading, false, true),
            (centre + gap, right, self.inactive_track, true, false),
        ];
        if let Some(t) = trailing {
            segs[1] = (centre + gap, t - gap, self.active_track, true, true);
            segs.push((t + gap, right, self.inactive_track, true, false));
        }
        for (x0, x1, fill, inner_l, inner_r) in segs {
            let (x0, x1) = (x0.max(left), x1.min(right));
            if x1 - x0 <= 0.0 {
                continue;
            }
            shapes.push(Shape::Rect {
                rect: Rect::new(x0, top, x1 - x0, h),
                corners: Corners {
                    tl: if inner_l { INNER_CORNER } else { outer },
                    tr: if inner_r { INNER_CORNER } else { outer },
                    br: if inner_r { INNER_CORNER } else { outer },
                    bl: if inner_l { INNER_CORNER } else { outer },
                    shape: CornerShape::ROUND,
                },
                fill,
                gradient: None,
                border: None,
            });
        }
        self.push_ticks(&mut shapes, centre, trailing, left, right);
        // A stop dot marks each end of the track, keeping `trailing-space` clear of the edge and
        // taking the active ink once the filled span has run over it. Discrete sliders show tick
        // dots instead; a range gets one dot per end.
        let (stop_size, stop_space, stop_idle, stop_active) = self.stop;
        if stop_size > 0.0 && self.step.is_none() {
            let half = stop_size * 0.5;
            let (lo, hi) = match trailing {
                Some(t) => (centre, t),
                None => (left, centre),
            };
            let mut dots = vec![(
                right - stop_space - half,
                hi + gap >= right - stop_space - half,
            )];
            if trailing.is_some() {
                dots.push((
                    left + stop_space + half,
                    lo - gap <= left + stop_space + half,
                ));
            }
            for (c, filled) in dots {
                shapes.push(Shape::Rect {
                    rect: Rect::new(c - half, mid - half, stop_size, stop_size),
                    corners: Corners::all(half),
                    fill: if filled { stop_active } else { stop_idle },
                    gradient: None,
                    border: None,
                });
            }
        }
        if let Some((colour, radius)) = self.overlay {
            shapes.push(Shape::Rect {
                rect: Rect::new(centre - radius, mid - radius, radius * 2.0, radius * 2.0),
                corners: Corners::all(radius),
                fill: colour,
                gradient: None,
                border: None,
            });
        }
        // `RRect.fromRectAndRadius(..., thumbSize.shortestSide / 2)`: a capsule.
        let (hw, hh) = self.handle;
        for c in std::iter::once(centre).chain(trailing) {
            shapes.push(Shape::Rect {
                rect: Rect::new(c - hw * 0.5, mid - hh * 0.5, hw, hh),
                corners: Corners::all(hw * 0.5),
                fill: self.handle_colour,
                gradient: None,
                border: None,
            });
        }

        Decl::container(self.id, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // A slider takes hover, click, drag and focus; the app writes none of this.
            .sense(Sense::HOVER | Sense::CLICK | Sense::DRAG | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .shapes(shapes)
    }
}
