//! Style selection and the one-frame channel a style builds through.

use ui_style::{SliderSpec, ToggleSpec};

use crate::scaffold::ScaffoldSpec;
use crate::widget::Widget;

/// A style kit: a design language asked for concrete controls; the engine only sees the `Decl`.
///
/// None of these take a scope argument: the scope travels on the declaration tree, so a
/// control reads the one in force where it is built. Passing it by hand let a caller hold
/// two of them at once, which is exactly the bug this shape removes.
pub trait StyleKit {
    /// A slider for `spec`.
    fn slider(&self, spec: &SliderSpec<'_>, cx: &mut crate::cx::Cx<'_, '_>) -> Box<dyn Widget>;

    /// A toggle for `spec`.
    fn toggle(&self, spec: &ToggleSpec<'_>, cx: &mut crate::cx::Cx<'_, '_>) -> Box<dyn Widget>;

    /// A page skeleton that places the six regions itself.
    fn scaffold(&self, spec: &ScaffoldSpec<'_>, cx: &mut crate::cx::Cx<'_, '_>) -> Box<dyn Widget>;
}
