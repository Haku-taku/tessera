//! Syntax highlighting over tree-sitter: byte ranges with a class, and no colour at all.
//!
//! The document and the layout never see a theme; a caller maps [`TokenClass`] onto ink,
//! weight or italic, which is why this crate has no design tokens in it.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

/// A language this build can highlight.
///
/// Each grammar is a C library of its own; adding one means adding its dependency and a
/// `configuration` arm, not a new trait. A configuration without a grammar is a plain text
/// fence, which is what an unlisted language falls back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Rust,
    Json,
}

impl Language {
    /// The `Language` a fence's info string names, if this build has that grammar.
    ///
    /// An info string commonly carries more than the name (`rust,no_run`), so the first
    /// comma-separated part is the one matched.
    pub fn from_info(info: &str) -> Option<Self> {
        let name = info.split(',').next()?.trim();
        match name.to_ascii_lowercase().as_str() {
            "rust" | "rs" => Some(Self::Rust),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

/// What a token is, in the terms a theme colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenClass {
    Attribute,
    Comment,
    Constant,
    Function,
    Keyword,
    Label,
    Number,
    Operator,
    Property,
    Punctuation,
    String,
    Type,
    Variable,
}

/// One highlighted range of the source, in UTF-8 bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub range: Range<usize>,
    pub class: TokenClass,
}

/// The capture names this crate recognises, with the class each one means.
///
/// tree-sitter matches a capture to the longest recognised name whose parts it contains,
/// so `function.method` lands on `function` and an unknown capture is simply unstyled.
const CAPTURES: &[(&str, TokenClass)] = &[
    ("attribute", TokenClass::Attribute),
    ("comment", TokenClass::Comment),
    ("constant", TokenClass::Constant),
    ("function", TokenClass::Function),
    ("keyword", TokenClass::Keyword),
    ("label", TokenClass::Label),
    ("number", TokenClass::Number),
    ("operator", TokenClass::Operator),
    ("property", TokenClass::Property),
    ("punctuation", TokenClass::Punctuation),
    ("string", TokenClass::String),
    ("type", TokenClass::Type),
    ("variable", TokenClass::Variable),
];

/// Why highlighting produced nothing: the grammar's own query did not compile.
#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "highlight query did not compile: {}", self.0)
    }
}

impl std::error::Error for Error {}

/// How many recent parses are kept. `CACHE_LIMIT`, not a count of blocks: a page rebuild asks
/// for the same blocks again, and a tween rebuilds once per frame.
const CACHE_LIMIT: usize = 16;

thread_local! {
    /// `(language, source, tokens)`, most recent first. Thread-local because the shared value
    /// is the parse result, while the `Highlighter` that produced it is not `Sync`.
    static PARSED: RefCell<VecDeque<(Language, String, Arc<[Token]>)>> =
        RefCell::new(VecDeque::new());
}

/// The tokens of `source`, in source order and non-overlapping.
///
/// A range the grammar did not capture is absent rather than classed `Other`: the caller
/// paints it in the base ink, and an unstyled gap costs nothing to merge.
///
/// Repeat calls with the same language and source answer from the last parse; the `Arc` makes
/// that sharing visible, and a caller that mutates nothing can hold it across frames.
pub fn tokens(language: Language, source: &str) -> Result<Arc<[Token]>, Error> {
    let cached = PARSED.with(|cache| {
        let mut cache = cache.borrow_mut();
        let index = cache
            .iter()
            .position(|(lang, text, _)| *lang == language && text == source)?;
        let entry = cache.remove(index)?;
        let shared = Arc::clone(&entry.2);
        cache.push_front(entry);
        Some(shared)
    });
    if let Some(shared) = cached {
        return Ok(shared);
    }
    let parsed: Arc<[Token]> = parse(language, source)?.into();
    PARSED.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.push_front((language, source.to_owned(), Arc::clone(&parsed)));
        cache.truncate(CACHE_LIMIT);
    });
    Ok(parsed)
}

