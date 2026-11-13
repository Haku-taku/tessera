//! MD3 top app bar: a container holding a leading slot, a title and trailing actions.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{AnimSpec, Change, CursorShape, Decl, Id, InkRadius, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::Background;
use crate::paint::{lerp, spec_ink};
use crate::spec::{AppBarFlags, AppBarSize, app_bar_spec};
use crate::typescale;

/// The small bar's height, which is also the band a medium or large bar keeps its slot in.
pub const APP_BAR_TOP_BAND: f32 = 64.0;
/// MD3's icon-button touch target; the export's `leading-space` sits outside it.
pub const APP_BAR_SLOT: f32 = 48.0;
/// The mark inside a slot, from `md.comp.app-bar.icon.size`.
pub const APP_BAR_ICON: f32 = 24.0;
/// `NavigationToolbar.kMiddleSpacing`: the title follows the leading slot by this much.
pub const APP_BAR_TITLE_GAP: f32 = 16.0;
/// A title with nothing leading it starts this far from the bar's edge.
pub const APP_BAR_TITLE_INSET: f32 = 16.0;

/// The scroll-under transition: MD3's standard pair.
const COLLAPSE: AnimSpec = crate::motion::STANDARD_TRANSITION;
use crate::motion::REACTION;

/// The six app bars the export tokenises; each fixes its height, title scale and padding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppBarVariant {
    Small,
    Medium,
    Large,
    /// Collapses to the small bar as its content scrolls under it.
    MediumFlexible,
    LargeFlexible,
    /// The 56dp pill with a search label instead of a title.
    Search,
}

impl AppBarVariant {
    /// The generated spec's axes: `(large_flexible, medium_flexible, search, size)`.
    fn axes(self) -> (bool, bool, bool, AppBarSize) {
        match self {
            Self::Small => (false, false, false, AppBarSize::Small),
            Self::Medium => (false, false, false, AppBarSize::Medium),
            Self::Large => (false, false, false, AppBarSize::Large),
            Self::MediumFlexible => (false, true, false, AppBarSize::Medium),
            Self::LargeFlexible => (true, false, false, AppBarSize::Large),
            Self::Search => (false, false, true, AppBarSize::Small),
        }
    }

    /// Whether the bar shrinks from its expanded height to the small one.
    pub fn flexible(self) -> bool {
        matches!(self, Self::MediumFlexible | Self::LargeFlexible)
    }

    /// The height this variant takes when expanded, straight from the export; a caller that
    /// reserves space for the bar needs it before the bar exists.
    pub fn expanded_height(self, with_subtitle: bool) -> f32 {
        let (large_flex, medium_flex, search, size) = self.axes();
        app_bar_spec(
            AppBarFlags {
                large_flexible: large_flex,
                medium_flexible: medium_flex,
                search,
                with_subtitle,
                ..AppBarFlags::NONE
            },
            size,
        )
        .container
        .height
    }

    /// The gap under the title when it is anchored to the bar's bottom edge.
    fn title_bottom(self) -> f32 {
        match self {
            Self::Medium | Self::MediumFlexible => 20.0,
            Self::Large | Self::LargeFlexible => 28.0,
            Self::Small | Self::Search => 0.0,
        }
    }

    /// Whether the title sits in the top band with the slots rather than in its own band.
    fn inline_title(self) -> bool {
        matches!(self, Self::Small | Self::Search)
    }
}

/// What leads the title.
pub enum AppBarLeading {
    None,
    /// A 24dp mark inside the 48dp icon-button slot, in `leading-icon` ink.
    Icon(IconSource),
    /// A 32dp avatar (`avatar.size`) centred in the slot; the app builds it.
    Avatar(Decl),
}

/// One trailing action: the id the app knows it by and its mark.
pub struct AppBarAction {
    pub id: Id,
    /// The 24dp mark in `trailing-icon` ink, as a source the slot resolves.
    pub icon: IconSource,
}

/// Everything an app bar is built with. Variant, slots and width are fixed at build time.
pub struct AppBarParams<'a> {
    pub id: Id,
    pub variant: AppBarVariant,
    pub title: &'a str,
    /// Only the flexible variants tokenise a subtitle; elsewhere it is dropped.
    pub subtitle: Option<&'a str>,
    pub leading: AppBarLeading,
    pub actions: &'a [AppBarAction],
    /// The width the bar will be laid out at; the title wraps inside it.
    pub width: f32,
}

