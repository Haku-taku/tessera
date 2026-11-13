//! A surface's material: what to do with the pixels behind it, and what to fall back to.

use ui_core::geom::Color;

/// The fill recipe one surface draws with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Material {
    /// Opaque paint; nothing behind is read.
    Solid(Color),
    /// Translucent fill over a blurred backdrop, with an opaque stand-in for when the
    /// backend cannot sample or the user asked for reduced transparency.
    Frost {
        /// Gaussian sigma of the backdrop blur.
        sigma: f32,
        /// The tint laid over the blurred pixels.
        tint: Color,
        /// What to draw instead, with nothing sampled behind it.
        fallback: Color,
    },
    /// WinUI acrylic: luminosity wash, colour tint, grain over the blurred backdrop.
    Acrylic {
        /// Gaussian sigma of the backdrop blur.
        sigma: f32,
        /// The acrylic recipe itself.
        params: ui_core::draw::Acrylic,
        /// What to draw instead, with nothing sampled behind it.
        fallback: Color,
    },
    /// WinUI Mica: a wallpaper sample under a theme-tinted layer. No blur, no grain, and
    /// opaque with respect to app content — content behind it does not show through.
    Mica {
        /// Stable id of the wallpaper texture to sample.
        source: u64,
        /// The layer laid over it; `a` is WinUI's mica layer opacity.
        tint: Color,
        /// `MicaAlt`: the stronger layer WinUI uses behind tabbed title bars.
        alt: bool,
    },
    /// Glass: LiquidGlassKit's lens, reading a capture of the backdrop at its own density.
    Glass {
        /// Gaussian sigma of the backdrop blur.
        sigma: f32,
        /// The glass recipe itself.
        params: ui_core::draw::LiquidGlass,
        /// What to draw instead, with nothing sampled behind it.
        fallback: Color,
    },
}

impl Material {
    /// A named standard content material the design system declared, as an engine surface.
    ///
    /// The system supplies the numbers and the fallback; this layer only decides how the
    /// engine draws them, so a system that has no material family answers `None` instead of
    /// a made-up blur.
    pub fn from_spec(spec: ui_design::MaterialSpec) -> Self {
        Material::Frost {
            sigma: spec.sigma,
            tint: colour_of(spec.tint),
            fallback: colour_of(spec.fallback),
        }
    }

    /// The colour an opaque surface would have had; used by the fallback path.
    pub fn fallback_color(&self) -> Color {
        match self {
            Material::Solid(c) => *c,
            Material::Frost { fallback, .. }
            | Material::Acrylic { fallback, .. }
            | Material::Glass { fallback, .. } => *fallback,
            // Mica never falls back: without a wallpaper it is the tint over the base.
            Material::Mica { tint, .. } => *tint,
        }
    }

    /// How the blurred backdrop should be shaded, if this material samples one.
    pub fn backdrop(&self) -> Option<ui_core::draw::BackdropMaterial> {
        match self {
            Material::Solid(_) | Material::Frost { .. } => None,
            Material::Acrylic { params, .. } => {
                Some(ui_core::draw::BackdropMaterial::Acrylic(*params))
            }
            Material::Glass { params, .. } => {
                Some(ui_core::draw::BackdropMaterial::LiquidGlass(*params))
            }
            // Mica is painted from an image plus a tint, not filtered from the backdrop.
            Material::Mica { .. } => None,
        }
    }

    /// Gaussian sigma of the backdrop blur, if this material asks for one.
    pub fn sigma(&self) -> Option<f32> {
        match self {
            Material::Solid(_) => None,
            Material::Frost { sigma, .. }
            | Material::Acrylic { sigma, .. }
            | Material::Glass { sigma, .. } => Some(*sigma),
            // No blur: mica is a wallpaper sample, so there is nothing to filter.
            Material::Mica { .. } => None,
        }
    }

    /// Whether this material needs pixels from behind the widget.
    pub fn needs_backdrop(&self) -> bool {
        self.sigma().is_some()
    }
}

fn colour_of(rgba: ui_design::Rgba) -> Color {
    let argb = rgba.argb();
    Color::from_rgba8(
        (argb >> 16) as u8,
        (argb >> 8) as u8,
        argb as u8,
        (argb >> 24) as u8,
    )
}
