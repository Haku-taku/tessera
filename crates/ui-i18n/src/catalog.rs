//! Locale bundles, the explicit fallback chain, and preformatted static messages.

use std::collections::HashSet;
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use icu_calendar::{Date, Gregorian};
use icu_datetime::FixedCalendarDateTimeFormatter;
use icu_datetime::fieldsets::YMD;
use unic_langid::LanguageIdentifier;

use crate::{ALL, Args, LOCALES, Msg, MsgSpec};

/// A loaded catalog: one bundle per locale, in fallback order.
pub struct Catalog {
    /// The chain's head: the language that supplies the text.
    locale: LanguageIdentifier,
    chain: Vec<LocaleBundle>,
    /// Messages already logged, so a missing key does not warn on every frame.
    warned: Mutex<HashSet<(usize, String)>>,
}

struct LocaleBundle {
    locale: LanguageIdentifier,
    bundle: FluentBundle<FluentResource>,
    /// CLDR's medium date pattern for this locale; `None` when it has no date data.
    /// The calendar is fixed to Gregorian: the app's `Date` is a Gregorian civil date,
    /// so a locale whose calendar differs still names the year its picker shows.
    dates: Option<FixedCalendarDateTimeFormatter<Gregorian, YMD>>,
    /// Preformatted no-argument messages, indexed by `Msg`.
    static_text: Vec<Option<Arc<str>>>,
}

/// ICU's own locale type, for the APIs that do not take a `unic-langid` identifier.
pub(crate) fn icu_locale(locale: &LanguageIdentifier) -> Option<icu_locale_core::Locale> {
    match icu_locale_core::Locale::try_from_str(&locale.to_string()) {
        Ok(locale) => Some(locale),
        Err(_) => {
            log::warn!("ui-i18n: {locale} is not an ICU locale");
            None
        }
    }
}

impl Catalog {
    /// Build from the locales embedded in this crate.
    ///
    /// `preferences` is the user's ordered wish, most wanted first; the shipped
    /// locales are negotiated against it and `fallback` closes the chain.
    pub fn builtin(
        preferences: &[LanguageIdentifier],
        fallback: &LanguageIdentifier,
    ) -> Result<Catalog, LoadError> {
        let mut sources = Vec::with_capacity(LOCALES.len());
        for files in LOCALES {
            let locale: LanguageIdentifier = files
                .tag
                .parse()
                .map_err(|_| LoadError::BadLocale(files.tag.to_string()))?;
            sources.push((locale, files.source));
        }
        build(preferences, fallback, &sources)
    }

    /// Build from arbitrary FTL sources, keyed by locale tag; tests and apps use this.
    ///
    /// The tags are the languages this application ships: negotiation can only pick
    /// one of them, so a preference with no source never becomes the chain's head.
    pub fn from_sources(
        preferences: &[LanguageIdentifier],
        fallback: &LanguageIdentifier,
        sources: &[(&str, &str)],
    ) -> Result<Catalog, LoadError> {
        let mut parsed = Vec::with_capacity(sources.len());
        for (tag, source) in sources {
            let locale: LanguageIdentifier = tag
                .parse()
                .map_err(|_| LoadError::BadLocale((*tag).to_string()))?;
            parsed.push((locale, *source));
        }
        build(preferences, fallback, &parsed)
    }

    /// The language the chain starts with: the one that supplied the text.
    pub fn locale(&self) -> &LanguageIdentifier {
        &self.locale
    }

