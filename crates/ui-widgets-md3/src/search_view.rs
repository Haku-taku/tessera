//! MD3 search view: the search bar's own surface, grown over the results it finds.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Size};
use ui_core::layout::{AxisSize, Clip, Grow, Overflow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{AnimSpec, CursorShape, Decl, Id, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::paint::{lerp, role};
use crate::search_bar::{ACTION_BOX, QueryField, SearchAction, icon_box, query_field};
use crate::typescale::TITLE_MEDIUM;

// The view is hand-written: the export splits it into `docked`, `full-screen` and `contained`
// sections that `gen_spec.py` flattens into one undifferentiated set of states. Every value
// below names the token it comes from.

/// `docked.header.container.height` and `full-screen.header.container.height`.
const HEADER_H: (f32, f32) = (56.0, 72.0);
/// `leading-space` / `trailing-space`: 16dp at both ends.
const SPACE: f32 = 16.0;
/// `docked.container.shape` at rest; `full-screen.container.shape` is square.
const CORNER: (f32, f32) = (28.0, 0.0);
/// `container.elevation`, against the divider's hairline.
const ELEVATION: u8 = 6;
const DIVIDER: f32 = 1.0;
/// M3's paired curve for a transition that begins and ends on screen: Standard, 300 ms.
const OPEN: AnimSpec = crate::motion::STANDARD_TRANSITION;

/// `container.color`.
const CONTAINER: &str = "surfaceContainerHigh";
/// `header.input-text.color`.
const INPUT: &str = "onSurface";
/// `header.leading-icon.color`.
const LEADING_INK: &str = "onSurface";
/// `header.trailing-icon.color`, also the hint's: the view exports no hint of its own.
const TRAILING_INK: &str = "onSurfaceVariant";
/// `divider.color`.
const DIVIDER_INK: &str = "outline";

/// Everything a search view is built with; the app owns the query, the openness and the results.
pub struct SearchViewParams<'a> {
    pub id: Id,
    pub text: &'a str,
    /// Shown while the query is empty.
    pub hint: &'a str,
    /// The app decides whether the view is open; the surface travels between the two sizes.
    pub expanded: bool,
    /// The resting bar and the open view: the two ends of the size transition.
    pub collapsed: Size,
    pub open: Size,
    pub disabled: bool,
    /// The header's leading icon: the bar's own, or the view's back arrow.
    pub leading: Option<IconSource>,
    pub actions: Vec<SearchAction>,
    /// What the view shows below the divider; laid out only while it is open at all.
    pub results: Vec<Decl>,
}

impl<'a> SearchViewParams<'a> {
    /// A view resting at `collapsed` and opening to `open`, with no hint, icon or actions.
    pub fn new(id: Id, collapsed: Size, open: Size) -> Self {
        Self {
            id,
            text: "",
            hint: "",
            expanded: false,
            collapsed,
            open,
            disabled: false,
            leading: None,
            actions: Vec::new(),
            results: Vec::new(),
        }
    }

    pub fn text(mut self, text: &'a str) -> Self {
        self.text = text;
        self
    }

    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn leading(mut self, icon: IconSource) -> Self {
        self.leading = Some(icon);
        self
    }

    pub fn action(mut self, id: Id, icon: IconSource) -> Self {
        self.actions.push(SearchAction { id, icon });
        self
    }

    pub fn results(mut self, results: Vec<Decl>) -> Self {
        self.results = results;
        self
    }
}

/// What a search view reports besides the edits the engine sends as `Change::Text`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchViewChange {
    /// One of the trailing actions was clicked.
    Action(Id),
    /// The resting bar was clicked: the app should open the view.
    Activated,
}

/// One search view, resolved against the pack.
pub struct Md3SearchView {
    id: Id,
    text: std::sync::Arc<str>,
    hint: String,
    expanded: bool,
    disabled: bool,
    /// How far the surface has opened, 0..=1; the engine moves the size, this moves the rest.
    open_t: f32,
    collapsed: Size,
    open: Size,
    leading: Option<(IconSource, Color)>,
    actions: Vec<(SearchAction, Color)>,
    fill: Color,
    shadow: Option<ShadowSpec>,
    divider: Color,
    input_ink: Color,
    hint_ink: Color,
    caret_ink: Color,
    caret: Option<usize>,
    painted: Option<ui_core::SelectionPrim>,
    preedit: Option<ui_core::Preedit>,
    results: Vec<Decl>,
    fired: Option<Id>,
}

