//! Font fixtures for tests, resolved through the platform provider.
//!
//! `fc-match` is a Linux tool; using the provider keeps the same tests meaningful
//! on DirectWrite and CoreText.

use ui_text::{FontProvider, FontRequest, GenericFamily, Script};

/// A resolvable sans face.
pub(crate) fn sans_face() -> Option<ui_text::FontBlob> {
    let provider = crate::system_or_isolated();
    let face = provider.resolve(&FontRequest::new(GenericFamily::SansSerif))?;
    provider.face_data(face)
}

/// A face that can render right-to-left script.
pub(crate) fn rtl_face() -> Option<ui_text::FontBlob> {
    let provider = crate::system_or_isolated();
    let face = *provider.fallback_chain(Script::ARABIC).first()?;
    provider.face_data(face)
}

/// Any installed colour emoji face, found by family name rather than a fixed name.
pub(crate) fn emoji_face() -> Option<ui_text::FontBlob> {
    let provider = crate::system_or_isolated();
    let name = provider
        .family_names()
        .into_iter()
        .find(|name| name.to_ascii_lowercase().contains("emoji"))?;
    let face = provider.resolve(&FontRequest::new(GenericFamily::SansSerif).family(name))?;
    provider.face_data(face)
}
