//! The glass search bar: a capsule field with a clear button and an optional cancel button.

use std::cell::Cell;

use ui_core::geom::{Insets, Rect};
use ui_core::layout::{AxisSize, Clip, Grow, Overflow, SizeClass};
use ui_core::{AnimSpec, Change, Decl, Easing, Id, NodeKind};
use ui_style::Style;
use ui_widgets::{Cx, IconSource, Widget};

use crate::button::{GlassButton, GlassButtonParams};
use crate::text_field::{FIELD_SEARCH_H, GlassTextField, GlassTextFieldParams};

/// Everything a search bar is built with; the query belongs to the app.
pub struct GlassSearchBarParams<'a> {
    pub id: Id,
    pub text: &'a str,
    pub placeholder: &'a str,
    pub width: f32,
    /// The cancel button's presence; it slides in over 250 ms.
    pub cancel: bool,
    pub cancel_icon: IconSource,
    pub disabled: bool,
}

impl<'a> GlassSearchBarParams<'a> {
    pub fn new(id: Id, text: &'a str, width: f32) -> Self {
        Self {
            id,
            text,
            placeholder: "",
            width,
            cancel: false,
            cancel_icon: IconSource::sf("xmark"),
            disabled: false,
        }
    }

    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    pub fn cancel(mut self, cancel: bool) -> Self {
        self.cancel = cancel;
        self
    }

    pub fn cancel_icon(mut self, icon: IconSource) -> Self {
        self.cancel_icon = icon;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// One glass search bar; the field, its clear button and the cancel button are its parts.
pub struct GlassSearchBar {
    id: Id,
    width: f32,
    cancel_t: f32,
    field: GlassTextField,
    cancel: Option<GlassButton>,
    clear_id: Id,
    cancel_id: Id,
    clear_clicked: Cell<bool>,
    cancel_clicked: Cell<bool>,
}

impl GlassSearchBar {
    pub fn new(params: GlassSearchBarParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let GlassSearchBarParams {
            id,
            text,
            placeholder,
            width,
            cancel,
            cancel_icon,
            disabled,
        } = params;
        let cancel_id = id.with("cancel");
        let clear_id = id.with("field").with("trailing");
        // `AnimatedSize`: 250 ms easeInOutCubic; the button's own ink does not fade.
        let cancel_t = cx.at(id).animate(
            id.with("cancel-t"),
            f32::from(cancel && !disabled),
            AnimSpec::new(CANCEL_GROW, Easing::Cubic(0.4, 0.0, 0.2, 1.0)),
        );
        let field_w = width - (FIELD_SEARCH_H + CANCEL_GAP) * cancel_t;
        let field = GlassTextField::new(
            GlassTextFieldParams::search(id.with("field"), text, field_w.max(0.0))
                .placeholder(placeholder)
                .leading(IconSource::sf("magnifyingglass"))
                .trailing_button(
                    IconSource::sf("xmark_circle_fill"),
                    CLEAR_ICON,
                    !text.is_empty(),
                )
                .disabled(disabled),
            cx,
            pack,
        );
        let cancel = (cancel_t > 0.001).then(|| {
            GlassButton::new(
                GlassButtonParams {
                    width: Some(FIELD_SEARCH_H),
                    height: FIELD_SEARCH_H,
                    icon_size: CANCEL_ICON,
                    ..GlassButtonParams::icon(cancel_id, cancel_icon)
                },
                &mut cx.at(cancel_id),
                pack,
            )
        });
        Self {
            id,
            width,
            cancel_t,
            field,
            cancel,
            clear_id,
            cancel_id,
            clear_clicked: Cell::new(false),
            cancel_clicked: Cell::new(false),
        }
    }
}

impl Widget for GlassSearchBar {
    fn handles_disabled(&self) -> bool {
        true
    }

    fn change(&self, _state: &ui_widgets::WidgetState, _bounds: Rect) -> Option<Change> {
        // The bar names the part that was pressed; clearing the query is the app's call.
        if self.clear_clicked.get() {
            return Some(Change::Selected(Some(self.clear_id)));
        }
        self.cancel_clicked
            .get()
            .then(|| Change::Selected(Some(self.cancel_id)))
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        self.clear_clicked.set(cx.state(self.clear_id).clicked);
        self.cancel_clicked.set(cx.state(self.cancel_id).clicked);
        let mut bar = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::fixed(FIELD_SEARCH_H),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .child(self.field.decl(cx));
        if let Some(button) = &self.cancel {
            // The slot grows from zero and clips the full-size button: a slide-in, not a
            // squeeze — the button keeps its 44 dp capsule while it enters.
            bar = bar.child(
                Decl::container(self.id.with("cancel-slot"), NodeKind::Stack)
                    .size(SizeClass::fixed(
                        FIELD_SEARCH_H * self.cancel_t,
                        FIELD_SEARCH_H,
                    ))
                    .margin(Insets {
                        left: CANCEL_GAP * self.cancel_t,
                        ..Insets::ZERO
                    })
                    .clip(Clip {
                        x: Overflow::Hidden,
                        y: Overflow::Hidden,
                    })
                    .child(button.decl(cx)),
            );
        }
        bar
    }
}

/// The clear glyph's size and the cancel button's, both from `GlassSearchBar`.
pub const CLEAR_ICON: f32 = 18.0;
pub const CANCEL_ICON: f32 = 24.0;
pub const CANCEL_GAP: f32 = 10.0;
/// `AnimatedSize`'s 250 ms, `Curves.easeInOutCubic`.
pub const CANCEL_GROW: f32 = 0.250;
