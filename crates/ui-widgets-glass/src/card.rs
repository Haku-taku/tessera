//! The glass container: a card or plain surface, in glass or as a flat fill.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Clip, Grow, Overflow, SizeClass};
use ui_core::{Decl, Id, NodeKind};
use ui_style::Style;

use crate::material::{Control, glass_recipe};
use crate::shared::glass_backdrop;

/// `GlassContainer.card`: 16 dp of inset and a 12 dp continuous corner.
pub const CARD_PAD: f32 = 16.0;
pub const CARD_RADIUS: f32 = 12.0;
/// A plain container's own default corner (`GlassContainer`'s 16 dp).
pub const CONTAINER_RADIUS: f32 = 16.0;
/// Native inset-grouped cards use ~26 pt continuous corners.
pub const SOLID_CARD_RADIUS: f32 = 26.0;

/// How a container is painted: the glass body, or iOS's flat card fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlassSurface {
    Glass,
    /// White in light, `#1C1C1E` in dark, with no blur, rim or shadow.
    Solid,
}

/// Everything a container is built with; `radius` of `None` takes the surface's default.
#[derive(Clone, Copy, Debug)]
pub struct GlassCardParams {
    pub id: Id,
    pub padding: Insets,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub surface: GlassSurface,
    radius: Option<f32>,
    /// A fill over the glass, when the reference names a translucent card colour.
    fill: Option<Color>,
}

impl GlassCardParams {
    /// `GlassContainer.card`: 16 dp inset, 12 dp corner.
    pub fn card(id: Id) -> Self {
        Self {
            id,
            padding: Insets::all(CARD_PAD),
            width: None,
            height: None,
            surface: GlassSurface::Glass,
            radius: None,
            fill: None,
        }
    }

    /// `GlassContainer`: no inset, 16 dp corner.
    pub fn container(id: Id) -> Self {
        Self {
            padding: Insets::ZERO,
            radius: Some(CONTAINER_RADIUS),
            ..Self::card(id)
        }
    }

    pub fn solid(mut self) -> Self {
        self.surface = GlassSurface::Solid;
        self
    }

    pub fn padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    /// Fill the surface with this colour instead of its default body.
    pub fn fill(mut self, fill: Color) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }
}

/// A container under construction: children are declared first, the box is resolved last.
pub struct GlassCard {
    params: GlassCardParams,
    fill: Color,
    backdrop: Option<ui_core::BackdropBlur>,
    children: Vec<Decl>,
}

impl GlassCard {
    pub fn new(params: GlassCardParams, pack: &Style) -> Self {
        let dark = pack.appearance() == Some(ui_design::Appearance::Dark);
        let (fill, backdrop) = match params.surface {
            GlassSurface::Solid => (
                if dark {
                    Color::from_rgba8(0x1C, 0x1C, 0x1E, 255)
                } else {
                    Color::WHITE
                },
                None,
            ),
            GlassSurface::Glass => (
                Color::TRANSPARENT,
                glass_backdrop(&glass_recipe(pack, Control::Container), 1.0),
            ),
        };
        let fill = params.fill.unwrap_or(fill);
        Self {
            params,
            fill,
            backdrop,
            children: Vec::new(),
        }
    }

    pub fn child(mut self, child: Decl) -> Self {
        self.children.push(child);
        self
    }

    pub fn resolve(self) -> Decl {
        let radius = self.params.radius.unwrap_or(match self.params.surface {
            GlassSurface::Glass => CARD_RADIUS,
            GlassSurface::Solid => SOLID_CARD_RADIUS,
        });
        let corners = Corners::all(radius);
        // A card is a column of content: it fills the width it is given and hugs its height.
        let mut node = Decl::container(self.params.id, NodeKind::Column)
            .size(SizeClass {
                width: self
                    .params
                    .width
                    .map_or(AxisSize::fill(0.0, 0.0), AxisSize::fixed),
                height: self
                    .params
                    .height
                    .map_or(AxisSize::hug(0.0, 0.0), AxisSize::fixed),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .padding(self.params.padding)
            .corners(corners)
            // A rounded surface cuts its content to its own shape: a pressed row's square wash
            // must not square off the card's corner.
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            });
        // The node's own box is the face: its width is only known after layout, so the
        // hairline `glass_face` draws from an explicit rect cannot be used here.
        node = node.background(self.fill, corners);
        if let Some(backdrop) = self.backdrop
            && let Some(p) = node.data.params_mut()
        {
            p.backdrop_blur = Some(backdrop);
        }
        for child in self.children {
            node = node.child(child);
        }
        node
    }
}
