//! MD3 FAB menu: a column of pill items above the circular close button that replaced the FAB.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{
    AnimSpec, CursorShape, Decl, Easing, Id, InkRadius, NodeData, NodeKind, PressInk, Sense,
};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::press_ink;
use crate::paint::{faded, role, spec_ink};
use crate::spec::{FabMenuColor, FabMenuContainer, FabMenuFlags, FabMenuSpec, fab_menu_spec};
use crate::typescale::{self, TypeStyle};

/// MD3's `emphasizedDecelerate` in spirit: the items arrive fast and settle.
const OPEN: AnimSpec = AnimSpec::new(0.15, Easing::EaseOut);
use crate::motion::REACTION;

/// The FAB's colour role, which decides both the close button's fill and the items' container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FabMenuColour {
    Primary,
    Secondary,
    Tertiary,
}

impl FabMenuColour {
    fn close(self) -> FabMenuColor {
        match self {
            Self::Primary => FabMenuColor::Primary,
            Self::Secondary => FabMenuColor::Secondary,
            Self::Tertiary => FabMenuColor::Tertiary,
        }
    }

    /// The items take the `-container` form of the same role.
    fn items(self) -> FabMenuContainer {
        match self {
            Self::Primary => FabMenuContainer::PrimaryContainer,
            Self::Secondary => FabMenuContainer::SecondaryContainer,
            Self::Tertiary => FabMenuContainer::TertiaryContainer,
        }
    }

    /// Resolve this tone for one interaction state.
    fn spec(self, hovered: bool, focused: bool, pressed: bool) -> FabMenuSpec {
        fab_menu_spec(
            FabMenuFlags {
                hovered,
                focused,
                pressed,
            },
            self.close(),
            self.items(),
        )
    }
}

/// One menu entry: the id the app knows it by, its label, and its mark.
pub struct Md3FabMenuItem<'a> {
    pub id: Id,
    pub label: &'a str,
    /// The mark at `id`, in the item's own ink and `FILL` axis.
    pub icon: IconSource,
}

/// One resolved menu entry.
struct Item {
    id: Id,
    label: String,
    icon: (IconSource, Color),
    container: Color,
    layer: Option<Color>,
    /// The press ink the engine splats at the pointer.
    press: Option<PressInk>,
    shadow: Option<ShadowSpec>,
    label_ink: Color,
}

/// The metrics every item shares, resolved once from the export.
struct Metrics {
    item_h: f32,
    item_gap: f32,
    close_gap: f32,
    close_w: f32,
    close_h: f32,
    close_icon: f32,
    icon: f32,
    icon_label: f32,
    leading: f32,
    trailing: f32,
    label: TypeStyle,
}

impl Metrics {
    fn of(spec: &FabMenuSpec) -> Self {
        Self {
            item_h: spec.menu_item_container.height,
            item_gap: spec.menu_item.between_space,
            close_gap: spec.close_button.between_space,
            close_w: spec.close_button_container.width,
            close_h: spec.close_button_container.height,
            close_icon: spec.close_button_icon.size,
            icon: spec.menu_item_icon.size,
            icon_label: spec.menu_item.icon_label_space,
            leading: spec.menu_item.leading_space,
            trailing: spec.menu_item.trailing_space,
            label: typescale::named(spec.menu_item.label_text),
        }
    }
}

/// The close button's resolved paint data.
struct Close {
    container: Color,
    icon_ink: Color,
    layer: Option<Color>,
    /// The press ink the engine splats at the pointer.
    press: Option<PressInk>,
    shadow: Option<ShadowSpec>,
}

/// A FAB menu; the tone and `open` belong to the app.
pub struct Md3FabMenu {
    id: Id,
    items: Vec<Item>,
    close: Close,
    metrics: Metrics,
    close_icon: IconSource,
    /// How far the items have arrived, 0..=1.
    open: f32,
    open_target: bool,
    disabled: bool,
}

/// The two interaction strengths the export gives one shape, as animated alphas. A press is
/// a splat from the pointer, so it is not one of them.
fn layer_of(
    pack: &Style,
    hover: f32,
    focus: f32,
    hover_opacity: f32,
    focus_opacity: f32,
    ink: &'static str,
    disabled: bool,
) -> Option<Color> {
    let strength = (hover * hover_opacity).max(focus * focus_opacity);
    let ink = role(pack, ink);
    (strength > 0.0 && !disabled).then_some(Color {
        a: ink.a * strength,
        ..ink
    })
}

