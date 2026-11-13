//! MD3 toolbar: a 64dp bar of 48dp icon-button slots, docked (square, flush) or floating
//! (stadium, raised) with the standard and vibrant member themes, and a floating bar's
//! trailing FAB.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{AnimSpec, CursorShape, Decl, Easing, Id, InkRadius, NodeKind, PressInk, Sense};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::Background;
use crate::nav_bar::boxed;
use crate::paint::{between, faded, role, spec_ink};
use crate::spec::{
    ToolbarFabSize, ToolbarFabTheme, ToolbarStandardFlags, ToolbarVibrantFlags,
    toolbar_docked_spec, toolbar_fab_spec, toolbar_floating_spec, toolbar_standard_spec,
    toolbar_vibrant_spec,
};

/// The 48dp touch target; the same constant the app bar uses for its slots.
pub const TOOLBAR_SLOT: f32 = 48.0;
/// `md.comp.icon-button.container.height`: the 40dp disc inside the touch target.
pub const TOOLBAR_DISC: f32 = 40.0;
/// `md.comp.icon-button.icon.size`.
pub const TOOLBAR_ICON: f32 = 24.0;
/// `md.sys.state.focus-indicator.thickness`, which the icon button points at.
const FOCUS_THICKNESS: f32 = 3.0;

use crate::motion::REACTION;

/// A filled FAB's own state layers. The toolbar's FAB subtree names geometry and colours
/// only, so these are `md.comp.fab`'s opacities: 8% hover, 10% focus and press.
const FAB_HOVER: f32 = 0.08;
const FAB_FOCUS: f32 = 0.1;
const FAB_PRESS: f32 = 0.1;
/// `corner.large` and `corner.large-increased`; the export names both shapes
/// `rounded_corners` without a number.
const FAB_CORNER: f32 = 16.0;
const FAB_CORNER_MEDIUM: f32 = 20.0;
/// The disc's selection dissolve; the export has no token, 200ms keeps it crisp.
const SELECT_DISC: AnimSpec = AnimSpec::new(0.2, Easing::Linear);

/// Where the toolbar sits: flush against its parent, or floating above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToolbarVariant {
    /// Full width, square, no shadow.
    #[default]
    Docked,
    /// Hugs its members, stadium-shaped, raised by the export's elevation 6.
    Floating,
}

/// Which member theme the toolbar paints: the export's two tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToolbarTheme {
    /// `surface-container` bar with `secondary-container` selected discs.
    #[default]
    Standard,
    /// `primary-container` bar with `surface-container` selected discs.
    Vibrant,
}

/// One member: the id the app knows it by, its 24dp mark, and whether it reads as toggled on.
pub struct ToolbarAction<'a> {
    pub id: Id,
    /// The mark at the member's icon id, in the ink and `FILL` axis the state resolves to.
    pub icon: IconSource,
    /// The export's `selected` state: the disc takes the theme's selected container colour.
    pub selected: bool,
    /// Kept for callers that key the member by something other than its id.
    pub label: Option<&'a str>,
}

impl<'a> ToolbarAction<'a> {
    /// An untoggled member.
    pub fn new(id: Id, icon: IconSource) -> Self {
        Self {
            id,
            icon,
            selected: false,
            label: None,
        }
    }

    /// The same member, reading as toggled on.
    pub fn selected(mut self) -> Self {
        self.selected = true;
        self
    }

    /// A label for the member; the export carries only its ink, so it is not painted yet.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }
}

/// The FAB a floating toolbar carries at its trailing end. The bar sizes and themes it;
/// its press is reported as `Change::Selected(Some(id))`.
pub struct ToolbarFab {
    pub id: Id,
    pub icon: IconSource,
    /// The 80dp medium FAB; the default is the 56dp one.
    pub medium: bool,
}

impl ToolbarFab {
    pub fn new(id: Id, icon: IconSource) -> Self {
        Self {
            id,
            icon,
            medium: false,
        }
    }

    /// The 80dp medium size, with the larger mark.
    pub fn medium(mut self) -> Self {
        self.medium = true;
        self
    }
}

