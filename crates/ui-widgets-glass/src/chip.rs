//! The iOS chip: a capsule of icon, label and optional delete, tinted when selected.

use ui_core::geom::{Color, Corners, Rect};
use ui_core::layout::{OverlayAlign, SizeClass};
use ui_core::{Change, CursorShape, Decl, Id, NodeKind, Sense, Weight};
use ui_style::{Style, TextRole};
use ui_widgets::{Cx, IconSource, Widget};

use crate::HigOptions;
use crate::button::{PRESS_IN, PRESS_OUT};
use crate::paint::*;
use crate::shared::*;
use crate::text::LabelText;

/// Everything a chip is built with; the app owns what a press means.
pub struct GlassChipParams<'a> {
    pub id: Id,
    pub label: &'a str,
    pub icon: Option<IconSource>,
    pub selected: bool,
    /// Adds a delete glyph that reports on its own id (`id.with("delete")`).
    pub deletable: bool,
    pub disabled: bool,
}

impl<'a> GlassChipParams<'a> {
    pub fn new(id: Id, label: &'a str) -> Self {
        Self {
            id,
            label,
            icon: None,
            selected: false,
            deletable: false,
            disabled: false,
        }
    }

    pub fn icon(mut self, icon: IconSource) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }

    pub fn deletable(mut self) -> Self {
        self.deletable = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// One glass chip; the press curve advances in `new`, `decl()` is pure.
pub struct GlassChip {
    id: Id,
    /// Press scale progress 0..=1, on the reference's 1.03 scale.
    press: f32,
    label: Option<LabelText>,
    icon: Option<IconSource>,
    ink: Color,
    /// The selected wash: the base ink at 10 % (light) or 30 % (dark).
    tint: Color,
    selected: bool,
    deletable: bool,
    /// The delete node reported this frame.
    deleted: bool,
    disabled: bool,
    focus_ring: Option<Color>,
    /// The capsule's own face: a system fill, since a chip is content rather than chrome.
    fill: Color,
}

impl GlassChip {
    /// Takes the build `Cx`: the delete glyph reports from its own node, so the chip reads
    /// a child's state as well as advancing its own spring.
    pub fn new(params: GlassChipParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        Self::new_with(HigOptions::default(), params, cx, pack)
    }

    pub fn new_with(
        _params: HigOptions,
        params: GlassChipParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let GlassChipParams {
            id,
            label,
            icon,
            selected,
            deletable,
            disabled,
        } = params;
        let state = cx.state(id);
        let deleted = deletable && {
            let st = cx.state(id.with("delete"));
            st.activation()
        };

        // ── Press: a uniform 3 % scale about the centre (`glass_chip.dart:112`).
        let target = f32::from(state.pressed || state.drag_started || state.activated);
        let (k, c) = if target > 0.5 { PRESS_IN } else { PRESS_OUT };
        let press = {
            let mut node = cx.at(id);
            node.spring(id.with("press"), target, k, c).x
        };

        let ink = role_of(pack, "textFillColorPrimary");
        let dark = pack.appearance() == Some(ui_design::Appearance::Dark);
        let base = if dark { Color::WHITE } else { Color::BLACK };
        let tint = Color {
            a: if dark {
                CHIP_TINT_DARK
            } else {
                CHIP_TINT_LIGHT
            },
            ..base
        };
        let text = LabelText::new(pack, TextRole::Body, label, ink, f32::INFINITY)
            .map(|l| l.restyled(CHIP_FONT, Weight::MEDIUM));

        Self {
            id,
            press,
            label: text,
            icon,
            ink,
            tint,
            selected,
            deletable,
            deleted,
            disabled: disabled || state.disabled,
            focus_ring: state.focus_visible.then(|| focus_ring_ink(pack)),
            fill: role_of(pack, "glassChipFill"),
        }
    }

    /// The chip's rest box, measured from its content as the reference's intrinsics do.
    fn rest(&self, cx: &Cx<'_, '_>) -> (f32, f32) {
        let text_h = self.label.as_ref().map_or(0.0, |l| l.height());
        let h = text_h.max(CHIP_ICON) + CHIP_PAD_Y * 2.0;
        let mut w = CHIP_PAD_X * 2.0
            + self
                .label
                .as_ref()
                .map_or(0.0, |l| l.width_in(cx).unwrap_or(0.0));
        if self.icon.is_some() {
            w += ui_widgets::icon_extent(CHIP_ICON) + CHIP_GAP;
        }
        if self.deletable {
            w += CHIP_GAP + CHIP_DELETE;
        }
        (w, h)
    }
}

impl Widget for GlassChip {
    fn handles_disabled(&self) -> bool {
        // The 0.5 fade is painted here; the adapter must not fade again.
        true
    }

    fn change(&self, state: &ui_widgets::WidgetState, _bounds: Rect) -> Option<Change> {
        if self.disabled {
            return None;
        }
        if self.deleted {
            return Some(Change::Selected(Some(self.id.with("delete"))));
        }
        state.activation().then_some(Change::Activated)
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let (w, h) = self.rest(cx);
        let s = 1.0 + self.press * (CHIP_PRESS_SCALE - 1.0);
        let (fw, fh) = (w * s, h * s);
        let corners = Corners::all(fh * 0.5);

        // ── The face: the fill, the ring, and the selected wash under the content.
        let mut face = Decl::container(self.id.with("face"), NodeKind::Stack)
            .offset((w - fw) * 0.5, (h - fh) * 0.5)
            .size(SizeClass::fixed(fw, fh))
            .corners(corners)
            .background(self.fill, corners)
            .stack_align(OverlayAlign::Center);
        if let Some(ink) = self.focus_ring {
            face = face.shapes(vec![focus_ring_shape(
                Rect::new(0.0, 0.0, fw, fh),
                corners,
                ink,
            )]);
        }
        if self.selected {
            face = face.child(
                Decl::container(self.id.with("tint"), NodeKind::Box)
                    .size(SizeClass::fixed(fw, fh))
                    .background(self.tint, corners),
            );
        }

        let mut content = Decl::container(self.id.with("content"), NodeKind::Row)
            .gap(CHIP_GAP)
            .cross_align(0.0)
            .cross_pos(0.5);
        if let Some(icon) = self.icon {
            content = content.child(
                ui_widgets::Icon {
                    id: self.id.with("icon"),
                    source: icon,
                    size: CHIP_ICON * s,
                    ink: self.ink,
                }
                .resolve(),
            );
        }
        if let Some(label) = &self.label {
            content = content.child(label.scaled(s).decl(self.id.with("label")));
        }
        if self.deletable {
            // The glyph sits in its own box: a text node ignores the pointer, so the box is
            // what reports the delete.
            content = content.child(
                Decl::container(self.id.with("delete"), NodeKind::Stack)
                    .size(SizeClass::fixed(CHIP_DELETE * s, CHIP_DELETE * s))
                    .stack_align(OverlayAlign::Center)
                    .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
                    .cursor(CursorShape::PointingHand)
                    .child(
                        ui_widgets::Icon {
                            id: self.id.with("delete").with("glyph"),
                            source: IconSource::sf("xmark"),
                            size: CHIP_DELETE * s,
                            ink: self.ink,
                        }
                        .resolve(),
                    ),
            );
        }
        face = face.child(content);

        let mut root = Decl::container(self.id, NodeKind::Stack)
            .size(SizeClass::fixed(w, h))
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            // The ring is drawn on the face; the root still has to declare the stop.
            .focus_stop(true)
            .cursor(CursorShape::PointingHand)
            .child(face);
        if self.disabled {
            root = root.opacity(0.5);
        }
        root
    }
}

/// `GlassChip.padding` (`glass_chip.dart:105`), its `spacing`, icon and delete sizes.
pub const CHIP_PAD_X: f32 = 12.0;
pub const CHIP_PAD_Y: f32 = 8.0;
pub const CHIP_GAP: f32 = 6.0;
pub const CHIP_ICON: f32 = 16.0;
pub const CHIP_DELETE: f32 = 16.0;
/// The label's 14 pt at w500 (`glass_chip.dart:292-297`).
pub const CHIP_FONT: f32 = 14.0;
/// `interactionScale` 1.03 (`glass_chip.dart:112`).
/// `interactionScale` 1.03(`glass_chip.dart:112`).
pub const CHIP_PRESS_SCALE: f32 = 1.03;
/// The selected wash's alpha, light and dark (`glass_chip.dart:281-283`).
pub const CHIP_TINT_LIGHT: f32 = 0.1;
pub const CHIP_TINT_DARK: f32 = 0.3;
