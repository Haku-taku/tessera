// Glyph sources shared by the navigation and toolbar pages.

use ui_widgets::IconSet;
use ui_widgets::IconSource;

fn glyph(name: &str) -> IconSource {
    IconSource::glyph(name, IconSet::Outlined)
}

/// The iOS glyph for the same destination, from the vendored SF Symbols font.
pub(crate) fn sf_glyph(name: &str) -> IconSource {
    IconSource::sf(name)
}

pub(crate) fn home_glyph() -> IconSource {
    glyph("home")
}

pub(crate) fn add_glyph() -> IconSource {
    glyph("add")
}

pub(crate) fn search_glyph() -> IconSource {
    glyph("search")
}

pub(crate) fn profile_glyph() -> IconSource {
    glyph("person")
}

pub(crate) fn inbox_glyph() -> IconSource {
    glyph("inbox")
}

pub(crate) fn sent_glyph() -> IconSource {
    glyph("send")
}

pub(crate) fn drafts_glyph() -> IconSource {
    glyph("drafts")
}

pub(crate) fn favorite_glyph() -> IconSource {
    glyph("favorite")
}

pub(crate) fn share_glyph() -> IconSource {
    glyph("share")
}

pub(crate) fn tune_glyph() -> IconSource {
    glyph("tune")
}

pub(crate) fn more_glyph() -> IconSource {
    glyph("more_vert")
}