/// Everything a toolbar needs besides the design system's own tokens.
pub struct ToolbarParams<'a> {
    pub id: Id,
    pub variant: ToolbarVariant,
    pub theme: ToolbarTheme,
    pub actions: &'a [ToolbarAction<'a>],
    /// Only a floating bar carries a FAB; a docked bar ignores it.
    pub fab: Option<ToolbarFab>,
    pub disabled: bool,
}

/// The token names one member resolves to in one state, whichever theme it belongs to.
struct Inks {
    ink: &'static str,
    disabled_ink: &'static str,
    disabled_opacity: f32,
    disc: &'static str,
    layer: &'static str,
    hover: f32,
    focus: f32,
    press: f32,
}

/// Resolve the member's token names for one state combination.
fn inks(theme: ToolbarTheme, selected: bool, hovered: bool, focused: bool, pressed: bool) -> Inks {
    match theme {
        ToolbarTheme::Standard => {
            let s = toolbar_standard_spec(ToolbarStandardFlags {
                selected,
                hovered,
                focused,
                pressed,
                ..ToolbarStandardFlags::NONE
            });
            let d = toolbar_standard_spec(ToolbarStandardFlags {
                selected,
                disabled: true,
                ..ToolbarStandardFlags::NONE
            });
            Inks {
                ink: s.icon.color,
                disabled_ink: d.icon.color,
                disabled_opacity: d.icon.opacity,
                disc: s.button_container.color,
                layer: s.state_layer.color,
                hover: toolbar_standard_spec(ToolbarStandardFlags {
                    selected,
                    hovered: true,
                    ..ToolbarStandardFlags::NONE
                })
                .state_layer
                .opacity,
                focus: toolbar_standard_spec(ToolbarStandardFlags {
                    selected,
                    focused: true,
                    ..ToolbarStandardFlags::NONE
                })
                .state_layer
                .opacity,
                press: toolbar_standard_spec(ToolbarStandardFlags {
                    selected,
                    pressed: true,
                    ..ToolbarStandardFlags::NONE
                })
                .state_layer
                .opacity,
            }
        }
        ToolbarTheme::Vibrant => {
            let s = toolbar_vibrant_spec(ToolbarVibrantFlags {
                selected,
                hovered,
                focused,
                pressed,
                ..ToolbarVibrantFlags::NONE
            });
            let d = toolbar_vibrant_spec(ToolbarVibrantFlags {
                selected,
                disabled: true,
                ..ToolbarVibrantFlags::NONE
            });
            Inks {
                ink: s.icon.color,
                disabled_ink: d.icon.color,
                disabled_opacity: d.icon.opacity,
                disc: s.button_container.color,
                layer: s.state_layer.color,
                hover: toolbar_vibrant_spec(ToolbarVibrantFlags {
                    selected,
                    hovered: true,
                    ..ToolbarVibrantFlags::NONE
                })
                .state_layer
                .opacity,
                focus: toolbar_vibrant_spec(ToolbarVibrantFlags {
                    selected,
                    focused: true,
                    ..ToolbarVibrantFlags::NONE
                })
                .state_layer
                .opacity,
                press: toolbar_vibrant_spec(ToolbarVibrantFlags {
                    selected,
                    pressed: true,
                    ..ToolbarVibrantFlags::NONE
                })
                .state_layer
                .opacity,
            }
        }
    }
}

/// One member's resolved paint data.
struct Member {
    id: Id,
    icon: IconSource,
    ink: Color,
    /// The 40dp disc's fill, which is the theme's `button.container.color`.
    disc: Color,
    layer: Option<Color>,
    press: Option<PressInk>,
    ring: f32,
}

/// The trailing FAB's resolved paint data.
struct Fab {
    id: Id,
    icon: IconSource,
    ink: Color,
    container: Color,
    size: f32,
    corners: Corners,
    icon_size: f32,
    shadow: Option<ShadowSpec>,
    layer: Option<Color>,
    press: Option<PressInk>,
    ring: f32,
    /// The extra gap this FAB keeps before itself, beyond the row's own gap.
    lead: f32,
}

