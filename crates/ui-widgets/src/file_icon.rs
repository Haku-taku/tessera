//! The framework's own mark for a file type: the layer under a platform icon and a
//! thumbnail, and the answer when neither exists.
//!
//! It is a Material Symbols glyph rather than a bundled image, so it inherits the ink, the
//! theme and the scale of every other icon, and costs nothing to ship.

use ui_platform::FileKind;

use crate::icon::{IconSet, IconSource};

/// The glyph for a kind. An app that has a platform icon (or a thumbnail) draws that
/// instead; this is what remains when the platform has neither, and on every platform.
pub fn file_icon(kind: FileKind) -> IconSource {
    IconSource::glyph(name(kind), IconSet::Outlined)
}

fn name(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Folder => "folder",
        FileKind::Drive => "hard_drive",
        FileKind::Document => "description",
        FileKind::Spreadsheet => "table_chart",
        FileKind::Presentation => "slideshow",
        FileKind::Pdf => "picture_as_pdf",
        FileKind::Text => "text_snippet",
        FileKind::Code => "code",
        FileKind::Image => "image",
        FileKind::Video => "movie",
        FileKind::Audio => "audio_file",
        FileKind::Archive => "folder_zip",
        FileKind::Generic => "insert_drive_file",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `IconSource::glyph` panics on a name the generated table lacks; this names the
    /// missing one instead, because the table is regenerated from the font.
    #[test]
    fn every_kind_maps_to_a_glyph_in_the_table() {
        let kinds = [
            FileKind::Folder,
            FileKind::Drive,
            FileKind::Document,
            FileKind::Spreadsheet,
            FileKind::Presentation,
            FileKind::Pdf,
            FileKind::Text,
            FileKind::Code,
            FileKind::Image,
            FileKind::Video,
            FileKind::Audio,
            FileKind::Archive,
            FileKind::Generic,
        ];
        for kind in kinds {
            let wanted = name(kind);
            assert!(
                crate::icon_codepoints::ICONS
                    .binary_search_by_key(&wanted, |(name, _)| *name)
                    .is_ok(),
                "{kind:?} wants `{wanted}`, which the generated table does not carry"
            );
        }
    }
}
