//! Mapping the framework's neutral shape names onto `wp_cursor_shape_v1`.

use ui_core::CursorShape;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape;

/// The protocol value for a shape. Total, so a new variant cannot silently mean `Default`.
pub(crate) fn protocol_shape(shape: CursorShape) -> Shape {
    match shape {
        CursorShape::Arrow => Shape::Default,
        CursorShape::Text => Shape::Text,
        CursorShape::Crosshair => Shape::Crosshair,
        CursorShape::PointingHand => Shape::Pointer,
        CursorShape::Grab => Shape::Grab,
        CursorShape::Grabbing => Shape::Grabbing,
        CursorShape::Move => Shape::Move,
        CursorShape::NotAllowed => Shape::NotAllowed,
        CursorShape::Copy => Shape::Copy,
        CursorShape::Alias => Shape::Alias,
        CursorShape::NoDrop => Shape::NoDrop,
        CursorShape::ResizeHorizontal => Shape::EwResize,
        CursorShape::ResizeVertical => Shape::NsResize,
        CursorShape::ResizeDiagonalDownRight => Shape::NwseResize,
        CursorShape::ResizeDiagonalUpRight => Shape::NeswResize,
        CursorShape::ResizeColumn => Shape::ColResize,
        CursorShape::ResizeRow => Shape::RowResize,
        CursorShape::AllScroll => Shape::AllScroll,
        CursorShape::ZoomIn => Shape::ZoomIn,
        CursorShape::ZoomOut => Shape::ZoomOut,
    }
}
