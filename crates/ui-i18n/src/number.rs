//! Locale-aware number display.
//!
//! Fluent writes a [`FluentValue::Number`] with the float's own formatting unless the bundle
//! installs a formatter. This one renders the same value through ICU4X's CLDR data, which
//! supplies the locale's digits, separators and grouping positions.

use std::convert::Infallible;
use std::str::FromStr;

use fluent_bundle::FluentValue;
use fluent_bundle::memoizer::MemoizerKind;
use fluent_bundle::types::FluentNumber;
use icu_decimal::DecimalFormatter;
use icu_decimal::input::Decimal;
use icu_decimal::options::GroupingStrategy;
use intl_memoizer::Memoizable;
use intl_memoizer::concurrent::IntlLangMemoizer;
use unic_langid::LanguageIdentifier;

/// Built once per locale and grouping choice, then kept by the bundle's memoizer for its life.
enum Numbers {
    Ready(DecimalFormatter),
    /// No CLDR data for the locale. `construct` logged it; numbers then fall through to
    /// Fluent's own rendering, so the message still reads rather than vanishing.
    Unavailable,
}

impl Memoizable for Numbers {
    type Args = (GroupingStrategy,);
    type Error = Infallible;

    fn construct(locale: LanguageIdentifier, (grouping,): Self::Args) -> Result<Self, Self::Error> {
        let Some(icu_locale) = crate::catalog::icu_locale(&locale) else {
            return Ok(Self::Unavailable);
        };
        match DecimalFormatter::try_new(icu_locale.into(), grouping.into()) {
            Ok(formatter) => Ok(Self::Ready(formatter)),
            Err(error) => {
                log::warn!("ui-i18n: no number data for {locale}: {error}");
                Ok(Self::Unavailable)
            }
        }
    }
}

/// Same digits Fluent itself would print, so only their presentation is the locale's.
fn render(formatter: &DecimalFormatter, number: &FluentNumber) -> Option<String> {
    let text = number.value.to_string();
    let mut decimal = Decimal::from_str(&text).ok()?;
    if let Some(digits) = number.options.minimum_fraction_digits {
        let digits = i16::try_from(digits).unwrap_or(i16::MAX);
        decimal.absolute.pad_end(-digits);
    }
    Some(formatter.format_to_string(&decimal))
}

/// The bundle's formatter hook: `None` hands the value back to Fluent.
pub(crate) fn format(value: &FluentValue<'_>, intls: &IntlLangMemoizer) -> Option<String> {
    let FluentValue::Number(number) = value else {
        return None;
    };
    let grouping = if number.options.use_grouping {
        GroupingStrategy::Auto
    } else {
        GroupingStrategy::Never
    };
    intls
        .with_try_get_threadsafe::<Numbers, _, _>((grouping,), |numbers| match numbers {
            Numbers::Ready(formatter) => render(formatter, number),
            Numbers::Unavailable => None,
        })
        .ok()
        .flatten()
}
