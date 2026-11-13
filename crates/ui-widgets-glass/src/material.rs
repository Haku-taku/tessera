//! The glass presets: optics from LiquidGlassKit, body from a measurement of Apple's glass.
//!
//! LiquidGlassKit ships one shader and three presets: the thumb (`LiquidGlassView.swift:56-78`),
//! the tab-bar lens (`:81-104`), and the regular panel (`:106-124`). Its regular panel stands
//! in for Apple's luminance lift with a pale-blue wash; Theseus measured the real transfer.

use ui_core::draw::{BLEND_SHAPES, BlendShape, GlassBody, LiquidGlass};
use ui_core::geom::Color;
use ui_design::Appearance;
use ui_style::Style;

/// Which control the material is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Switch,
    Slider,
    /// The bar's own capsule: a wide panel behind the moving lens.
    TabBar,
    /// The capsule's moving lens.
    TabLens,
    Button,
    ButtonProminent,
    /// The segmented control's moving lens.
    SegmentLens,
    Menu,
    /// A card or plain container.
    Container,
}

/// The material a control wears.
pub fn glass_recipe(pack: &Style, control: Control) -> LiquidGlass {
    match control {
        Control::Switch => thumb(0.75),
        Control::Slider => thumb(1.0),
        Control::TabLens | Control::SegmentLens => lens(),
        // `.glassProminent` is the same body at higher strength, not a brand tint: thicker,
        // more opaque, brighter (`glass_button.dart:893-901`).
        // (`glass_button.dart:893-901`).
        Control::ButtonProminent => {
            let mut glass = regular(pack);
            glass.thickness = (glass.thickness * 2.5).clamp(30.0, 100.0);
            glass.tint = Color {
                a: 0.18,
                ..Color::WHITE
            };
            glass.glare_intensity = (glass.glare_intensity * 1.5).clamp(0.3, 1.0);
            glass
        }
        _ => regular(pack),
    }
}

/// The pressed knob: a 10 dp rim at index 1.11, and a 10 % lift with no hue of its own.
fn thumb(magnification: f32) -> LiquidGlass {
    LiquidGlass {
        body: GlassBody {
            mix: 0.10,
            ..GlassBody::CLEAR
        },
        tint: Color::TRANSPARENT,
        refractive_index: 1.11,
        thickness: 10.0,
        capture_size: 1.0 / magnification,
        capture_scale: magnification,
        magnification,
        frost: 0.0,
        dispersion: 5.0,
        fresnel_intensity: 0.0,
        fresnel_distance: 70.0,
        fresnel_sharpness: 0.0,
        glare_intensity: 0.01,
        glare_distance: 30.0,
        glare_sharpness: -0.2,
        glare_convergence: 0.0,
        glare_bias: 0.0,
        glare_offset: std::f32::consts::PI * 0.9,
        blend: [BlendShape::NONE; BLEND_SHAPES],
        blend_count: 0,
        blend_k: 0.0,
    }
}

/// `LiquidGlass.lens` (`LiquidGlassView.swift:81-104`): a clear 6 dp lens that shows what lies
/// under it 1:1, sampled at native density so nothing under it is downsampled.
fn lens() -> LiquidGlass {
    LiquidGlass {
        body: GlassBody::CLEAR,
        tint: Color::TRANSPARENT,
        refractive_index: 1.1,
        thickness: 6.0,
        capture_size: 1.0,
        capture_scale: 1.0,
        magnification: 1.0,
        frost: 0.0,
        dispersion: 15.0,
        fresnel_intensity: 0.0,
        fresnel_distance: 70.0,
        fresnel_sharpness: 0.0,
        glare_intensity: 0.1,
        glare_distance: 30.0,
        glare_sharpness: -0.1,
        glare_convergence: 0.1,
        glare_bias: 1.0,
        glare_offset: -std::f32::consts::FRAC_PI_4,
        blend: [BlendShape::NONE; BLEND_SHAPES],
        blend_count: 0,
        blend_k: 0.0,
    }
}

/// Apple's regular glass (`TheseusMaterialModel.swift:8-22`): light lifts luminance 53 % toward
/// white and takes 6 % of the chroma; dark lifts shadows through a tone curve instead.
fn regular(pack: &Style) -> LiquidGlass {
    let dark = pack.appearance() == Some(Appearance::Dark);
    LiquidGlass {
        body: if dark {
            GlassBody {
                mix: 0.0,
                pole: 0.1125,
                chroma: 1.043,
                tone: 1.0,
            }
        } else {
            GlassBody {
                mix: 0.53,
                pole: 0.99,
                chroma: 0.937,
                tone: 0.0,
            }
        },
        tint: Color::TRANSPARENT,
        refractive_index: 1.5,
        thickness: 10.0,
        capture_size: 1.0,
        // A full-density capture, frosted by a real blur: a coarse capture upscaled with
        // bilinear is what made the backdrop look blocky.
        capture_scale: 1.0,
        magnification: 1.0,
        frost: 10.0,
        dispersion: 5.0,
        fresnel_intensity: 0.0,
        fresnel_distance: 70.0,
        fresnel_sharpness: 0.0,
        glare_intensity: 0.1,
        glare_distance: 30.0,
        glare_sharpness: -0.15,
        glare_convergence: 0.1,
        glare_bias: 1.0,
        glare_offset: -std::f32::consts::FRAC_PI_4,
        blend: [BlendShape::NONE; BLEND_SHAPES],
        blend_count: 0,
        blend_k: 0.0,
    }
}
