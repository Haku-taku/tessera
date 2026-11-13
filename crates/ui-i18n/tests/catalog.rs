//! The catalog's three load-bearing contracts: locale parity, arguments, and fallback.

use ui_i18n::{ALL, Args, Catalog, Msg};
use unic_langid::{LanguageIdentifier, langid};

/// Missing translations are the core risk this design guards; every locale needs every key.
#[test]
fn every_locale_defines_every_generated_message() {
    for files in ui_i18n::LOCALES {
        let locale: LanguageIdentifier = files.tag.parse().expect("a locale tag");
        // Only this locale's source: the fallback chain must not hide a missing key.
        let catalog = Catalog::from_sources(
            std::slice::from_ref(&locale),
            &locale,
            &[(files.tag, files.source)],
        )
        .unwrap_or_else(|error| panic!("{}: {error}", files.tag));
        for (index, spec) in ALL.iter().enumerate() {
            assert!(
                catalog.has(Msg::from_index(index)),
                "{} is missing {} ({:?})",
                files.tag,
                spec.id,
                spec.attr
            );
        }
    }
}

/// A parameterized message proves `text_with` reaches Fluent's CLDR plural branches.
#[test]
fn a_plural_message_picks_the_cldr_branch() {
    let en = Catalog::builtin(&[langid!("en")], &langid!("en")).expect("en");
    let zh = Catalog::builtin(&[langid!("zh-CN")], &langid!("en")).expect("zh-CN");
    let mut args = Args::new();

    args.set("count", 1);
    assert_eq!(
        &*en.text_with(Msg::WIDGETS_LIST_SELECTED_COUNT, &args),
        "1 item selected"
    );

    args.set("count", 3);
    assert_eq!(
        &*en.text_with(Msg::WIDGETS_LIST_SELECTED_COUNT, &args),
        "3 items selected"
    );
    assert_eq!(
        &*zh.text_with(Msg::WIDGETS_LIST_SELECTED_COUNT, &args),
        "已选择 3 个项目"
    );
}

const NUMBER_SOURCE: &str = r#"
rows = { $count } rows
weight = { $value } kg
items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
"#;

/// A placeable number is written the way its locale writes numbers, and the plural branch
/// still decides on the number itself rather than on the formatted text.
#[test]
fn a_number_is_written_the_way_its_locale_writes_numbers() {
    let catalog = |tag: &str| {
        let locale: LanguageIdentifier = tag.parse().expect("a locale tag");
        Catalog::from_sources(
            std::slice::from_ref(&locale),
            &locale,
            &[(tag, NUMBER_SOURCE)],
        )
        .expect(tag)
    };
    let mut args = Args::new();

    args.set("count", 1_000);
    assert_eq!(&*catalog("en").text_by_id_with("rows", &args), "1,000 rows");
    // Separators and digits come from the locale's CLDR data, not from `f64`'s own text.
    assert_eq!(&*catalog("de").text_by_id_with("rows", &args), "1.000 rows");
    assert_eq!(&*catalog("bn").text_by_id_with("rows", &args), "১,০০০ rows");

    args.set("value", 1234.5);
    assert_eq!(
        &*catalog("en").text_by_id_with("weight", &args),
        "1,234.5 kg"
    );
    assert_eq!(
        &*catalog("de").text_by_id_with("weight", &args),
        "1.234,5 kg"
    );

    args.set("count", 1);
    assert_eq!(&*catalog("en").text_by_id_with("items", &args), "1 item");
    args.set("count", 1_000);
    assert_eq!(
        &*catalog("en").text_by_id_with("items", &args),
        "1,000 items"
    );
}

/// A civil date is written in the locale's own field order; a day that cannot exist has none.
#[test]
fn a_date_is_written_in_its_locales_order() {
    let en = Catalog::builtin(&[langid!("en")], &langid!("en")).expect("en");
    let zh = Catalog::builtin(&[langid!("zh-CN")], &langid!("en")).expect("zh-CN");

    assert_eq!(&*en.date(2026, 11, 9).expect("a date"), "Nov 9, 2026");
    assert_eq!(&*zh.date(2026, 11, 9).expect("a date"), "2026年11月9日");
    assert!(en.date(2026, 2, 30).is_none());
}

const EN: &str = r#"
widgets-shortcut-ctrl = Ctrl
widgets-shortcut-enter = Enter
"#;
const ZH: &str = r#"
widgets-shortcut-ctrl = 控制
"#;

/// `zh-CN` must reach `zh`, and a key missing there must reach `en`.
#[test]
fn a_region_falls_back_to_its_language_then_to_english() {
    let requested: LanguageIdentifier = "zh-CN".parse().expect("zh-CN");
    let fallback: LanguageIdentifier = "en".parse().expect("en");
    let catalog =
        Catalog::from_sources(&[requested], &fallback, &[("en", EN), ("zh", ZH)]).expect("catalog");

    assert_eq!(&*catalog.text_by_id("widgets-shortcut-ctrl"), "控制");
    assert_eq!(&*catalog.text_by_id("widgets-shortcut-enter"), "Enter");
    assert_eq!(
        &*catalog.text_by_id("__missing-for-test"),
        "⟨__missing-for-test⟩"
    );
}

/// The negotiation levels are the whole point of the dependency; each tag lands where
/// its likely subtags say, and an unshipped language falls back instead of failing.
#[test]
fn negotiation_picks_the_shipped_language_closest_to_the_request() {
    let fallback = langid!("en");
    let shipped = [("en", EN), ("zh-CN", ZH)];
    let resolved = |tag: &str| {
        let wanted: LanguageIdentifier = tag.parse().expect("a tag");
        Catalog::from_sources(&[wanted], &fallback, &shipped)
            .expect("a catalog")
            .locale()
            .clone()
    };

    assert_eq!(resolved("zh-CN"), langid!("zh-CN"));
    assert_eq!(resolved("zh-Hans-CN"), langid!("zh-CN"), "likely subtags");
    assert_eq!(
        resolved("zh-Hant-TW"),
        langid!("zh-CN"),
        "a script this application does not ship keeps the language"
    );
    assert_eq!(resolved("de-AT"), langid!("en"), "unshipped falls back");
    assert_eq!(
        Catalog::from_sources(&[], &fallback, &shipped)
            .expect("a catalog")
            .locale()
            .clone(),
        langid!("en"),
        "no preference is a fallback, not an error"
    );
}
