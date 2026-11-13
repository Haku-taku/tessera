//! The icon face WinUI draws its own glyphs from, and the names these controls ask for.

use ui_core::Family;
use ui_widgets::IconSource;

use crate::fluent_codepoints as table;

/// The family the host registers `FluentSystemIcons-Regular.ttf` under.
pub const ICON_FAMILY: &str = "Fluent System Icons";

/// The codepoint for a glyph name; `None` when the vendored font does not carry it.
pub fn codepoint(name: &str) -> Option<char> {
    table::FLUENT_ICONS
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| table::FLUENT_ICONS[i].1)
}

/// An icon in the Fluent face; `None` rather than a blank box when the name is unknown.
pub fn icon(name: &str) -> Option<IconSource> {
    Some(IconSource::from_parts(
        Family::Named(ICON_FAMILY),
        codepoint(name)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_so_the_lookup_can_bisect() {
        // `codepoint` binary-searches; an unsorted table would miss real glyphs silently.
        assert!(table::FLUENT_ICONS.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