/// The press ink for a shape whose pressed layer is `ink` at `opacity`.
fn ink_of(pack: &Style, ink: &'static str, opacity: f32, disabled: bool) -> Option<PressInk> {
    if disabled {
        return None;
    }
    let ink = role(pack, ink);
    Some(press_ink(
        Color {
            a: ink.a * opacity,
            ..ink
        },
        InkRadius::Cover,
    ))
}

impl Md3FabMenu {
    /// Build one menu. The close button is the anchor: it is drawn even while closed.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with(
        params: Md3Options,
        id: Id,
        items: &[Md3FabMenuItem<'_>],
        tone: FabMenuColour,
        open: bool,
        close_icon: IconSource,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let disabled = params.disabled || cx.state(id).disabled;
        let rest = tone.spec(false, false, false);
        let metrics = Metrics::of(&rest);
        let fade = if disabled {
            crate::DISABLED_CONTENT_OPACITY
        } else {
            1.0
        };
        // The layer's ink is the same under hover/focus/press; only its strength differs.
        let hover_spec = tone.spec(true, false, false);
        let focus_spec = tone.spec(false, true, false);
        let press_spec = tone.spec(false, false, true);
        let mut resolved = Vec::with_capacity(items.len());
        for item in items {
            let mut node = cx.at(item.id);
            let state = node.state();
            let hover = node.animate(
                item.id.with(param::HOVER),
                f32::from(state.hovered),
                REACTION,
            );
            let focus = node.animate(
                item.id.with(param::FOCUS),
                f32::from(state.focus_visible),
                REACTION,
            );
            let on = tone.spec(state.hovered, state.focus_visible, state.pressed);
            let layer = layer_of(
                pack,
                hover,
                focus,
                hover_spec.list_item_state_layer.opacity,
                focus_spec.list_item_state_layer.opacity,
                hover_spec.list_item_state_layer.color,
                disabled,
            );
            let ink = ink_of(
                pack,
                press_spec.list_item_state_layer.color,
                press_spec.list_item_state_layer.opacity,
                disabled,
            );
            resolved.push(Item {
                id: item.id,
                label: item.label.to_string(),
                icon: (
                    item.icon,
                    faded(spec_ink(pack, on.list_item_icon.color, 1.0), fade),
                ),
                container: container_of(pack, on.list_item_container.color, disabled),
                layer,
                press: ink,
                shadow: shadow_of(on.list_item_container.elevation),
                label_ink: faded(spec_ink(pack, on.list_item_label_text.color, 1.0), fade),
            });
        }
        let close_id = id.with("close");
        let mut node = cx.at(close_id);
        let state = node.state();
        let hover = node.animate(
            close_id.with(param::HOVER),
            f32::from(state.hovered),
            REACTION,
        );
        let focus = node.animate(
            close_id.with(param::FOCUS),
            f32::from(state.focus_visible),
            REACTION,
        );
        let on = tone.spec(state.hovered, state.focus_visible, state.pressed);
        let layer = layer_of(
            pack,
            hover,
            focus,
            hover_spec.close_button_state_layer.opacity,
            focus_spec.close_button_state_layer.opacity,
            hover_spec.close_button_state_layer.color,
            disabled,
        );
        let ink = ink_of(
            pack,
            press_spec.close_button_state_layer.color,
            press_spec.close_button_state_layer.opacity,
            disabled,
        );
        let close = Close {
            container: container_of(pack, on.close_button_container.color, disabled),
            icon_ink: faded(spec_ink(pack, on.close_button_icon.color, 1.0), fade),
            layer,
            press: ink,
            shadow: shadow_of(on.close_button_container.elevation),
        };
        let mut root = cx.at(id);
        let open_t = root.animate(id.with(param::PROGRESS), f32::from(open), OPEN);
        Self {
            id,
            items: resolved,
            close,
            metrics,
            close_icon,
            open: open_t,
            open_target: open,
            disabled,
        }
    }

    /// The mark's size from the export, so the caller builds the glyph at the right size.
    pub fn icon_size(&self) -> f32 {
        self.metrics.icon
    }

    /// The anchor's own diameter: the FAB slot this menu replaced.
    pub fn anchor_size(&self) -> f32 {
        self.metrics.close_w
    }

    /// The control's own height: every item, the gaps, and the close button.
    pub fn height(&self) -> f32 {
        let n = self.items.len() as f32;
        if n == 0.0 {
            return self.metrics.close_h;
        }
        n * self.metrics.item_h
            + (n - 1.0) * self.metrics.item_gap
            + self.metrics.close_gap
            + self.metrics.close_h
    }