    /// The read-only English catalog used when an `Env` carries none.
    pub fn english() -> &'static Catalog {
        static ENGLISH: OnceLock<Catalog> = OnceLock::new();
        ENGLISH.get_or_init(|| {
            let en: LanguageIdentifier = "en".parse().expect("en is a locale");
            Catalog::builtin(std::slice::from_ref(&en), &en).expect("built-in English catalog")
        })
    }

    /// A no-argument message: one array index and one refcount bump.
    pub fn text(&self, msg: Msg) -> Arc<str> {
        let index = msg.index();
        let spec = msg.spec();
        for locale in &self.chain {
            if let Some(Some(text)) = locale.static_text.get(index) {
                return text.clone();
            }
            if locale.contains(spec) {
                self.warn_once(index, &locale.locale);
            }
        }
        self.missing(index, spec)
    }

    /// A parameterized message: format on demand, falling through locales on error.
    pub fn text_with(&self, msg: Msg, args: &Args<'_>) -> Arc<str> {
        let spec = msg.spec();
        for locale in &self.chain {
            if !locale.contains(spec) {
                continue;
            }
            if let Some(text) = format_in(&locale.bundle, spec, Some(args.inner())) {
                return text;
            }
            self.warn_once(msg.index(), &locale.locale);
        }
        self.missing(msg.index(), spec)
    }

    /// A civil date (year, month 1–12, day) at CLDR's medium length in the first locale
    /// the chain resolved to, or `None` when no such date exists.
    pub fn date(&self, year: i32, month: u8, day: u8) -> Option<Arc<str>> {
        let date = Date::try_new_gregorian(year, month, day).ok()?;
        for locale in &self.chain {
            if let Some(dates) = &locale.dates {
                return Some(Arc::from(dates.format(&date).to_string()));
            }
        }
        // No locale in the chain has date data; an ISO date still names the day.
        Some(Arc::from(format!("{year:04}-{month:02}-{day:02}")))
    }

    /// The app's own key; built-in keys use `Msg`.
    pub fn text_by_id(&self, id: &str) -> Arc<str> {
        self.by_id(id, None)
    }

    /// The same, with arguments.
    pub fn text_by_id_with(&self, id: &str, args: &Args<'_>) -> Arc<str> {
        self.by_id(id, Some(args.inner()))
    }

    /// Whether any locale in the chain defines this message.
    pub fn has(&self, msg: Msg) -> bool {
        self.chain.iter().any(|locale| locale.contains(msg.spec()))
    }

    /// Whether any locale in the chain defines this app-level key.
    ///
    /// A caller that fills text for a whole tree, such as the command line's help, asks
    /// first: a key nobody defines must stay unset rather than become its own placeholder.
    pub fn has_id(&self, id: &str) -> bool {
        let (message_id, attr) = match id.split_once('.') {
            Some((message_id, attr)) => (message_id, Some(attr)),
            None => (id, None),
        };
        self.chain.iter().any(|locale| {
            locale
                .bundle
                .get_message(message_id)
                .is_some_and(|message| {
                    attr.is_none_or(|attr| message.get_attribute(attr).is_some())
                })
        })
    }

    fn by_id(&self, id: &str, args: Option<&FluentArgs<'_>>) -> Arc<str> {
        for locale in &self.chain {
            if let Some(text) = format_by_id(&locale.bundle, id, args) {
                return text;
            }
        }
        let key = (usize::MAX, id.to_string());
        if self.remember(key) {
            log::error!("ui-i18n: no locale provides {id}");
        }
        Arc::from(format!("⟨{id}⟩"))
    }

    fn warn_once(&self, index: usize, locale: &LanguageIdentifier) {
        let key = (index, locale.to_string());
        if self.remember(key) {
            let spec = &ALL[index];
            log::warn!(
                "ui-i18n: {locale} cannot format {} ({:?})",
                spec.id,
                spec.attr
            );
        }
    }

    fn missing(&self, index: usize, spec: &MsgSpec) -> Arc<str> {
        let name = match spec.attr {
            Some(attr) => format!("{}.{}", spec.id, attr),
            None => spec.id.to_string(),
        };
        if self.remember((index, String::from("<missing>"))) {
            log::error!("ui-i18n: no locale provides {name}");
        }
        Arc::from(format!("⟨{name}⟩"))
    }

    fn remember(&self, key: (usize, String)) -> bool {
        let mut warned = match self.warned.lock() {
            Ok(warned) => warned,
            Err(poisoned) => poisoned.into_inner(),
        };
        warned.insert(key)
    }
}

impl LocaleBundle {
    fn load(locale: LanguageIdentifier, source: &str) -> Result<Self, LoadError> {
        let resource = FluentResource::try_new(source.to_string()).map_err(|(_, errors)| {
            LoadError::Parse {
                locale: locale.to_string(),
                errors: format!("{errors:?}"),
            }
        })?;
        let mut bundle = FluentBundle::new_concurrent(vec![locale.clone()]);
        bundle.set_use_isolating(false);
        // Numbers are placeables in almost every message; rendering them with Fluent's
        // bare float formatting would show "1000" where the locale writes "1,000".
        bundle.set_formatter(Some(crate::number::format));
        bundle.add_builtins().map_err(|error| LoadError::Build {
            locale: locale.to_string(),
            errors: format!("{error:?}"),
        })?;
        bundle
            .add_resource(resource)
            .map_err(|errors| LoadError::Build {
                locale: locale.to_string(),
                errors: format!("{errors:?}"),
            })?;
        let dates =
            icu_locale(&locale).and_then(|icu| {
                match FixedCalendarDateTimeFormatter::try_new(icu.into(), YMD::medium()) {
                    Ok(formatter) => Some(formatter),
                    Err(error) => {
                        log::warn!("ui-i18n: no date data for {locale}: {error}");
                        None
                    }
                }
            });
        let static_text = ALL
            .iter()
            .map(|spec| format_in(&bundle, spec, None))
            .collect();
        Ok(Self {
            locale,
            bundle,
            dates,
            static_text,
        })
    }

