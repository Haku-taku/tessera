//! Core of the UI engine: ids, tree, layout, drawing, damage, hit testing.
//!
//! No platform, GPU, or windowing dependency lives here; that is what keeps the
//! engine testable without a compositor.

mod profile;

pub mod anim;
pub mod boxblur;
pub mod change;
pub mod clipboard;
pub mod collections;
pub mod colour;
pub mod cursor;
pub mod cx;
pub mod damage;
pub mod dnd;
pub mod drag;
pub mod draw;
pub mod effect;
pub mod event;
pub mod flex;
pub mod geom;
pub mod grapheme;
pub mod grid;
pub mod hit;
pub mod id;
pub mod ime;
pub mod layout;
pub mod layout_engine;
pub mod math;
pub mod memo;
pub mod menu;
pub mod selection;
pub mod sense;
pub mod slot;
pub mod text;
pub mod text_input;
pub use change::Change;
pub use sense::Sense;
pub mod shortcut;
pub mod tree;

pub use anim::{AnimSpec, AnimStore, Easing, PieceCubic, Status, Travel};
pub use cursor::CursorShape;
pub use cx::MathLayout;
pub use damage::{Damage, DamageAccum};
pub use dnd::{
    DndEvent, DragData, DragItem, DropEffect, DropEffects, DropOutcome, DropReport, Performer,
    mime_matches, negotiate_effect, offered_matches,
};
pub use drag::{DRAG_THRESHOLD, DragEvent, DragState, DragTracker, past_threshold};
pub use draw::{
    Batch, Caret, DrawList, GlyphKind, GlyphPrim, InkRadius, InkTarget, LinePrim, MathPrim,
    PressInk, PrimKind, Primitives, RectPrim, TextPrim,
};
pub use effect::{ColorMatrix, Effect, EffectReach, Kernel};
pub use event::{Event, Key, Modifiers, PointerButton};
pub use geom::{Color, CornerShape, Corners, Insets, Point, Rect, Size};
pub use hit::{HitBehavior, HitIndex, HitTarget};
pub use id::{Id, NodeId};
pub use ime::Preedit;
pub use layout::{
    AUTO, Align, AutoFlow, AxisSize, AxisSizes, BoxModel, Clip, Constraints, GridArea, Grow,
    LastMeasure, LayoutEngine, LayoutStats, Len, MeasureResult, Overflow, Placement, Position,
    SizeClass, Sizing, Track, TrackBound, TrackList,
};
pub use layout_engine::{BuiltinLayout, MonospaceMeasure, TextMeasure, hug_extent};
pub use math::{
    EditKey, Formula, InlineObject, MathAttach, MathCaret, MathEdit, MathEditRect, MathItem,
};
pub use memo::{CachedKey, DeclCache, Dep};
pub use menu::{MenuIcon, MenuItem, MenuRow};
pub use selection::{
    SelectionFragment, SelectionGeometry, SelectionPaint, SelectionPainter, SelectionPainterRef,
    SelectionPrim, TextSelection, flat_selection,
};
pub use shortcut::{KeyChord, ShortcutId};
pub use slot::{SlotFills, SlotName, slot_id};
pub use text::{
    AtomBox, Decoration, Family, Features, GenericFamily, GlyphOutline, InlineAtom, InlineLayout,
    InlineLine, InlineRun, LineEdges, OBJECT, Room, Span, TextAlign, TextBlock, TextRun, TextStyle,
    Variations, VerticalAlign, Visibility, Weight,
};
pub use tree::{
    ApplyStats, BackdropBlur, ContainerParams, Decl, ExitSpace, GridItem, ModalTarget, Node,
    NodeData, NodeKind, ScrollAxis, Scrollbar, ScrollbarBar, Tree,
};