    /// Where each row starts, top to bottom: items first, the close button last.
    fn rows(&self) -> Vec<f32> {
        let m = &self.metrics;
        let mut ys = Vec::with_capacity(self.items.len() + 1);
        let mut y = 0.0;
        for _ in &self.items {
            ys.push(y);
            y += m.item_h + m.item_gap;
        }
        // The last item is followed by `close_gap`, not another `item_gap`.
        y = y - m.item_gap + m.close_gap;
        ys.push(y);
        ys
    }

    /// How far item `k` has slid toward the anchor: the rows keep their slots so the anchor
    /// never moves, and the items themselves travel into it as the menu collapses.
    fn slide(&self, k: usize) -> f32 {
        let m = &self.metrics;
        let below = (self.items.len() - 1 - k) as f32 * (m.item_h + m.item_gap);
        (1.0 - self.open) * (m.close_gap + below + m.item_h)
    }

    /// One item's node: its container, its state layer, then the mark and the label.
    fn item_decl(&self, item: &Item) -> Decl {
        let m = &self.metrics;
        let mut shapes = Vec::with_capacity(2);
        let mut b = self.boxed(item.container, m.item_h);
        if let Shape::Box { shadow, .. } = &mut b {
            *shadow = item.shadow;
        }
        shapes.push(b);
        if let Some(layer) = item.layer {
            shapes.push(self.boxed(layer, m.item_h));
        }
        Decl::container(item.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::fixed(m.item_h),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: m.leading,
                right: m.trailing,
                top: 0.0,
                bottom: 0.0,
            })
            .gap(m.icon_label)
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink(item.press)
            .shapes(shapes)
            .child(
                item.icon
                    .0
                    .resolve_with_size(item.id.with("icon"), item.icon.1, m.icon),
            )
            .child(
                Text::new(
                    item.id.with("label"),
                    item.label.clone(),
                    m.label.size,
                    item.label_ink,
                )
                .weight(m.label.weight)
                .resolve(),
            )
    }

    /// The close button: a circle with the mark centred in it.
    fn close_decl(&self) -> Decl {
        let m = &self.metrics;
        let mut shapes = Vec::with_capacity(2);
        let mut b = self.boxed(self.close.container, m.close_h);
        if let Shape::Box { shadow, .. } = &mut b {
            *shadow = self.close.shadow;
        }
        shapes.push(b);
        if let Some(layer) = self.close.layer {
            shapes.push(self.boxed(layer, m.close_h));
        }
        let side = ((m.close_w - m.close_icon) * 0.5).max(0.0);
        Decl::container(self.id.with("close"), NodeKind::Row)
            .size(SizeClass::fixed(m.close_w, m.close_h))
            // Equal side padding puts the mark on the circle's centre line; the vertical
            // centring is the cross axis's job.
            .padding(Insets {
                left: side,
                right: side,
                top: 0.0,
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
            .press_ink(self.close.press)
            .shapes(shapes)
            .child(self.close_icon.resolve_with_size(
                self.id.with("close").with("icon"),
                self.close.icon_ink,
                m.close_icon,
            ))
    }

    /// A zero-basis grower that holds the anchor on the bottom edge while the items are away.
    fn grower(&self) -> Decl {
        Decl::container(self.id.with("fill"), NodeKind::Column).size(SizeClass {
            width: AxisSize::hug(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::Yes,
        })
    }

    /// An invisible node that reserves one gap; `k` keeps its id unique among siblings.
    fn spacer(&self, k: usize, height: f32) -> Decl {
        Decl::container(self.id.with(("gap", k)), NodeKind::Column).size(SizeClass {
            width: AxisSize::fixed(0.0),
            height: AxisSize::fixed(height),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
    }

    /// A node-sized box rounded by half its height: MD3's `circular` shape.
    fn boxed(&self, fill: Color, height: f32) -> Shape {
        Shape::Box {
            corners: Corners::all(height * 0.5),
            fill,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        }
    }
}

impl Widget for Md3FabMenu {
    fn handles_disabled(&self) -> bool {
        // The 38% content fade is painted here, so the adapter must not fade again.
        true
    }

    fn change(&self, state: &WidgetState, bounds: Rect) -> Option<ui_core::Change> {
        if self.disabled || !state.activation() || bounds.size.w <= 0.0 {
            return None;
        }
        // The keyboard has no press point, so the anchor is the only target it can name.
        let Some(local) = state.local else {
            return Some(ui_core::Change::Activated);
        };
        let ys = self.rows();
        // A closed menu draws no items and they sit on the anchor's row, so they must not be
        // tested at all: otherwise the press that opens the menu names an invisible item.
        if self.open_target {
            for (k, item) in self.items.iter().enumerate() {
                // Rows travel while the menu collapses, so the press point follows the slide.
                let top = ys[k] + self.slide(k);
                if local.y >= top && local.y < top + self.metrics.item_h {
                    return Some(ui_core::Change::Selected(Some(item.id)));
                }
            }
        }
        let close_y = *ys.last()?;
        (local.y >= close_y && local.y < close_y + self.metrics.close_h)
            .then_some(ui_core::Change::Activated)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let mut column = Decl::container(self.id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::fixed(self.height()),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // Items are wider than the close button; MD3 lines them up on the trailing edge.
            .cross_align(0.0)
            .cross_pos(1.0)
            // Two different gaps, so spacing is explicit spacers rather than one column gap.
            .gap(0.0)
            // Without this the anchor jumps to the top of the reserved box when the items go.
            .child(self.grower());
        // A closed menu draws nothing above the anchor; fading to zero would leave ghost hits.
        if self.open > 0.001 {
            for (k, item) in self.items.iter().enumerate() {
                if k > 0 {
                    column = column.child(self.spacer(k, self.metrics.item_gap));
                }
                let slid = self.item_decl(item).offset(0.0, self.slide(k));
                column = column.child(faded_decl(slid, self.open));
            }
            column = column.child(self.spacer(self.items.len(), self.metrics.close_gap));
        }
        column.child(self.close_decl())
    }
}

/// Multiply every colour in a declaration by `k`, so a fading item carries its inks along.
fn faded_decl(mut decl: Decl, k: f32) -> Decl {
    decl.shapes = decl.shapes.into_iter().map(|s| fade_shape(s, k)).collect();
    if let NodeData::Text { ink, .. } = &mut decl.data {
        *ink = faded(*ink, k);
    }
    let children = std::mem::take(&mut decl.children);
    decl.children = children
        .into_iter()
        .map(|c| std::rc::Rc::new(faded_decl((*c).clone(), k)))
        .collect();
    decl
}

fn fade_shape(shape: Shape, k: f32) -> Shape {
    match shape {
        Shape::Box {
            corners,
            fill,
            gradient,
            border,
            shadow,
            image,
        } => Shape::Box {
            corners,
            fill: faded(fill, k),
            gradient,
            border: border.map(|(c, w)| (faded(c, k), w)),
            shadow,
            image,
        },
        Shape::Rect {
            rect,
            corners,
            fill,
            gradient,
            border,
        } => Shape::Rect {
            rect,
            corners,
            fill: faded(fill, k),
            gradient,
            border: border.map(|(c, w)| (faded(c, k), w)),
        },
        Shape::Shadow {
            rect,
            corners,
            colour,
            blur,
            dy,
            ring,
        } => Shape::Shadow {
            rect,
            corners,
            colour: faded(colour, k),
            blur,
            dy,
            ring,
        },
        Shape::Outline {
            corners,
            colour,
            gradient,
            width,
            gap,
        } => Shape::Outline {
            corners,
            colour: faded(colour, k),
            gradient: gradient.map(|g| ui_core::draw::Gradient {
                from: faded(g.from, k),
                to: faded(g.to, k),
                ..g
            }),
            width,
            gap,
        },
        Shape::Sector {
            centre,
            radius,
            thickness,
            trim,
            round_caps,
            colour,
        } => Shape::Sector {
            centre,
            radius,
            thickness,
            trim,
            round_caps,
            colour: faded(colour, k),
        },
        Shape::Ellipse {
            rect,
            thickness,
            fill,
            colour,
        } => Shape::Ellipse {
            rect,
            thickness,
            fill: faded(fill, k),
            colour: faded(colour, k),
        },
        Shape::Line {
            from,
            to,
            thickness,
            colour,
        } => Shape::Line {
            from,
            to,
            thickness,
            colour: faded(colour, k),
        },
    }
}

/// The export names no disabled inks; MD3's rule is `on-surface` at 12%.
fn container_of(pack: &Style, token: &'static str, disabled: bool) -> Color {
    if disabled {
        Color {
            a: crate::DISABLED_CONTAINER_OPACITY,
            ..role(pack, "onSurface")
        }
    } else {
        spec_ink(pack, token, 1.0)
    }
}

/// MD3's elevation as the one analytic shadow the engine has.
fn shadow_of(elevation: f32) -> Option<ShadowSpec> {
    (elevation > 0.0).then(|| ShadowSpec::black(elevation * 2.0, 0.18, elevation * 0.5))
}
