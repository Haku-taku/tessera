//! Motion tokens: named durations and curves that widgets reference instead of literals.

use ui_core::anim::{AnimSpec, Easing, PieceCubic};

/// A slot every widget resolves its animation through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionName {
    /// State swaps: hover, focus, press.
    StateChange,
    /// Small popups opening.
    Quick,
    /// Standard transitions: expand, page swap.
    Standard,
    /// Large or prominent motion.
    Slow,
    /// A colour or brush swap. WinUI's `BrushTransition` is linear where its transform
    /// transitions use the fast-out-slow-in spline, so the two cannot share a slot.
    ColourChange,
}

/// One entry: duration in seconds and a curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionToken {
    pub spec: AnimSpec,
}

impl MotionToken {
    pub fn new(duration: f32, easing: Easing) -> Self {
        Self {
            spec: AnimSpec::new(duration, easing),
        }
    }
}

/// A whole style's motion vocabulary; swap the table, the widgets change feel.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionTable {
    state_change: MotionToken,
    quick: MotionToken,
    standard: MotionToken,
    slow: MotionToken,
    colour_change: MotionToken,
}

impl MotionTable {
    pub fn get(&self, name: MotionName) -> &MotionToken {
        match name {
            MotionName::StateChange => &self.state_change,
            MotionName::Quick => &self.quick,
            MotionName::Standard => &self.standard,
            MotionName::Slow => &self.slow,
            MotionName::ColourChange => &self.colour_change,
        }
    }

    /// The legacy feel currently painted in the examples: 150 ms ease-out everywhere.
    pub fn legacy() -> Self {
        let quick = MotionToken::new(0.15, Easing::EaseOut);
        Self {
            state_change: quick,
            quick,
            standard: quick,
            slow: quick,
            colour_change: quick,
        }
    }

    /// The WinUI durations 83/167/250 ms plus Flutter's 358 ms slow, on `EaseInOut`.
    pub fn fluent() -> Self {
        let curve = Easing::EaseInOut;
        Self {
            state_change: MotionToken::new(0.083, curve),
            quick: MotionToken::new(0.167, curve),
            standard: MotionToken::new(0.250, curve),
            slow: MotionToken::new(0.358, curve),
            colour_change: MotionToken::new(0.083, curve),
        }
    }

    /// WinUI 3's own durations and spline, from `Common_themeresources_any.xaml`.
    ///
    /// `ControlFastOutSlowInKeySpline` is `0,0,0,1`; brush transitions are linear at 83 ms.
    pub fn winui() -> Self {
        let spline = WINUI_SPLINE;
        Self {
            state_change: MotionToken::new(0.083, spline),
            quick: MotionToken::new(0.167, spline),
            standard: MotionToken::new(0.250, spline),
            // WinUI's longest control animation is the normal 250 ms one; no 358 ms slot.
            slow: MotionToken::new(0.250, spline),
            colour_change: MotionToken::new(0.083, Easing::Linear),
        }
    }

    /// MD3 buckets: 100 ms state layers, 200 ms/300 ms standard, 400 ms decelerate.
    ///
    /// The 300 ms slot is the standard pair; 400 ms is `emphasizedDecelerate`, the curve an
    /// element entering on screen uses, not the full two-part emphasized curve. State layers
    /// fade linearly at `kRadialReactionDuration` (100 ms), so `state_change` shares the
    /// colour-change curve.
    pub fn material() -> Self {
        let standard = Easing::Cubic(0.2, 0.0, 0.0, 1.0);
        Self {
            state_change: MotionToken::new(0.100, Easing::Linear),
            quick: MotionToken::new(0.200, standard),
            standard: MotionToken::new(0.300, standard),
            slow: MotionToken::new(0.400, EMPHASIZED_DECELERATE),
            // MD3 state layers fade linearly; `kRadialReactionDuration` is the 100 ms clock.
            colour_change: MotionToken::new(0.100, Easing::Linear),
        }
    }

    /// All four slots share one token; a pack uses this then dials individual slots.
    pub fn uniform(token: MotionToken) -> Self {
        Self {
            state_change: token,
            quick: token,
            standard: token,
            slow: token,
            colour_change: token,
        }
    }
}

/// Material's emphasized curve, for a transition that begins and ends on screen.
///
/// Piecewise because one cubic cannot express it: Flutter's `Curves.easeInOutCubicEmphasized`.
/// The entering element's curve is the separate `emphasizedDecelerate` token (`0.05, 0.7,
/// 0.1, 1`), which one cubic does express.
pub static EMPHASIZED: PieceCubic = PieceCubic {
    a1: (0.05, 0.0),
    b1: (0.133_333, 0.06),
    mid: (0.166_666, 0.4),
    a2: (0.208_333, 0.82),
    b2: (0.25, 1.0),
};

/// Material's `emphasizedDecelerate`, the single-cubic curve an element enters on.
pub const EMPHASIZED_DECELERATE: Easing = Easing::Cubic(0.05, 0.7, 0.1, 1.0);

/// WinUI's `ControlFastOutSlowInKeySpline`, the curve every WinUI control transition uses.
pub const WINUI_SPLINE: Easing = Easing::Cubic(0.0, 0.0, 0.0, 1.0);

/// CSS `ease-out`, `cubic-bezier(0, 0, 0.58, 1)`; `Easing::EaseOut` is the Penner curve.
pub const CSS_EASE_OUT: Easing = Easing::Cubic(0.0, 0.0, 0.58, 1.0);

/// Flutter's `Curves.easeOutBack`, MD3's overshoot for a thumb or sheet that springs past
/// its stop; also the `easeOutBack` row of the dimensions table.
pub const EASE_OUT_BACK: Easing = Easing::Cubic(0.175, 0.885, 0.32, 1.275);

