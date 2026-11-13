//! MD3 menu: a surface of command rows, each with an optional mark, supporting line and
//! trailing shortcut. The engine's own context menu keeps its `menu_style` look.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{Change, CursorShape, Decl, Id, InkRadius, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, Icon, IconSource, Widget, WidgetState};

use crate::layer::press_ink;
use crate::motion::REACTION;
use crate::paint::{role, spec_ink};
use crate::spec::{MenusFlags, MenusSpec, divider_spec, menus_spec};
use crate::typescale;

/// `rounded_corners` on the shape scale: the menu container's and every row's radius.
const MENU_RADIUS: f32 = 4.0;
/// M3's floor for a menu's width; the export carries no width of its own.
const MENU_MIN_WIDTH: f32 = 112.0;
/// A section label's height; the export names its ink only.
const SECTION_HEIGHT: f32 = 24.0;

/// One command row.
#[derive(Clone, Debug)]
pub struct Md3MenuItem {
    pub id: Id,
    pub label: String,
    /// A second line under the label.
    pub supporting: Option<String>,
    /// The mark before the label, drawn in the row's icon ink.
    pub leading: Option<IconSource>,
    /// The mark after the shortcut, drawn in the row's icon ink.
    pub trailing: Option<IconSource>,
    /// A chord or short value at the trailing edge, before the trailing mark.
    pub shortcut: Option<String>,
    /// The app owns selection: the row paints the selected fill, it never toggles itself.
    pub selected: bool,
    pub disabled: bool,
}

