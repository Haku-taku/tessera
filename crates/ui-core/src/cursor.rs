//! Pointer shapes, named after the CSS keywords `wp_cursor_shape_v1` itself derives from.

/// A named pointer shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CursorShape {
    /// The ordinary pointer.
    #[default]
    Arrow,
    /// Text may be selected or entered.
    Text,
    /// Precision work.
    Crosshair,
    /// An interactive element, e.g. a link.
    PointingHand,
    /// The hovered thing can be grabbed.
    Grab,
    /// A grab is in progress.
    Grabbing,
    /// Something may be moved.
    Move,
    /// The action will not be carried out.
    NotAllowed,
    /// A drop here copies.
    Copy,
    /// A drop here creates a shortcut.
    Alias,
    /// The dragged item cannot be dropped here.
    NoDrop,
    /// Resize left/right.
    ResizeHorizontal,
    /// Resize up/down.
    ResizeVertical,
    /// Resize along `\`.
    ResizeDiagonalDownRight,
    /// Resize along `/`.
    ResizeDiagonalUpRight,
    /// A column may be resized.
    ResizeColumn,
    /// A row may be resized.
    ResizeRow,
    /// Pan in any direction.
    AllScroll,
    /// Zoom in.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
}
