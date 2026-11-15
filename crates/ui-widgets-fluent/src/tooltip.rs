//! The Fluent tooltip: a 12 pt text panel on acrylic, hung above its anchor.

use ui_core::BackdropBlur;
use ui_core::anim::{AnimSpec, Easing};
use ui_core::draw::{Acrylic, BackdropMaterial};
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{
    Anchor, AxisSize, Grow, OverlayAlign, OverlayPlacement, Presentation, Side, SizeClass,
};
use ui_core::{Decl, Id, NodeKind};
use ui_style::Style;
use ui_widgets::text::Text;
use ui_widgets::{NodeCx, Widget};

use crate::material::{AcrylicRecipe, acrylic_dark, acrylic_light};
use crate::paint::role_of;

/// `ToolTipContentThemeFontSize`, `ToolTipBorderPadding`, `ToolTipMaxWidth`, `ControlCornerRadius`.
pub const TOOLTIP_FONT: f32 = 12.0;
pub const TOOLTIP_PAD: Insets = Insets {
    left: 9.0,
    top: 6.0,
    right: 9.0,
    bottom: 8.0,
};
pub const TOOLTIP_MAX_WIDTH: f32 = 320.0;
pub const TOOLTIP_RADIUS: f32 = 4.0;
/// `ToolTipBorderThemeThickness`.
pub const TOOLTIP_BORDER: f32 = 1.0;
/// `FadeInThemeAnimation`/`FadeOutThemeAnimation` on `OpenStates`, at
/// `ControlFastAnimationDuration`.
pub const TOOLTIP_FADE: AnimSpec = AnimSpec::new(0.167, Easing::Linear);

/// The text and the anchor a tooltip hangs from.
pub struct ToolTipParams<'a> {
    pub id: Id,
    pub text: &'a str,
    pub anchor: Anchor,
    /// `ToolTip::DefaultPlacementMode` is `Top`; the reference flips when it would not fit.
    pub side: Side,
    /// `ToolTip::IsOpen`: the panel fades between the two states and paints nothing closed.
    pub open: bool,
}

impl<'a> ToolTipParams<'a> {
    pub fn new(id: Id, text: &'a str, anchor: Anchor) -> Self {
        Self {
            id,
            text,
            anchor,
            side: Side::Above,
            open: true,
        }
    }

    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }
}

/// One tooltip panel: acrylic under 12 pt text, anchored and flipped by the engine.
pub struct FluentToolTip {
    id: Id,
    text: String,
    ink: Color,
    stroke: Color,
    anchor: Anchor,
    side: Side,
    acrylic: Acrylic,
    blur_sigma: f32,
    /// The `OpenStates` fade, `0..=1`; a closed panel paints nothing.
    alpha: f32,
}

impl FluentToolTip {
    pub fn new(params: ToolTipParams<'_>, node: &mut NodeCx<'_>, pack: &Style) -> Self {
        // `AcrylicBrush_themeresources.xaml`: dark `#2C2C2C/0.15/0.96`, light `#FCFCFC/0/0.85`.
        let recipe: AcrylicRecipe = match pack.appearance() {
            Some(ui_design::Appearance::Dark) => acrylic_dark(),
            _ => acrylic_light(),
        };
        let alpha = node.animate(params.id.with("fade"), f32::from(params.open), TOOLTIP_FADE);
        Self {
            id: params.id,
            text: params.text.to_string(),
            ink: role_of(pack, "ToolTipForegroundBrush"),
            stroke: role_of(pack, "SurfaceStrokeColorFlyoutBrush"),
            anchor: params.anchor,
            side: params.side,
            acrylic: Acrylic {
                tint: recipe.tint,
                luminosity: recipe.luminosity,
                noise: recipe.noise,
            },
            blur_sigma: recipe.blur_sigma,
            alpha,
        }
    }

    /// This frame's `OpenStates` fade; the app keeps the panel mounted until it reaches 0.
    pub fn alpha(&self) -> f32 {
        self.alpha
    }
}

impl Widget for FluentToolTip {
    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let id = self.id;
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
        // A fully faded panel paints nothing, so the app can keep it mounted across the fade.
        // It stays an overlay: an in-flow stand-in would take a slot and shift the page.
        if self.alpha <= 0.0 {
            return Decl::container(id, NodeKind::Box)
                .size(SizeClass::fixed(0.0, 0.0))
                .overlay(placement);
        }
        let corners = Corners::all(TOOLTIP_RADIUS);
        let mut panel = Decl::container(id, NodeKind::Box)
            // `MaxWidth` caps the whole panel, padding included; the text wraps inside.
            .size(SizeClass {
                width: AxisSize::hug(0.0, TOOLTIP_MAX_WIDTH),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(TOOLTIP_PAD)
            .corners(corners)
            // The acrylic pass shades this box; without a box shape there is nothing to shade.
            .background(Color::TRANSPARENT, corners)
            .border(self.stroke, TOOLTIP_BORDER)
            .opacity(self.alpha)
            .overlay(placement);
        // Acrylic shades this backdrop on the GPU; the CPU path keeps the plain blur.
        if let Some(p) = panel.data.params_mut() {
            p.backdrop_blur = Some(BackdropBlur {
                radius: (3.0_f32 * self.blur_sigma).ceil().max(1.0) as u32,
                sigma: self.blur_sigma,
                material: BackdropMaterial::Acrylic(self.acrylic),
            });
        }
        panel.child(
            Text::new(
                id.with("text"),
                self.text.to_string(),
                TOOLTIP_FONT,
                self.ink,
            )
            .max_width(TOOLTIP_MAX_WIDTH - TOOLTIP_PAD.left - TOOLTIP_PAD.right)
            .resolve(),
        )
    }
}