/// Tokenise from scratch: create a highlighter, run the grammar, fold the capture stack.
fn parse(language: Language, source: &str) -> Result<Vec<Token>, Error> {
    let config = configuration(language)?;
    let mut highlighter = Highlighter::new();
    let events = highlighter
        .highlight(config, source.as_bytes(), None, None, |_| None)
        .map_err(|error| Error(error.to_string()))?;
    let mut out = Vec::new();
    // Captures nest; a source run inside one takes the innermost class.
    let mut stack: Vec<Option<TokenClass>> = Vec::new();
    for event in events {
        match event.map_err(|error| Error(error.to_string()))? {
            HighlightEvent::HighlightStart(highlight) => {
                stack.push(CAPTURES.get(highlight.0).map(|(_, class)| *class));
            }
            HighlightEvent::HighlightEnd => {
                stack.pop();
            }
            HighlightEvent::Source { start, end } => {
                if let Some(Some(class)) = stack.last()
                    && start < end
                {
                    out.push(Token {
                        range: start..end,
                        class: *class,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// Parse and configure the grammar once per language, then share it.
///
/// The query compile is the expensive part, so each language keeps its own `OnceLock`; a
/// failure is memoised too, rather than retried on every fence.
fn configuration(language: Language) -> Result<&'static HighlightConfiguration, Error> {
    static RUST: OnceLock<Result<HighlightConfiguration, Error>> = OnceLock::new();
    static JSON: OnceLock<Result<HighlightConfiguration, Error>> = OnceLock::new();
    let built = match language {
        Language::Rust => RUST.get_or_init(|| {
            build(
                tree_sitter_rust::LANGUAGE.into(),
                "rust",
                tree_sitter_rust::HIGHLIGHTS_QUERY,
                tree_sitter_rust::INJECTIONS_QUERY,
                // This grammar ships no locals query; injections carry the rest.
                "",
            )
        }),
        Language::Json => JSON.get_or_init(|| {
            build(
                tree_sitter_json::LANGUAGE.into(),
                "json",
                tree_sitter_json::HIGHLIGHTS_QUERY,
                // Nothing to inject into a JSON document, and no locals query either.
                "",
                "",
            )
        }),
    };
    built.as_ref().map_err(|error| Error(error.0.clone()))
}

/// Compile one grammar's queries and keep only the captures this crate classes.
fn build(
    language: tree_sitter::Language,
    name: &str,
    highlights: &str,
    injections: &str,
    locals: &str,
) -> Result<HighlightConfiguration, Error> {
    let mut config = HighlightConfiguration::new(language, name, highlights, injections, locals)
        .map_err(|error| Error(error.to_string()))?;
    let names: Vec<&str> = CAPTURES.iter().map(|(name, _)| *name).collect();
    config.configure(&names);
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ranges index the source itself, in order and without overlap, so a run can be
    /// cut from them without a second copy of the text.
    #[test]
    fn tokens_cover_the_source_in_order() {
        let source = "// note\nfn main() { let s = \"hi\"; }\n";
        let tokens = tokens(Language::Rust, source).expect("the rust query compiles");
        assert!(!tokens.is_empty(), "a Rust snippet has tokens");
        let mut last = 0;
        for token in tokens.iter() {
            assert!(token.range.start >= last, "tokens come in source order");
            assert!(
                token.range.end <= source.len(),
                "and stay inside the source"
            );
            last = token.range.end;
        }
        let text = |class: TokenClass| {
            tokens
                .iter()
                .find(|token| token.class == class)
                .map(|token| &source[token.range.clone()])
        };
        assert_eq!(text(TokenClass::Comment), Some("// note"));
        assert_eq!(text(TokenClass::Keyword).map(str::trim), Some("fn"));
        assert_eq!(text(TokenClass::String), Some("\"hi\""));
    }

    #[test]
    fn a_fences_info_string_names_the_grammar_or_nothing() {
        // The first comma-separated part is the language; the rest are attributes.
        assert_eq!(Language::from_info("rust"), Some(Language::Rust));
        assert_eq!(Language::from_info("rs,no_run"), Some(Language::Rust));
        assert_eq!(Language::from_info(" JSON "), Some(Language::Json));
        assert_eq!(Language::from_info("python"), None);
        assert_eq!(Language::from_info(""), None);
    }

    #[test]
    fn a_json_document_is_classed_in_its_own_grammar() {
        let source = "{\"name\": \"crab\", \"count\": 2}\n";
        let tokens = tokens(Language::Json, source).expect("the json query compiles");
        let text = |class: TokenClass| {
            tokens
                .iter()
                .filter(|token| token.class == class)
                .map(|token| source[token.range.clone()].to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            text(TokenClass::String),
            vec!["\"name\"", "\"crab\"", "\"count\""],
            "keys and values are both strings; this grammar captures no property"
        );
        assert_eq!(text(TokenClass::Number), vec!["2"]);
        // This grammar captures no punctuation, so braces and commas stay in the base ink.
        assert!(text(TokenClass::Punctuation).is_empty());
    }

    #[test]
    fn a_second_call_for_the_same_source_shares_the_first_parse() {
        // The cache exists for rebuilds at frame rate: a theme tween invalidates every
        // declaration, and each fenced block would otherwise be re-parsed every frame.
        let source = "fn cached() { let _ = 1; }\n";
        let first = tokens(Language::Rust, source).expect("the rust query compiles");
        let second = tokens(Language::Rust, source).expect("the rust query compiles");
        assert!(
            Arc::ptr_eq(&first, &second),
            "the same source is parsed once"
        );
        let other = tokens(Language::Rust, "fn other() {}\n").expect("the rust query compiles");
        assert!(
            !Arc::ptr_eq(&first, &other),
            "a different source is a different parse"
        );
    }
}