/// A top app bar; `scrolled` and `collapsed` are the two facts its app owns.
pub struct Md3AppBar {
    id: Id,
    height: f32,
    /// The width the app said the bar will get; the title band and its wrap use it.
    width: f32,
    corners: Corners,
    surface: Color,
    shadow: Option<ShadowSpec>,
    leading_space: f32,
    trailing_space: f32,
    inline_title: bool,
    /// The gap between the leading slot and an inline title; the band sets its own inset.
    title_gap: f32,
    leading: Option<Decl>,
    actions: Vec<Decl>,
    title: Option<Decl>,
    subtitle: Option<Decl>,
    title_left: f32,
    title_bottom: f32,
    disabled: bool,
    /// The slot whose press landed this frame.
    fired: Option<Id>,
}

impl Md3AppBar {
    /// Build one bar. `collapsed` only moves the flexible variants.
    pub fn new_with(
        params: Md3Options,
        content: AppBarParams<'_>,
        scrolled: bool,
        collapsed: bool,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let AppBarParams {
            id,
            variant,
            title,
            subtitle,
            leading,
            actions,
            width,
        } = content;
        let (large_flex, medium_flex, search, size) = variant.axes();
        let with_subtitle = subtitle.is_some() && variant.flexible();
        // The generated spec writes title and subtitle onto the same slot, so ask twice.
        let base = app_bar_spec(
            AppBarFlags {
                large_flexible: large_flex,
                medium_flexible: medium_flex,
                on_scroll: scrolled,
                search,
                with_subtitle,
                ..AppBarFlags::NONE
            },
            size,
        );
        let title_spec = app_bar_spec(
            AppBarFlags {
                large_flexible: large_flex,
                medium_flexible: medium_flex,
                on_scroll: scrolled,
                search,
                title: true,
                with_subtitle,
                ..AppBarFlags::NONE
            },
            size,
        );
        let sub_spec = app_bar_spec(
            AppBarFlags {
                large_flexible: large_flex,
                medium_flexible: medium_flex,
                on_scroll: scrolled,
                search,
                subtitle: true,
                with_subtitle,
                ..AppBarFlags::NONE
            },
            size,
        );

        let mut node = cx.at(id);
        let disabled = params.disabled || node.state().disabled;
        let collapse = if variant.flexible() {
            node.animate(id.with("collapse"), f32::from(collapsed), COLLAPSE)
        } else {
            0.0
        };

        // The base variant carries no height of its own; every real variant does.
        let expanded = match base.container.height {
            0.0 => APP_BAR_TOP_BAND,
            height => height,
        };
        let height = lerp(expanded, APP_BAR_TOP_BAND, collapse);
        let fade = if disabled {
            crate::DISABLED_CONTENT_OPACITY
        } else {
            1.0
        };

        let scale = if !base.label_text.font.is_empty() {
            base.label_text.font
        } else {
            title_spec.root.font
        };
        let mut title_style = typescale::named(scale);
        if variant.flexible() {
            title_style.size = lerp(title_style.size, typescale::TITLE_LARGE.size, collapse);
        }
        let title_left = if variant.inline_title() {
            leading_inset(&leading, base.root.leading_space)
        } else {
            lerp(
                APP_BAR_TITLE_INSET,
                leading_inset(&leading, base.root.leading_space),
                collapse,
            )
        };
        // Centring the collapsed title needs its own height, which only the small scale fixes.
        let collapsed_bottom = (APP_BAR_TOP_BAND - typescale::TITLE_LARGE.size * 1.4) * 0.5;
        let title_bottom = lerp(variant.title_bottom(), collapsed_bottom, collapse);

        let slots = width - title_left - APP_BAR_TITLE_INSET;
        let inline_slots = slots - actions.len() as f32 * APP_BAR_SLOT - base.root.trailing_space;
        let max_w = if variant.inline_title() {
            inline_slots
        } else {
            slots
        }
        .max(0.0);
        // An inline title is laid out by the row's flow, so the gap is what is left over
        // between the inset and the slot that already precedes it.
        let title_gap = title_left
            - base.root.leading_space
            - if matches!(leading, AppBarLeading::None) {
                0.0
            } else {
                APP_BAR_SLOT
            };

        let title_decl = Text::new(
            id.with("title"),
            title,
            title_style.size,
            spec_ink(pack, title_spec.root.color, fade),
        )
        .weight(title_style.weight)
        .max_width(max_w)
        .resolve();
        let subtitle_decl = subtitle.filter(|_| with_subtitle).map(|text| {
            let style = typescale::named(sub_spec.root.font);
            let ink = spec_ink(pack, sub_spec.root.color, fade * (1.0 - collapse));
            // The subtitle's line shrinks with the bar, so the title slides down instead of
            // jumping on the frame the line disappears.
            Decl::container(id.with("subtitle-row"), NodeKind::Column)
                .size(SizeClass {
                    width: AxisSize::fixed(max_w),
                    height: AxisSize::fixed(style.size * 1.4 * (1.0 - collapse)),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .cross_align(0.0)
                .child(
                    Text::new(id.with("subtitle"), text, style.size, ink)
                        .weight(style.weight)
                        .max_width(max_w)
                        .resolve(),
                )
        });

        let layer_ink = spec_ink(pack, "onSurface", 1.0);
        let leading_decl = match leading {
            AppBarLeading::None => None,
            AppBarLeading::Icon(build) => Some(icon_slot(
                id.with("leading"),
                build,
                spec_ink(pack, base.leading_icon.color, fade),
                layer_ink,
                disabled,
                cx,
            )),
            AppBarLeading::Avatar(avatar) => Some(avatar_slot(id.with("leading"), avatar)),
        };
        let action_decls: Vec<Decl> = actions
            .iter()
            .map(|a| {
                icon_slot(
                    a.id,
                    a.icon,
                    spec_ink(pack, base.trailing_icon.color, fade),
                    layer_ink,
                    disabled,
                    cx,
                )
            })
            .collect();
        let fired = actions
            .iter()
            .map(|a| a.id)
            .chain([id.with("leading")])
            .find(|slot| cx.state(*slot).activation());

        Self {
            id,
            height,
            width,
            corners: match base.container.shape {
                "circular" => Corners::all(height * 0.5),
                _ => Corners::ZERO,
            },
            surface: spec_ink(pack, base.container.color, 1.0),
            shadow: (base.container.elevation > 0.0).then(|| {
                ShadowSpec::black(
                    base.container.elevation * 2.0,
                    0.18,
                    base.container.elevation * 0.5,
                )
            }),
            leading_space: base.root.leading_space,
            trailing_space: base.root.trailing_space,
            inline_title: variant.inline_title(),
            title_gap,
            leading: leading_decl,
            actions: action_decls,
            title: Some(title_decl),
            subtitle: subtitle_decl,
            title_left,
            title_bottom,
            disabled,
            fired,
        }
    }

    /// The bar's own height this frame; a flexible one shrinks as it collapses.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// The leading slot, the title when it is inline, then the actions at the far end.
    fn top_band(&self) -> Decl {
        let mut row = Decl::container(self.id.with("top"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.height.min(APP_BAR_TOP_BAND)),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: self.leading_space,
                top: 0.0,
                right: self.trailing_space,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(0.5);
        if let Some(leading) = &self.leading {
            row = row.child(leading.clone());
        }
        if self.inline_title
            && let Some(title) = &self.title
        {
            row = row.child(
                Decl::container(self.id.with("title-gap"), NodeKind::Column)
                    .padding(Insets {
                        left: self.title_gap,
                        top: 0.0,
                        right: 0.0,
                        bottom: 0.0,
                    })
                    .cross_align(0.0)
                    .child(title.clone()),
            );
        }
        row = row.child(spacer(self.id.with("gap")));
        for action in &self.actions {
            row = row.child(action.clone());
        }
        row
    }

    /// The title band: a full-height overlay, so a bottom anchor survives any bar height.
    ///
    /// `cross_pos` is the container's knob, so the row inside the overlay owns the anchor.
    fn title_band(&self) -> Option<Decl> {
        if self.inline_title {
            return None;
        }
        let mut column = Decl::container(self.id.with("title-col"), NodeKind::Column)
            .cross_align(0.0)
            .cross_pos(0.0)
            // The title is decoration: a press on it belongs to whatever is under the band.
            .sense(Sense::PASS_THROUGH);
        column = column.child(self.title.clone()?);
        if let Some(subtitle) = &self.subtitle {
            column = column.child(subtitle.clone());
        }
        let row = Decl::container(self.id.with("title-row"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: self.title_left,
                top: 0.0,
                right: APP_BAR_TITLE_INSET,
                bottom: self.title_bottom,
            })
            .cross_align(0.0)
            .cross_pos(1.0)
            .sense(Sense::PASS_THROUGH)
            .child(column);
        Some(
            Decl::container(self.id.with("title-band"), NodeKind::Overlay)
                .size(SizeClass {
                    width: AxisSize::fixed(self.width),
                    height: AxisSize::fixed(self.height),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                // The band spans the whole bar, so it must let presses reach the slots.
                .sense(Sense::PASS_THROUGH)
                .child(row),
        )
    }
}

impl Widget for Md3AppBar {
    fn handles_disabled(&self) -> bool {
        // The 38% fade is resolved per ink, so the adapter must not fade the subtree again.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> {
        if self.disabled {
            return None;
        }
        self.fired.map(|slot| Change::Selected(Some(slot)))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let mut root = Decl::container(self.id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .disabled(self.disabled)
            .shapes(vec![Shape::Box {
                corners: self.corners,
                fill: self.surface,
                gradient: None,
                border: None,
                shadow: self.shadow,
                image: None,
            }]);
        root = root.child(self.top_band());
        if let Some(band) = self.title_band() {
            root = root.child(band);
        }
        root
    }
}

/// Where the title starts: past the leading slot when there is one, else at its own inset.
fn leading_inset(leading: &AppBarLeading, leading_space: f32) -> f32 {
    match leading {
        AppBarLeading::None => APP_BAR_TITLE_INSET,
        _ => leading_space + APP_BAR_SLOT + APP_BAR_TITLE_GAP,
    }
}

/// A zero-basis grower: it takes the spare space so the actions reach the far end.
fn spacer(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::Yes,
        grow_y: Grow::No,
    })
}

/// The 40dp disc an icon button paints into, inside the 48dp slot's 4dp inset.
const ICON_DISC: Rect = Rect::new(4.0, 4.0, 40.0, 40.0);
/// `md.sys.state.hover.state-layer-opacity`, and the 10% that focus and press share.
const HOVER_LAYER: f32 = 0.08;
const FOCUS_PRESS_LAYER: f32 = 0.10;

/// The shared shell of an icon-button slot: state layer, press ink, pointer and the mark.
fn icon_slot(
    id: Id,
    source: IconSource,
    ink: Color,
    layer_ink: Color,
    disabled: bool,
    cx: &mut Cx<'_, '_>,
) -> Decl {
    let mut node = cx.at(id);
    let state = node.state();
    let hover = node.animate(id.with(param::HOVER), f32::from(state.hovered), REACTION);
    let focus = node.animate(
        id.with(param::FOCUS),
        f32::from(state.focus_visible),
        REACTION,
    );
    // The ripple belongs to the 40dp disc, not the 48dp slot: the disc is the effective
    // background, as it is for the toolbar's icon buttons.
    let background = Background::new(ICON_DISC, Corners::all(ICON_DISC.size.w * 0.5));
    let alpha = (hover * HOVER_LAYER).max(focus * FOCUS_PRESS_LAYER);
    let mut shapes = Vec::with_capacity(1);
    if alpha > 0.0 && !disabled {
        shapes.push(background.fill(Color {
            a: layer_ink.a * alpha,
            ..layer_ink
        }));
    }
    let press = (!disabled).then(|| {
        background.press(
            Color {
                a: layer_ink.a * FOCUS_PRESS_LAYER,
                ..layer_ink
            },
            InkRadius::Cover,
        )
    });
    // A Stack centres the mark's em box; `Text`'s line box would hang it high.
    Decl::container(id, NodeKind::Stack)
        .size(SizeClass::fixed(APP_BAR_SLOT, APP_BAR_SLOT))
        .stack_align(ui_core::layout::OverlayAlign::Center)
        .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
        .cursor(if disabled {
            CursorShape::Arrow
        } else {
            CursorShape::PointingHand
        })
        .disabled(disabled)
        .press_ink(press)
        // The slot's curve is a disc, stated rather than left to the shape it happens to
        // paint: a press near the corner must not fill the square around the state layer.
        .corners(Corners::all(APP_BAR_SLOT * 0.5))
        .shapes(shapes)
        .child(source.resolve_with_size(id.with("icon"), ink, APP_BAR_ICON))
}

/// An avatar slot: the app's own 32dp mark, centred like an icon.
fn avatar_slot(id: Id, avatar: Decl) -> Decl {
    Decl::container(id, NodeKind::Column)
        .size(SizeClass::fixed(APP_BAR_SLOT, APP_BAR_SLOT))
        .padding(Insets {
            left: 0.0,
            top: (APP_BAR_SLOT - 32.0) * 0.5,
            right: 0.0,
            bottom: 0.0,
        })
        .cross_align(0.0)
        .cross_pos(0.5)
        .child(avatar)
}
