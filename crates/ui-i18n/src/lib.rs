//! Fluent-backed catalogs for the workspace's built-in strings.
//!
//! The build script turns `locales/en/**/*.ftl` into `Msg` constants, so a missing key is a
//! compile error. No-argument messages are formatted once per locale and indexed by `Msg`.

mod args;
mod catalog;
mod language;
mod number;

include!(concat!(env!("OUT_DIR"), "/msg.rs"));

pub use args::{Args, IntoArg, Value};
pub use catalog::{Catalog, LoadError};
pub use language::{Language, LanguageChoice, language_names};
pub use unic_langid::LanguageIdentifier;