/// A toolbar; the app owns which members read as selected.
pub struct Md3Toolbar {
    id: Id,
    members: Vec<Member>,
    fab: Option<Fab>,
    /// The FAB whose press arrived this frame.
    fired: Option<Id>,
    height: f32,
    inset: f32,
    gap: f32,
    container: Color,
    floating: bool,
    shadow: Option<ShadowSpec>,
    ring_colour: Color,
    disabled: bool,
}

impl Md3Toolbar {
    /// Same control with explicit parameters, so the semantic path and a direct caller
    /// produce identical `Decl`s.
    pub fn new_with(
        params: Md3Options,
        p: ToolbarParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let docked = toolbar_docked_spec();
        let floating = toolbar_floating_spec();
        let is_floating = p.variant == ToolbarVariant::Floating;
        let container = match p.theme {
            ToolbarTheme::Standard => {
                toolbar_standard_spec(ToolbarStandardFlags::NONE)
                    .container
                    .color
            }
            ToolbarTheme::Vibrant => {
                toolbar_vibrant_spec(ToolbarVibrantFlags::NONE)
                    .container
                    .color
            }
        };
        let disabled = params.disabled || cx.state(p.id).disabled;
        // The 40dp disc every member paints into: layer, selection and ripple all use it.
        let inset = (TOOLBAR_SLOT - TOOLBAR_DISC) * 0.5;
        let background = Background::new(
            Rect::new(inset, inset, TOOLBAR_DISC, TOOLBAR_DISC),
            Corners::all(TOOLBAR_DISC * 0.5),
        );
        let mut members = Vec::with_capacity(p.actions.len());
        for action in p.actions {
            let member_id = action.id;
            let mut node = cx.at(member_id);
            let state = node.state();
            let t = inks(
                p.theme,
                action.selected,
                state.hovered,
                state.focus_visible,
                state.pressed,
            );
            // The disc and its ink dissolve between the two selection states instead of
            // snapping when the app flips `selected`.
            let on = inks(
                p.theme,
                true,
                state.hovered,
                state.focus_visible,
                state.pressed,
            );
            let off = inks(
                p.theme,
                false,
                state.hovered,
                state.focus_visible,
                state.pressed,
            );
            let sel = node.animate(
                member_id.with(param::PROGRESS),
                f32::from(action.selected),
                SELECT_DISC,
            );
            let ink = if disabled {
                between(
                    spec_ink(pack, off.disabled_ink, off.disabled_opacity),
                    spec_ink(pack, on.disabled_ink, on.disabled_opacity),
                    sel,
                )
            } else {
                between(
                    spec_ink(pack, off.ink, 1.0),
                    spec_ink(pack, on.ink, 1.0),
                    sel,
                )
            };
            let layer_ink = spec_ink(pack, t.layer, 1.0);
            let hover = node.animate(
                member_id.with(param::HOVER),
                f32::from(state.hovered),
                REACTION,
            );
            let focus = node.animate(
                member_id.with(param::FOCUS),
                f32::from(state.focus_visible),
                REACTION,
            );
            let alpha = (hover * t.hover).max(focus * t.focus);
            let layer = (alpha > 0.0 && !disabled).then_some(Color {
                a: layer_ink.a * alpha,
                ..layer_ink
            });
            let press = (!disabled).then(|| {
                background.press(
                    Color {
                        a: layer_ink.a * t.press,
                        ..layer_ink
                    },
                    InkRadius::Cover,
                )
            });
            let ring = if disabled {
                0.0
            } else {
                node.animate(
                    member_id.with(param::THICKNESS),
                    f32::from(state.focus_visible),
                    REACTION,
                )
            };
            members.push(Member {
                id: member_id,
                icon: action.icon,
                ink,
                disc: between(
                    spec_ink(pack, off.disc, 1.0),
                    spec_ink(pack, on.disc, 1.0),
                    sel,
                ),
                layer,
                press,
                ring,
            });
        }
        let gap = if is_floating {
            floating.container.between_space
        } else {
            docked.container.min_spacing
        };
        // Only a floating bar carries a FAB; its own gap is measured from the last member.
        let mut fired = None;
        let fab = p.fab.filter(|_| is_floating).map(|fab| {
            let mut node = cx.at(fab.id);
            let state = node.state();
            if !disabled && state.activation() {
                fired = Some(fab.id);
            }
            let spec = toolbar_fab_spec(
                match p.theme {
                    ToolbarTheme::Standard => ToolbarFabTheme::Standard,
                    ToolbarTheme::Vibrant => ToolbarFabTheme::Vibrant,
                },
                if fab.medium {
                    ToolbarFabSize::Medium
                } else {
                    ToolbarFabSize::Base
                },
            );
            let size = spec.container.width;
            let icon_size = spec.icon.size;
            let elevation = spec.container.elevation;
            let corners = Corners::all(if fab.medium {
                FAB_CORNER_MEDIUM
            } else {
                FAB_CORNER
            });
            let hover = node.animate(
                fab.id.with(param::HOVER),
                f32::from(state.hovered),
                REACTION,
            );
            let focus = node.animate(
                fab.id.with(param::FOCUS),
                f32::from(state.focus_visible),
                REACTION,
            );
            let strength = (hover * FAB_HOVER).max(focus * FAB_FOCUS);
            let layer_ink = role(pack, spec.icon.color);
            let layer = (strength > 0.0 && !disabled).then_some(Color {
                a: layer_ink.a * strength,
                ..layer_ink
            });
            let press = (!disabled).then(|| {
                Background::new(Rect::new(0.0, 0.0, size, size), corners).press(
                    Color {
                        a: layer_ink.a * FAB_PRESS,
                        ..layer_ink
                    },
                    InkRadius::Cover,
                )
            });
            let ring = if disabled {
                0.0
            } else {
                node.animate(
                    fab.id.with(param::THICKNESS),
                    f32::from(state.focus_visible),
                    REACTION,
                )
            };
            let fade = if disabled {
                crate::DISABLED_CONTENT_OPACITY
            } else {
                1.0
            };
            Fab {
                id: fab.id,
                icon: fab.icon,
                ink: faded(spec_ink(pack, spec.icon.color, 1.0), fade),
                container: faded(spec_ink(pack, spec.container.color, 1.0), fade),
                size,
                corners,
                icon_size,
                shadow: (elevation > 0.0)
                    .then(|| ShadowSpec::black(elevation * 2.0, 0.18, elevation * 0.5)),
                layer,
                press,
                ring,
                lead: (spec.root.between_space - gap).max(0.0),
            }
        });
        let elevation = floating.container.elevation;
        Self {
            id: p.id,
            members,
            fab,
            fired,
            height: if is_floating {
                floating.horizontal_container.height
            } else {
                docked.container.height
            },
            inset: if is_floating {
                floating.container.leading_space
            } else {
                docked.container.leading_space
            },
            gap,
            container: role(pack, container),
            floating: is_floating,
            shadow: (is_floating && elevation > 0.0)
                .then(|| ShadowSpec::black(elevation * 2.0, 0.18, elevation * 0.5)),
            ring_colour: role(pack, "secondary"),
            disabled,
        }
    }

