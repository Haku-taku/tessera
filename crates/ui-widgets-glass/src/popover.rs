//! The glass popover: the menu's frosted sheet, holding the app's own blocks.

use ui_core::geom::{Insets, Rect};
use ui_core::layout::{Anchor, Side};
use ui_core::shortcut::ShortcutId;
use ui_core::{Change, Decl, Id};
use ui_style::Style;
use ui_widgets::{Cx, Widget, WidgetState};

use crate::material::Control;
use crate::overlay::{Panel, PanelParams};

/// `GlassPopover.popoverWidth` and `popoverBorderRadius`.
pub const POPOVER_W: f32 = 280.0;
pub const POPOVER_RADIUS: f32 = 24.0;
/// The reference leaves padding to the content builder; 16 dp is its own example.
pub const POPOVER_PAD: f32 = 16.0;

/// The id the sheet binds Escape to.
pub const POPOVER_DISMISS_SHORTCUT: ShortcutId = ShortcutId("glass.popover.dismiss");

/// Everything a popover is built with; the app owns what the content says and what a
/// press inside it means.
pub struct GlassPopoverParams {
    pub id: Id,
    /// The node the sheet unfolds from.
    pub anchor: Anchor,
    pub side: Side,
    /// `IsOpen`; a closed popover paints nothing and takes no keyboard.
    pub open: bool,
    pub width: f32,
    /// `popoverHeight`: a fixed height for content that scrolls itself.
    pub height: Option<f32>,
    pub radius: f32,
    pub padding: Insets,
    pub content: Vec<Decl>,
    /// Ids inside `content` whose press the sheet reports; their widgets are the app's.
    pub presses: Vec<Id>,
    /// `barrierDismissible`: whether a press outside the sheet closes it.
    pub dismissible: bool,
    pub disabled: bool,
}

impl GlassPopoverParams {
    pub fn new(id: Id, anchor: Anchor) -> Self {
        Self {
            id,
            anchor,
            side: Side::Below,
            open: true,
            width: POPOVER_W,
            height: None,
            radius: POPOVER_RADIUS,
            padding: Insets::all(POPOVER_PAD),
            content: Vec::new(),
            presses: Vec::new(),
            dismissible: true,
            disabled: false,
        }
    }

    pub fn content(mut self, content: Vec<Decl>) -> Self {
        self.content = content;
        self
    }

    pub fn presses(mut self, presses: Vec<Id>) -> Self {
        self.presses = presses;
        self
    }

    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }

    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// A glass popover: the sheet, and the blocks the app put in it.
pub struct GlassPopover {
    panel: Panel,
    content: Vec<Decl>,
    /// The content id pressed this frame, read from its own state.
    fired: Option<Id>,
    dismissible: bool,
    disabled: bool,
}

impl GlassPopover {
    pub fn new(params: GlassPopoverParams, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let GlassPopoverParams {
            id,
            anchor,
            side,
            open,
            width,
            height,
            radius,
            padding,
            content,
            presses,
            dismissible,
            disabled,
        } = params;
        let fired = presses.into_iter().find(|id| cx.state(*id).clicked);
        Self {
            panel: Panel::new(
                PanelParams {
                    id,
                    anchor,
                    side,
                    open,
                    width,
                    height,
                    radius,
                    padding,
                    // Content blocks bring their own spacing; the sheet adds none.
                    gap: 0.0,
                    control: Control::Container,
                    dismiss: POPOVER_DISMISS_SHORTCUT,
                },
                cx,
                pack,
            ),
            content,
            fired,
            dismissible,
            disabled,
        }
    }
}

impl Widget for GlassPopover {
    fn handles_disabled(&self) -> bool {
        // The app's own blocks carry their own disabled look; the adapter must not add one.
        true
    }

    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<Change> {
        if self.disabled {
            return None;
        }
        if let Some(id) = self.fired {
            return Some(Change::Selected(Some(id)));
        }
        // Escape is bound on the sheet, so it reaches here only while the popover holds keys.
        let escape = state.shortcut == Some(POPOVER_DISMISS_SHORTCUT);
        (escape || (self.dismissible && state.dismissed)).then_some(Change::Dismissed)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        self.panel.decl(self.content.clone())
    }
}
