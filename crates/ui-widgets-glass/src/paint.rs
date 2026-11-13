//! Pack lookups the controls share.

use ui_core::geom::Color;
use ui_style::Style;

pub(crate) fn role_of(pack: &Style, name: &'static str) -> Color {
    // Curated role first, then the live native palette; a miss is a bug, shown as magenta.
    let c = pack.ink(name);
    debug_assert!(c.is_some(), "neither the pack nor its palette knows {name}");
    c.unwrap_or(Color::from_rgba8(255, 0, 255, 255))
}

/// The corner radius on a selection's ends, matching this pack's pill radii.
pub const SELECTION_RADIUS: f32 = 3.0;

/// Glass's selection: rounded ends rather than a hard rectangle.
#[derive(Debug)]
pub struct GlassSelection;

impl ui_core::SelectionPainter for GlassSelection {
    fn paint(&self, geometry: &ui_core::SelectionGeometry<'_>) -> ui_core::SelectionPaint {
        ui_core::SelectionPaint {
            rects: geometry
                .fragments
                .iter()
                .map(|f| ui_core::RectPrim {
                    rect: f.rect,
                    radius: ui_core::Corners::all(SELECTION_RADIUS),
                    fill: geometry.fill,
                    gradient: None,
                    border: None,
                    border_gradient: None,
                    notch: None,
                })
                .collect(),
            ink: None,
        }
    }
}

/// The single instance a pack hands to every text node it styles.
pub static GLASS_SELECTION: GlassSelection = GlassSelection;
