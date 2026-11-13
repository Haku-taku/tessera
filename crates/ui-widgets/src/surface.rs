//! Surfaces: the card and the frosted panel. The frost's blur sigma and its fallback
//! colour both come from the pack, and the fallback is mandatory — a pack may not ask
//! for frost without saying what to draw when the backend cannot deliver it.

use ui_core::geom::Corners;
use ui_core::layout::SizeClass;
use ui_core::{Decl, Id, NodeKind};
use ui_style::{Material, RoleName, Style};

pub mod roles {
    pub const CARD_FILL: &str = "cardFill";
    pub const CARD_BORDER: &str = "cardBorder";
    pub const GLASS_FALLBACK: &str = "glassFallback";
    pub const GLASS_BORDER: &str = "glassBorder";
}

/// A plain surface with a background and a border, no interaction of its own.
pub struct Card<'a> {
    pub id: Id,
    pub title: Option<String>,
    /// Ink for the title; `None` falls back to the pack's `onSurface`. The card owns the
    /// surface, so it is the one that knows which role reads well on its fill.
    pub title_ink: Option<ui_core::geom::Color>,
    pub pack: &'a Style,
}

impl<'a> Card<'a> {
    pub fn resolve(&self, children: Vec<Decl>) -> Decl {
        let m = &self.pack.metrics;
        let fill = self
            .pack
            .roles
            .get(RoleName(roles::CARD_FILL))
            .unwrap_or_else(|| {
                self.pack.colour_or(
                    ui_design::CommonColor::SurfaceRaised,
                    ui_core::geom::Color::TRANSPARENT,
                )
            });
        let border = self
            .pack
            .roles
            .get(RoleName(roles::CARD_BORDER))
            .unwrap_or(ui_core::geom::Color::TRANSPARENT);
        let mut card = Decl::container(self.id, NodeKind::Column)
            // A card is a painted surface: it shows hover but takes no clicks.
            .sense(ui_core::Sense::HOVER)
            .background(
                fill,
                Corners::all(m.surface.radius).with_shape(m.surface.corner),
            )
            .border(border, m.surface.border)
            .padding(m.surface.padding);
        if let Some(body) = self.title.clone() {
            let t = self.pack.type_table.get(ui_style::TextRole::Title);
            let ink = self
                .title_ink
                .or_else(|| self.pack.roles.get(RoleName("onSurface")))
                .unwrap_or_else(|| {
                    self.pack.colour_or(
                        ui_design::CommonColor::OnSurfaceRaised,
                        ui_core::geom::Color::WHITE,
                    )
                });
            card = card.child(crate::text::leaf(
                self.id.with("title"),
                body.into(),
                t.size,
                ink,
                t.weight,
                f32::INFINITY,
                t.line_gap,
                t.indent,
            ));
        }
        for child in children {
            card = card.child(child);
        }
        card
    }
}

/// The frosted floating panel: blurs what lies behind it, or draws its fallback colour.
pub struct Panel<'a> {
    pub id: Id,
    pub size: (f32, f32),
    pub offset: (f32, f32),
    /// A drag is running from this panel; only the app knows.
    pub dragging: bool,
    pub title: String,
    pub hint: String,
    /// Title ink; `None` falls back to the pack's `onSurface`.
    pub title_ink: Option<ui_core::geom::Color>,
    /// Hint ink; `None` falls back to the pack's `onSurfaceVariant`.
    pub hint_ink: Option<ui_core::geom::Color>,
    pub pack: &'a Style,
}