    fn contains(&self, spec: &MsgSpec) -> bool {
        let Some(message) = self.bundle.get_message(spec.id) else {
            return false;
        };
        match spec.attr {
            Some(attr) => message.get_attribute(attr).is_some(),
            None => message.value().is_some(),
        }
    }
}

fn build(
    preferences: &[LanguageIdentifier],
    fallback: &LanguageIdentifier,
    sources: &[(LanguageIdentifier, &str)],
) -> Result<Catalog, LoadError> {
    let available: Vec<LanguageIdentifier> = sources.iter().map(|(tag, _)| tag.clone()).collect();
    let mut chain = Vec::new();
    for index in negotiate(preferences, &available, fallback) {
        let Some((locale, source)) = sources.get(index) else {
            continue;
        };
        chain.push(LocaleBundle::load(locale.clone(), source)?);
    }
    let Some(first) = chain.first() else {
        let wanted = preferences
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(LoadError::NoLocales(wanted));
    };
    Ok(Catalog {
        locale: first.locale.clone(),
        chain,
        warned: Mutex::new(HashSet::new()),
    })
}

/// The bundle order for `preferences`: indices into `available`, best first.
///
/// `Matching` stops at the first level that matches each requested language, so the
/// order follows the user's wish; the fallback is appended when no match is it.
fn negotiate(
    preferences: &[LanguageIdentifier],
    available: &[LanguageIdentifier],
    fallback: &LanguageIdentifier,
) -> Vec<usize> {
    use fluent_langneg::{NegotiationStrategy, negotiate_languages};

    let mut requested: Vec<fluent_langneg::LanguageIdentifier> =
        Vec::with_capacity(preferences.len());
    for locale in preferences {
        if let Ok(locale) = locale.to_string().parse() {
            requested.push(locale);
        }
    }
    let mut supported: Vec<fluent_langneg::LanguageIdentifier> =
        Vec::with_capacity(available.len());
    let mut origin = Vec::with_capacity(available.len());
    for (index, locale) in available.iter().enumerate() {
        if let Ok(locale) = locale.to_string().parse() {
            supported.push(locale);
            origin.push(index);
        }
    }
    let default: Option<fluent_langneg::LanguageIdentifier> = fallback.to_string().parse().ok();
    let matched = negotiate_languages(
        &requested,
        &supported,
        default.as_ref(),
        NegotiationStrategy::Matching,
    );
    let mut order = Vec::with_capacity(matched.len());
    for locale in matched {
        // The function returns references into `supported`, so its position is the
        // index in `available` the bundle must come from.
        if let Some(index) = supported.iter().position(|known| known == locale) {
            order.push(origin[index]);
        }
    }
    order
}

fn format_in(
    bundle: &FluentBundle<FluentResource>,
    spec: &MsgSpec,
    args: Option<&FluentArgs<'_>>,
) -> Option<Arc<str>> {
    let message = bundle.get_message(spec.id)?;
    let pattern = match spec.attr {
        Some(attr) => message.get_attribute(attr)?.value(),
        None => message.value()?,
    };
    let mut errors = Vec::new();
    let text = bundle.format_pattern(pattern, args, &mut errors);
    errors.is_empty().then(|| Arc::from(text.as_ref()))
}

fn format_by_id(
    bundle: &FluentBundle<FluentResource>,
    id: &str,
    args: Option<&FluentArgs<'_>>,
) -> Option<Arc<str>> {
    let (message_id, attr) = match id.split_once('.') {
        Some((message_id, attr)) => (message_id, Some(attr)),
        None => (id, None),
    };
    let message = bundle.get_message(message_id)?;
    let pattern = match attr {
        Some(attr) => message.get_attribute(attr)?.value(),
        None => message.value()?,
    };
    let mut errors = Vec::new();
    let text = bundle.format_pattern(pattern, args, &mut errors);
    errors.is_empty().then(|| Arc::from(text.as_ref()))
}

/// Why a catalog could not be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// No locale in the chain had an embedded source.
    NoLocales(String),
    /// A locale tag did not parse.
    BadLocale(String),
    /// An FTL source failed to parse.
    Parse { locale: String, errors: String },
    /// Fluent rejected a resource or a builtin.
    Build { locale: String, errors: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::NoLocales(locale) => write!(f, "no locale sources for {locale}"),
            LoadError::BadLocale(tag) => write!(f, "invalid locale tag {tag}"),
            LoadError::Parse { locale, errors } => {
                write!(f, "cannot parse {locale}: {errors}")
            }
            LoadError::Build { locale, errors } => {
                write!(f, "cannot build {locale}: {errors}")
            }
        }
    }
}

impl std::error::Error for LoadError {}
