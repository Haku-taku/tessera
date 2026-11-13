//! The user's language choice, and the catalog it resolves to.
//!
//! A `Catalog` is one answer for one language. This module holds the question: the
//! application's choice, the platform's language list, and the catalog the two
//! resolve to. It is the object a window keeps so that a later choice reaches it.

use std::fmt;
use std::sync::{Arc, RwLock};

use unic_langid::langid;

use crate::LanguageIdentifier;
use crate::catalog::{Catalog, LoadError};

/// How an application picks its language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanguageChoice {
    /// Follow the platform's preferred languages.
    Auto,
    /// Show this language, whatever the platform prefers.
    Fixed(LanguageIdentifier),
}

impl LanguageChoice {
    /// Read `auto` or a BCP-47 tag, as the config file and `--lang` spell them.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text == "auto" {
            return Ok(Self::Auto);
        }
        text.parse()
            .map(Self::Fixed)
            .map_err(|_| format!("not a language tag: {text}"))
    }
}

impl fmt::Display for LanguageChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => formatter.write_str("auto"),
            Self::Fixed(locale) => write!(formatter, "{locale}"),
        }
    }
}

/// The language an application shows, and the catalog that holds its text.
///
/// The catalog is rebuilt when the choice changes, so a window that keeps this
/// object follows a switch on its next build without rebuilding its `Env`.
pub struct Language {
    sources: Vec<(String, String)>,
    /// The platform's answer, in the order it gave it.
    system: Vec<String>,
    fallback: LanguageIdentifier,
    state: RwLock<State>,
}

struct State {
    choice: LanguageChoice,
    catalog: Arc<Catalog>,
    revision: u64,
}

impl Language {
    /// Build from the locales embedded in `ui-i18n`.
    pub fn builtin(choice: LanguageChoice, system: &[String]) -> Result<Arc<Language>, LoadError> {
        let sources: Vec<(String, String)> = crate::LOCALES
            .iter()
            .map(|files| (files.tag.to_string(), files.source.to_string()))
            .collect();
        Self::with_sources(choice, system, sources)
    }

    /// Build from the application's own FTL sources, keyed by locale tag.
    ///
    /// The tags are the languages the application ships; a choice that names any
    /// other language resolves to the fallback.
    pub fn from_sources(
        choice: LanguageChoice,
        system: &[String],
        sources: &[(&str, &str)],
    ) -> Result<Arc<Language>, LoadError> {
        let sources = sources
            .iter()
            .map(|(tag, source)| ((*tag).to_string(), (*source).to_string()))
            .collect();
        Self::with_sources(choice, system, sources)
    }

    fn with_sources(
        choice: LanguageChoice,
        system: &[String],
        sources: Vec<(String, String)>,
    ) -> Result<Arc<Language>, LoadError> {
        let fallback = langid!("en");
        let catalog = Arc::new(resolve(&choice, system, &fallback, &sources)?);
        Ok(Arc::new(Self {
            sources,
            system: system.to_vec(),
            fallback,
            state: RwLock::new(State {
                choice,
                catalog,
                revision: 0,
            }),
        }))
    }

    /// The choice in force.
    pub fn choice(&self) -> LanguageChoice {
        self.read().choice.clone()
    }

    /// The catalog for that choice.
    pub fn catalog(&self) -> Arc<Catalog> {
        Arc::clone(&self.read().catalog)
    }

    /// The language the catalog holds, after negotiation.
    pub fn resolved(&self) -> LanguageIdentifier {
        self.read().catalog.locale().clone()
    }

    /// Moves whenever the catalog is replaced, so a host can tell a switch happened.
    pub fn revision(&self) -> u64 {
        self.read().revision
    }

    /// Show a different language. The catalog is replaced on success; on failure the
    /// old one stays and the error says why the new one could not be built.
    pub fn set_choice(&self, choice: LanguageChoice) -> Result<(), LoadError> {
        let catalog = Arc::new(resolve(
            &choice,
            &self.system,
            &self.fallback,
            &self.sources,
        )?);
        let mut state = self.write();
        state.choice = choice;
        state.catalog = catalog;
        state.revision += 1;
        Ok(())
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, State> {
        match self.state.read() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, State> {
        match self.state.write() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Negotiate `choice` against the shipped sources.
fn resolve(
    choice: &LanguageChoice,
    system: &[String],
    fallback: &LanguageIdentifier,
    sources: &[(String, String)],
) -> Result<Catalog, LoadError> {
    let mut preferences: Vec<LanguageIdentifier> = match choice {
        LanguageChoice::Auto => system.iter().filter_map(|tag| tag.parse().ok()).collect(),
        LanguageChoice::Fixed(locale) => vec![locale.clone()],
    };
    // The chain ends in English whether or not the application asked for it: a key
    // the chosen language does not translate must still read as words.
    preferences.push(fallback.clone());
    let sources: Vec<(&str, &str)> = sources
        .iter()
        .map(|(tag, source)| (tag.as_str(), source.as_str()))
        .collect();
    Catalog::from_sources(&preferences, fallback, &sources)
}

/// The endonym of every shipped language, most natural for a picker.
///
/// A locale without a `language-name` message fails the build, so this is as complete
/// as `LOCALES`.
pub fn language_names() -> impl Iterator<Item = (&'static str, &'static str)> {
    crate::LANGUAGE_NAMES.iter().copied()
}