impl<'a> Panel<'a> {
    pub fn resolve(&self) -> Decl {
        let m = &self.pack.metrics;
        // The look is the pack's; the decision frost-or-fallback is the capability's.
        let material = self.pack.material.unwrap_or(Material::Solid(
            self.pack
                .roles
                .get(RoleName(roles::GLASS_FALLBACK))
                .unwrap_or(ui_core::geom::Color::TRANSPARENT),
        ));
        // Frost, acrylic and glass all sample the backdrop; only `Solid` never does.
        let frost = match material.sigma() {
            Some(sigma) if self.pack.frost_allowed(self.pack.device) => {
                // Acrylic and glass shade the backdrop in their own pass, so their rect
                // fill stays clear; only frost tints with a plain fill.
                let tint = match material {
                    Material::Frost { tint, .. } => tint,
                    _ => ui_core::geom::Color::TRANSPARENT,
                };
                Some((tint, sigma, material.backdrop()))
            }
            _ => None,
        };
        let frost_material = frost.and_then(|(_, _, m)| m);
        let (fill, sigma) = match frost {
            Some((tint, s, _)) => (tint, Some(s)),
            // Reduced transparency: opaque stand-in, no filter — Fluent's `DisableAcrylic`
            // does exactly this, without noise either.
            None => (material.fallback_color(), None),
        };
        let title = self.pack.type_table.get(ui_style::TextRole::Subtitle);
        let hint = self.pack.type_table.get(ui_style::TextRole::Hint);

        let mut panel = Decl::container(self.id, NodeKind::Overlay)
            // A panel floats above the page and takes both hover and drag.
            .sense(ui_core::Sense::HOVER | ui_core::Sense::DRAG)
            // A translucent surface floating over content reads as a sheet only if it casts.
            .cursor(if self.dragging {
                ui_core::CursorShape::Grabbing
            } else {
                ui_core::CursorShape::Grab
            })
            .offset(self.offset.0, self.offset.1)
            .size(SizeClass::fixed(self.size.0, self.size.1))
            .background(
                fill,
                Corners::all(m.overlay.radius).with_shape(m.overlay.corner),
            );
        if let Some(s) = sigma {
            panel = panel.backdrop_blur(s);
            if let Some(m) = frost_material {
                panel = panel.backdrop_material(m);
            }
        }
        // Mica samples a wallpaper texture instead of the frame, so it is an image plus a
        // tinted layer rather than a filter.
        let mut mica_layer = None;
        if let Material::Mica { source, tint, .. } = material {
            panel = panel.background_image(source, ui_core::tree::ImageFit::Cover);
            mica_layer = Some(tint);
        }
        panel = panel
            .border(
                self.pack
                    .roles
                    .get(RoleName(roles::GLASS_BORDER))
                    .unwrap_or(ui_core::geom::Color::TRANSPARENT),
                m.overlay.border,
            )
            .padding(m.overlay.padding);
        panel = panel.elevation(m.overlay.elevation);
        if let Some(tint) = mica_layer {
            // A full-size tinted layer above the wallpaper and below the content.
            panel = panel.child(
                Decl::container(self.id.with("mica"), NodeKind::Box)
                    .size(ui_core::SizeClass {
                        width: ui_core::layout::AxisSize::fill(0.0, self.size.0),
                        height: ui_core::layout::AxisSize::fill(0.0, self.size.1),
                        grow_x: ui_core::layout::Grow::No,
                        grow_y: ui_core::layout::Grow::No,
                    })
                    .background(
                        tint,
                        Corners::all(m.overlay.radius).with_shape(m.overlay.corner),
                    ),
            );
        }
        panel = panel
            .child(crate::text::leaf(
                self.id.with("title"),
                self.title.clone(),
                title.size,
                self.title_ink
                    .or_else(|| self.pack.roles.get(RoleName("onSurface")))
                    .unwrap_or(ui_core::geom::Color::WHITE),
                title.weight,
                self.size.0,
                title.line_gap,
                title.indent,
            ))
            .child(crate::text::leaf(
                self.id.with("hint"),
                self.hint.clone(),
                hint.size,
                self.hint_ink
                    .or_else(|| self.pack.roles.get(RoleName("onSurfaceVariant")))
                    .unwrap_or(ui_core::geom::Color::WHITE),
                hint.weight,
                self.size.0,
                hint.line_gap,
                hint.indent,
            ));
        panel
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::geom::Color;
    use ui_style::{Capability, MetricsSet, MotionTable, RoleDict};

    fn pack(material: Option<Material>, device: Capability) -> Style {
        let mut roles = RoleDict::new();
        roles.set(
            RoleName(roles::GLASS_FALLBACK),
            Color::from_rgba8(40, 44, 54, 255),
        );
        roles.set(
            RoleName(roles::CARD_BORDER),
            Color::from_rgba8(200, 200, 200, 255),
        );
        Style {
            common: None,
            roles,
            motion: MotionTable::legacy(),
            metrics: MetricsSet::NONE,
            controls: ui_style::ControlMetricsTable::EMPTY,
            type_table: ui_style::stub_type_table(),
            material,
            device,
            palette: None,
            selection: None,
        }
    }

    #[test]
    fn a_frost_panel_asks_for_a_blurred_backdrop_when_the_backend_allows_it() {
        let p = pack(
            Some(Material::Frost {
                sigma: 14.0,
                tint: Color::from_rgba8(255, 255, 255, 90),
                fallback: Color::from_rgba8(40, 44, 54, 255),
            }),
            Capability::GPU_BASELINE,
        );
        let panel = Panel {
            id: Id::str("glass"),
            size: (260.0, 120.0),
            offset: (10.0, 10.0),
            dragging: false,
            title: "frosted".into(),
            hint: "drag me".into(),
            title_ink: None,
            hint_ink: None,
            pack: &p,
        };
        let decl = panel.resolve();
        let dumped = format!("{:?}", decl.data);
        assert!(
            dumped.contains("backdrop_blur: Some"),
            "expected a blur, got {dumped}"
        );
    }

    #[test]
    fn the_same_panel_draws_its_fallback_without_a_blur_when_frost_is_forbidden() {
        // Reduced transparency must drop the filter AND land on the opaque stand-in.
        let frost = Material::Frost {
            sigma: 14.0,
            tint: Color::from_rgba8(255, 255, 255, 90),
            fallback: Color::from_rgba8(40, 44, 54, 255),
        };
        let strict = pack(
            Some(frost),
            Capability {
                reduced_transparency: true,
                ..Capability::GPU_BASELINE
            },
        );
        let panel = Panel {
            id: Id::str("glass"),
            size: (260.0, 120.0),
            offset: (0.0, 0.0),
            dragging: false,
            title: "t".into(),
            hint: "h".into(),
            title_ink: None,
            hint_ink: None,
            pack: &strict,
        };
        let decl = panel.resolve();
        let dumped = format!("{:?}", decl.data);
        assert!(
            !dumped.contains("backdrop_blur: Some"),
            "fallback must not blur"
        );
        let fill = decl.fill().expect("a background");
        assert_eq!(fill, Color::from_rgba8(40, 44, 54, 255));
    }
}