    /// The toolbar's own height, so a caller can reserve room for it.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// One member: its disc, state layer, focus ring and centred mark.
    fn member_decl(&self, m: &Member) -> Decl {
        let d = (TOOLBAR_SLOT - TOOLBAR_DISC) * 0.5;
        let background = Background::new(
            Rect::new(d, d, TOOLBAR_DISC, TOOLBAR_DISC),
            Corners::all(TOOLBAR_DISC * 0.5),
        );
        let mut shapes = vec![background.fill(m.disc)];
        if let Some(layer) = m.layer {
            shapes.push(background.fill(layer));
        }
        // The icon button's focus indicator sits outside the disc; the slot is 48dp, so the
        // ring is drawn on the slot's own edge rather than inset into the disc.
        if m.ring > 0.0 {
            let ring = Color {
                a: self.ring_colour.a * m.ring,
                ..self.ring_colour
            };
            shapes.push(boxed(Color::TRANSPARENT, Some((ring, FOCUS_THICKNESS))));
        }
        Decl::container(m.id, NodeKind::Column)
            .size(SizeClass::fixed(TOOLBAR_SLOT, TOOLBAR_SLOT))
            .padding(Insets {
                left: d,
                right: d,
                // The inset centres the mark's own em box in the slot.
                top: (TOOLBAR_SLOT - ui_widgets::icon_extent(TOOLBAR_ICON)) * 0.5,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink(m.press)
            .shapes(shapes)
            .child(
                m.icon
                    .resolve_with_size(m.id.with("icon"), m.ink, TOOLBAR_ICON),
            )
    }

    /// The trailing FAB: its container, state layer, focus ring and centred mark.
    fn fab_decl(&self, f: &Fab) -> Decl {
        let mut shapes = vec![Shape::Box {
            corners: f.corners,
            fill: f.container,
            gradient: None,
            border: None,
            shadow: f.shadow,
            image: None,
        }];
        if let Some(layer) = f.layer {
            shapes.push(Shape::Box {
                corners: f.corners,
                fill: layer,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            });
        }
        if f.ring > 0.0 {
            let ring = Color {
                a: self.ring_colour.a * f.ring,
                ..self.ring_colour
            };
            shapes.push(Shape::Box {
                corners: f.corners,
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((ring, FOCUS_THICKNESS)),
                shadow: None,
                image: None,
            });
        }
        let pad = (f.size - f.icon_size) * 0.5;
        Decl::container(f.id, NodeKind::Column)
            .size(SizeClass::fixed(f.size, f.size))
            .padding(Insets::all(pad))
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink(f.press)
            .shapes(shapes)
            .child(
                f.icon
                    .resolve_with_size(f.id.with("icon"), f.ink, f.icon_size),
            )
            .margin(Insets {
                left: f.lead,
                ..Insets::ZERO
            })
    }
}

impl Widget for Md3Toolbar {
    fn handles_disabled(&self) -> bool {
        // The disabled ink is the export's own 38% colour, painted here.
        true
    }

    fn change(&self, state: &WidgetState, bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        if self.disabled {
            return None;
        }
        // The FAB carries its own id and sense; the bar only reports what it fired.
        if let Some(id) = self.fired {
            return Some(ui_core::Change::Selected(Some(id)));
        }
        // The press point names the member; the keyboard has none (engine-audit F12).
        if !state.clicked {
            return None;
        }
        // Members are laid out left to right from the leading inset; the press point names
        // one by its slot.
        let local = state.local?;
        let x = local.x - self.inset;
        if x < 0.0 || local.y < 0.0 || local.y > bounds.size.h {
            return None;
        }
        let step = TOOLBAR_SLOT + self.gap;
        let k = (x / step).floor() as usize;
        if k >= self.members.len() || x - k as f32 * step > TOOLBAR_SLOT {
            return None;
        }
        Some(ui_core::Change::Selected(Some(self.members[k].id)))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let mut root = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass {
                width: if self.floating {
                    AxisSize::hug(0.0, 0.0)
                } else {
                    AxisSize::fill(0.0, 0.0)
                },
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: self.inset,
                right: self.inset,
                top: 0.0,
                bottom: 0.0,
            })
            .gap(self.gap)
            .cross_align(0.0)
            .cross_pos(0.5)
            .shapes(vec![Shape::Box {
                // The docked bar is `corner.none`; the floating one is `corner.full`.
                corners: if self.floating {
                    Corners::all(self.height * 0.5)
                } else {
                    Corners::ZERO
                },
                fill: self.container,
                gradient: None,
                border: None,
                shadow: self.shadow,
                image: None,
            }]);
        for m in &self.members {
            root = root.child(self.member_decl(m));
        }
        if let Some(fab) = &self.fab {
            root = root.child(self.fab_decl(fab));
        }
        root
    }
}
