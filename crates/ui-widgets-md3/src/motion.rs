//! Motion values every MD3 control shares.

use ui_core::{AnimSpec, Easing};

/// `kRadialReactionDuration`: hover, focus and press state-layer fades.
pub(crate) const REACTION_SECONDS: f32 = 0.1;

/// The state-layer clock. A control that fades hover, focus or press uses this,
/// never a literal, so the whole set reacts at one speed.
pub(crate) const REACTION: AnimSpec = AnimSpec::new(REACTION_SECONDS, Easing::Linear);

/// `Curves.fastOutSlowIn`, Material's utility curve for state and head/tail motion.
pub(crate) const FAST_OUT_SLOW_IN: Easing = Easing::Cubic(0.4, 0.0, 0.2, 1.0);

/// MD3's `standard`: a transition that begins and ends on screen.
pub(crate) const STANDARD: Easing = Easing::Cubic(0.2, 0.0, 0.0, 1.0);

/// The pair MD3 gives such a transition: standard at 300 ms.
pub(crate) const STANDARD_TRANSITION: AnimSpec = AnimSpec::new(0.3, STANDARD);

/// MD3's `emphasizedAccelerate`, the leaving half of the emphasized pair.
pub(crate) const EMPHASIZED_ACCELERATE: Easing = Easing::Cubic(0.3, 0.0, 0.8, 0.15);

/// A navigation destination's indicator: the bar, rail and drawer share one scale-in.
///
/// `NavigationBar.animationDuration` is 500 ms on `Curves.easeInOutCubicEmphasized` — the
/// full emphasized curve, because the indicator begins and ends on screen.
pub(crate) const DESTINATION_SELECT: AnimSpec =
    AnimSpec::new(0.5, Easing::Piece(&ui_style::motion::EMPHASIZED));