impl Md3MenuItem {
    /// A plain row: a label and nothing else.
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            supporting: None,
            leading: None,
            trailing: None,
            shortcut: None,
            selected: false,
            disabled: false,
        }
    }

    pub fn supporting(mut self, text: impl Into<String>) -> Self {
        self.supporting = Some(text.into());
        self
    }

    pub fn leading(mut self, icon: IconSource) -> Self {
        self.leading = Some(icon);
        self
    }

    pub fn trailing(mut self, icon: IconSource) -> Self {
        self.trailing = Some(icon);
        self
    }

    pub fn shortcut(mut self, text: impl Into<String>) -> Self {
        self.shortcut = Some(text.into());
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// One entry in a menu: a row, a section label, or a divider.
#[derive(Clone, Debug)]
pub enum Md3MenuEntry {
    Item(Md3MenuItem),
    Section(String),
    Divider,
}

/// Everything a menu is built with.
#[derive(Clone, Debug)]
pub struct Md3MenuParams {
    pub id: Id,
    pub entries: Vec<Md3MenuEntry>,
}

impl Md3MenuParams {
    pub fn new(id: Id, entries: Vec<Md3MenuEntry>) -> Self {
        Self { id, entries }
    }
}

/// The animated facts one row resolved this frame.
struct RowPaint {
    hover: f32,
    focus: f32,
    ring: f32,
    disabled: bool,
    activated: bool,
}

/// A resolved menu: the whole surface, and the row this frame activated.
pub struct Md3Menu {
    surface: Decl,
    fired: Option<Id>,
}

impl Md3Menu {
    pub fn new_with(params: Md3MenuParams, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let Md3MenuParams { id, entries } = params;
        let rest = menus_spec(MenusFlags::NONE);
        let fill = spec_ink(pack, rest.container.color, 1.0);
        // MD3's elevation is a 1dp ambient+caster pair; one analytic shadow stands in for both.
        let shadow = ShadowSpec::black(
            rest.container.elevation * 2.0,
            0.18,
            rest.container.elevation * 0.5,
        );
        let mut fired = None;
        let mut column = Decl::container(id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::hug(MENU_MIN_WIDTH, MENU_MIN_WIDTH),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::all(rest.group.padding))
            .gap(rest.root.gap)
            // A row's fill runs the surface's whole width; the column stretches it there.
            .cross_align(1.0)
            .sense(Sense::PASS_THROUGH)
            .corners(Corners::all(MENU_RADIUS))
            .shapes(vec![Shape::Box {
                corners: Corners::all(MENU_RADIUS),
                fill,
                gradient: None,
                border: None,
                shadow: Some(shadow),
                image: None,
            }]);
        for (k, entry) in entries.into_iter().enumerate() {
            let row = match entry {
                Md3MenuEntry::Item(item) => {
                    let paint = row_paint(cx, &item);
                    if paint.activated && fired.is_none() {
                        fired = Some(item.id);
                    }
                    item_row(&rest, item, paint, pack)
                }
                Md3MenuEntry::Section(text) => {
                    section_row(id.with(("section", k)), text, &rest, pack)
                }
                Md3MenuEntry::Divider => divider_row(id.with(("divider", k)), &rest, pack),
            };
            column = column.child(row);
        }
        Self {
            surface: column,
            fired,
        }
    }
}

/// Read one row's state and drive its three fades: hover, focus, and the ring's arrival.
fn row_paint(cx: &mut Cx<'_, '_>, item: &Md3MenuItem) -> RowPaint {
    let mut node = cx.at(item.id);
    let state = node.state();
    let disabled = item.disabled || state.disabled;
    let hover = node.animate(
        item.id.with(param::HOVER),
        f32::from(state.hovered && !disabled),
        REACTION,
    );
    let focus = node.animate(
        item.id.with(param::FOCUS),
        f32::from(state.focus_visible && !disabled),
        REACTION,
    );
    let ring = node.animate(
        item.id.with(param::THICKNESS),
        f32::from(state.focus_visible && !disabled),
        REACTION,
    );
    RowPaint {
        hover,
        focus,
        ring,
        disabled,
        activated: state.activation() && !disabled,
    }
}

/// One command row: its fill, state layer, ring, and the content the item asked for.
fn item_row(rest: &MenusSpec, item: Md3MenuItem, paint: RowPaint, pack: &Style) -> Decl {
    let Md3MenuItem {
        id,
        label,
        supporting,
        leading,
        trailing,
        shortcut,
        selected,
        ..
    } = item;
    let disabled = paint.disabled;
    let spec = menus_spec(MenusFlags {
        selected,
        disabled,
        ..MenusFlags::NONE
    });
    let hovered = menus_spec(MenusFlags {
        selected,
        hovered: true,
        disabled,
        ..MenusFlags::NONE
    });
    let focused = menus_spec(MenusFlags {
        selected,
        focused: true,
        disabled,
        ..MenusFlags::NONE
    });
    let pressed = menus_spec(MenusFlags {
        selected,
        pressed: true,
        disabled,
        ..MenusFlags::NONE
    });
    let fill = selected.then(|| {
        spec_ink(
            pack,
            spec.menu_item_container.color,
            spec.menu_item_container.opacity,
        )
    });
    let layer_ink = role(pack, hovered.menu_item_state_layer.color);
    let strength = (paint.hover * hovered.menu_item_state_layer.opacity)
        .max(paint.focus * focused.menu_item_state_layer.opacity);
    let layer = (strength > 0.0 && !disabled).then_some(Color {
        a: layer_ink.a * strength,
        ..layer_ink
    });
    let press = (!disabled).then(|| {
        press_ink(
            Color {
                a: layer_ink.a * pressed.menu_item_state_layer.opacity,
                ..layer_ink
            },
            InkRadius::Cover,
        )
    });
    let ring_ink = spec_ink(pack, focused.menu_item_focus_indicator.color, 1.0);
    let ring = (paint.ring > 0.0 && !disabled).then_some((
        Color {
            a: ring_ink.a * paint.ring,
            ..ring_ink
        },
        focused.menu_item_focus_indicator.thickness,
    ));
    let label_ink = spec_ink(
        pack,
        spec.menu_item_label_text.color,
        spec.menu_item_label_text.opacity,
    );
    let support_ink = spec_ink(
        pack,
        spec.menu_item_supporting_text.color,
        spec.menu_item_supporting_text.opacity,
    );
    let icon_ink = spec_ink(
        pack,
        spec.menu_item_leading_icon.color,
        spec.menu_item_leading_icon.opacity,
    );
    let shortcut_ink = spec_ink(
        pack,
        spec.menu_item_trailing_supporting_text.color,
        spec.menu_item_trailing_supporting_text.opacity,
    );

    let mut shapes = Vec::with_capacity(3);
    for fill in [fill, layer].into_iter().flatten() {
        shapes.push(Shape::Box {
            corners: Corners::all(MENU_RADIUS),
            fill,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        });
    }
    if let Some((ink, width)) = ring {
        // The export insets the ring by 3 dp; the decl cannot know the final rect, so the
        // ring is drawn on the row's own edge, as the button draws its own.
        shapes.push(Shape::Box {
            corners: Corners::all(MENU_RADIUS),
            fill: Color::TRANSPARENT,
            gradient: None,
            border: Some((ink, width)),
            shadow: None,
            image: None,
        });
    }
    let mut row = Decl::container(id, NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::hug(rest.menu_item.height, rest.menu_item.height),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .padding(Insets {
            left: rest.menu_item.leading_space,
            right: rest.menu_item.trailing_space,
            top: rest.menu_item.top_space,
            bottom: rest.menu_item.bottom_space,
        })
        .gap(rest.menu_item.between_space)
        .cross_align(0.0)
        .cross_pos(0.5)
        .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
        .cursor(if disabled {
            CursorShape::Arrow
        } else {
            CursorShape::PointingHand
        })
        .disabled(disabled)
        .press_ink(press)
        .corners(Corners::all(MENU_RADIUS))
        .shapes(shapes);
    if let Some(source) = leading {
        row = row.child(
            Icon {
                id: id.with("leading"),
                source,
                size: spec.menu_item_leading_icon.size,
                ink: icon_ink,
            }
            .resolve(),
        );
    }
    let mut text = Decl::container(id.with("text"), NodeKind::Column)
        .size(SizeClass::hug(0.0, 0.0))
        .cross_align(0.0)
        .child(
            Text::new(
                id.with("label"),
                label,
                spec.menu_item_label_text.size,
                label_ink,
            )
            .weight(weight(spec.menu_item_label_text.weight))
            .resolve(),
        );
    if let Some(supporting) = supporting {
        text = text.child(
            Text::new(
                id.with("supporting"),
                supporting,
                spec.menu_item_supporting_text.size,
                support_ink,
            )
            .weight(weight(spec.menu_item_supporting_text.weight))
            .resolve(),
        );
    }
    row = row.child(text).child(grower(id.with("grow")));
    if let Some(shortcut) = shortcut {
        row = row.child(
            Text::new(
                id.with("shortcut"),
                shortcut,
                spec.menu_item_trailing_supporting_text.size,
                shortcut_ink,
            )
            .weight(weight(spec.menu_item_trailing_supporting_text.weight))
            .resolve(),
        );
    }
    if let Some(source) = trailing {
        row = row.child(
            Icon {
                id: id.with("trailing"),
                source,
                size: spec.menu_item_trailing_icon.size,
                ink: icon_ink,
            }
            .resolve(),
        );
    }
    row
}

/// The font's own weight; the export writes it as a number.
fn weight(value: f32) -> Weight {
    Weight(value.round() as u16)
}

/// A section label: muted small text, no pointer target.
fn section_row(id: Id, text: String, spec: &MenusSpec, pack: &Style) -> Decl {
    let ink = spec_ink(pack, spec.section_label_text.color, 1.0);
    Decl::container(id, NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::hug(SECTION_HEIGHT, SECTION_HEIGHT),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .padding(Insets {
            left: spec.menu_item.leading_space,
            right: spec.menu_item.trailing_space,
            top: 0.0,
            bottom: 0.0,
        })
        .cross_align(0.0)
        .cross_pos(0.5)
        .sense(Sense::PASS_THROUGH)
        .child(
            Text::new(id.with("label"), text, typescale::LABEL_SMALL.size, ink)
                .weight(typescale::LABEL_SMALL.weight)
                .resolve(),
        )
}

/// A hairline that stops short of the surface's edges.
fn divider_row(id: Id, spec: &MenusSpec, pack: &Style) -> Decl {
    let divider = divider_spec();
    let ink = spec_ink(pack, divider.root.color, 1.0);
    let line = Decl::container(id.with("line"), NodeKind::Box)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fixed(divider.root.thickness),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .shapes(vec![Shape::Box {
            corners: Corners::ZERO,
            fill: ink,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        }]);
    Decl::container(id, NodeKind::Row)
        .size(SizeClass::hug(0.0, 0.0))
        .padding(Insets {
            left: spec.menu_item.leading_space,
            right: spec.menu_item.trailing_space,
            top: 0.0,
            bottom: 0.0,
        })
        .sense(Sense::PASS_THROUGH)
        .child(line)
}

/// A zero-basis grower: it takes the space left over and draws nothing.
fn grower(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::Yes,
        grow_y: Grow::No,
    })
}

impl Widget for Md3Menu {
    fn handles_disabled(&self) -> bool {
        // Each row paints its own 38% ink, so the adapter must not fade the surface again.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        self.fired.map(|id| Change::Selected(Some(id)))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        self.surface.clone()
    }
}