impl Md3SearchView {
    /// Build one view. The caret and the selection come from the engine.
    pub fn new_with(
        params: Md3Options,
        content: SearchViewParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let SearchViewParams {
            id,
            text,
            hint,
            expanded,
            collapsed,
            open,
            disabled: off,
            leading,
            actions,
            results,
        } = content;
        let caret = cx.core().caret_for(id);
        let preedit = cx.core().preedit_for(id);
        let painted = cx
            .core()
            .selection_for(id)
            .filter(|selection| !selection.is_collapsed())
            .map(|selection| ui_core::SelectionPrim {
                range: selection.range(),
                fill: pack
                    .ink(ui_widgets::text::roles::SELECTION)
                    .unwrap_or(Color::TRANSPARENT),
                painter: pack.selection,
            });
        let mut node = cx.at(id);
        let disabled = params.disabled || off || node.state().disabled;
        // One clock for the whole opening: the engine walks the declared size, this walks the
        // radius, the header and the results' alpha on the same curve and the same frame.
        let open_t = node.animate(id.with("expand"), f32::from(expanded), OPEN);
        let mut fired = None;
        for action in &actions {
            if cx.state(action.id).activation() {
                fired = Some(action.id);
            }
        }
        let leading_ink = role(pack, LEADING_INK);
        let trailing_ink = role(pack, TRAILING_INK);
        Self {
            id,
            text: text.into(),
            hint: hint.to_string(),
            expanded,
            disabled,
            open_t,
            collapsed,
            open,
            leading: leading.map(|f| (f, leading_ink)),
            actions: actions.into_iter().map(|a| (a, trailing_ink)).collect(),
            fill: role(pack, CONTAINER),
            shadow: Some(ShadowSpec::elevation(ELEVATION)),
            divider: role(pack, DIVIDER_INK),
            input_ink: role(pack, INPUT),
            hint_ink: role(pack, TRAILING_INK),
            caret_ink: role(pack, INPUT),
            caret,
            painted,
            preedit,
            results,
            fired,
        }
    }

    /// The query field's width at either end: the surface less the header's own insets.
    fn input_width(&self, width: f32) -> f32 {
        let lead = if self.leading.is_some() {
            ACTION_BOX
        } else {
            0.0
        };
        let actions = self.actions.len() as f32 * ACTION_BOX;
        (width - 2.0 * SPACE - lead - actions).max(0.0)
    }
}

impl Widget for Md3SearchView {
    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let t = self.open_t;
        let target = if self.expanded {
            self.open
        } else {
            self.collapsed
        };
        let header_h = lerp(HEADER_H.0, HEADER_H.1, t);
        let corners = Corners::all(lerp(CORNER.0, CORNER.1, t));

        let mut header = Decl::container(self.id.with("header"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(header_h),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: SPACE,
                right: SPACE,
                top: 0.0,
                bottom: 0.0,
            })
            .cross_align(0.0)
            // A row's cross axis is vertical: this centres the query line in the header.
            .cross_pos(0.5)
            .sense(Sense::CLICK);
        if let Some((source, ink)) = self.leading {
            header = header.child(icon_box(self.id.with("leading"), source, ink, header_h));
        }
        header = header.child(query_field(
            cx,
            QueryField {
                id: self.id,
                text: self.text.as_ref(),
                caret: self.caret,
                preedit: self.preedit.as_ref(),
                painted: self.painted.clone(),
                size: TITLE_MEDIUM.size,
                weight: TITLE_MEDIUM.weight,
                ink: self.input_ink,
                hint: &self.hint,
                hint_ink: self.hint_ink,
                caret_ink: self.caret_ink,
                width: self.input_width(target.w),
            },
        ));
        for (action, ink) in &self.actions {
            let action_box = icon_box(action.id, action.icon, *ink, header_h);
            header = header.child(action_box.sense(Sense::CLICK));
        }

        let mut root = Decl::container(self.id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(target.w),
                height: AxisSize::fixed(target.h),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .corners(corners)
            // The open surface is a page: anything the results overflow is cut at its edge.
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            })
            .shapes(vec![Shape::Box {
                corners,
                fill: self.fill,
                gradient: None,
                border: None,
                shadow: self.shadow,
                image: None,
            }])
            // The app keeps declaring the target; the engine walks the size there before
            // layout, so the results reflow on the way instead of jumping at the end.
            .transition_size(OPEN)
            // The field is the whole surface, as it is the whole bar: a press anywhere in it
            // focuses the one query, and every edit reports under this id.
            .sense(Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::Text
            })
            .disabled(self.disabled)
            .text_input(self.text.clone())
            .child(header);
        // At rest the bar is one line with nothing under it, as in Compose: the divider and
        // the results are laid out only once the surface has started to open.
        if t > 0.0 {
            root = root.child(
                Decl::container(self.id.with("divider"), NodeKind::Box)
                    .size(SizeClass {
                        width: AxisSize::fill(0.0, 0.0),
                        height: AxisSize::fixed(DIVIDER),
                        grow_x: Grow::Yes,
                        grow_y: Grow::No,
                    })
                    .background(
                        Color {
                            a: self.divider.a * t,
                            ..self.divider
                        },
                        Corners::ZERO,
                    ),
            );
            let mut results = Decl::container(self.id.with("results"), NodeKind::Column)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::fill(0.0, 0.0),
                    grow_x: Grow::Yes,
                    grow_y: Grow::Yes,
                })
                // The results are read, not edited: the arrow stops the header's text cursor.
                .cursor(CursorShape::Arrow)
                .opacity(t);
            for result in &self.results {
                results = results.child(result.clone());
            }
            root = root.child(results);
        }
        root
    }

    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        if let Some(id) = self.fired {
            return Some(ui_core::Change::Data(Box::new(SearchViewChange::Action(
                id,
            ))));
        }
        // The resting bar is a button, not a field: clicking it asks the app to open the view.
        (!self.expanded && state.clicked)
            .then_some(ui_core::Change::Data(Box::new(SearchViewChange::Activated)))
    }
}
