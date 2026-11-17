//! The marks the sidebar and the rows draw: a glyph per place, and the fallback per file.

use ui_i18n::Msg;
use ui_platform::places::PlaceKind;
use ui_platform::FileKind;
use ui_widgets::{IconSet, IconSource, file_icon};

/// The framework's own mark for a file type, when neither a thumbnail nor a type icon exists.
pub(crate) fn file_mark(kind: FileKind) -> IconSource {
    file_icon(kind)
}

/// A glyph for a place. The names are Material Symbols', so any pack that ships that face
/// draws them; a pack without it still has a glyph slot, just no picture in it.
pub(crate) fn place_mark(kind: PlaceKind) -> IconSource {
    IconSource::glyph(
        match kind {
            PlaceKind::Home => "home",
            PlaceKind::Desktop => "desktop_windows",
            PlaceKind::Documents => "description",
            PlaceKind::Downloads => "download",
            PlaceKind::Music => "music_note",
            PlaceKind::Pictures => "image",
            PlaceKind::Videos => "movie",
            PlaceKind::Templates => "folder_open",
            PlaceKind::PublicShare => "public",
        },
        IconSet::Outlined,
    )
}

/// A glyph for a mounted volume: the sidebar does not draw the platform's drive icon,
/// because a pack without one still has a glyph slot.
pub(crate) fn device_mark() -> IconSource {
    IconSource::glyph("hard_drive", IconSet::Outlined)
}

/// The chevron beside a crumb: along the trail, or down while its menu is open.
///
/// The reference turns one glyph; the engine paints no rotated text or icon, so the pair is
/// Material's own rotated shape rather than a transform.
pub(crate) fn crumb_arrow(open: bool) -> IconSource {
    IconSource::glyph(
        match open {
            true => "expand_more",
            false => "chevron_right",
        },
        IconSet::Outlined,
    )
}

/// A mark for a menu's quick-action strip: the same glyph the rest of the picker draws,
/// carried in the form a menu cell shapes.
pub(crate) fn menu_mark(name: &str) -> ui_core::menu::MenuIcon {
    IconSource::menu_glyph(name, IconSet::Outlined)
}

/// The sidebar's word for a place.
pub(crate) fn place_label(kind: PlaceKind) -> Msg {
    match kind {
        PlaceKind::Home => Msg::PICKER_PLACE_HOME,
        PlaceKind::Desktop => Msg::PICKER_PLACE_DESKTOP,
        PlaceKind::Documents => Msg::PICKER_PLACE_DOCUMENTS,
        PlaceKind::Downloads => Msg::PICKER_PLACE_DOWNLOADS,
        PlaceKind::Music => Msg::PICKER_PLACE_MUSIC,
        PlaceKind::Pictures => Msg::PICKER_PLACE_PICTURES,
        PlaceKind::Videos => Msg::PICKER_PLACE_VIDEOS,
        PlaceKind::Templates => Msg::PICKER_PLACE_TEMPLATES,
        PlaceKind::PublicShare => Msg::PICKER_PLACE_PUBLIC,
    }
}
