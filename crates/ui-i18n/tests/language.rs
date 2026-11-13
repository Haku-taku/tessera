//! The language service: auto resolves the system list, and a switch replaces the text.

use ui_i18n::{Language, LanguageChoice};
use unic_langid::langid;

const EN: &str = "greeting = Hello";
const ZH: &str = "greeting = 你好";

#[test]
fn a_switch_changes_the_text_and_bumps_the_revision() {
    let system = vec!["zh-CN".to_string()];
    let language =
        Language::from_sources(LanguageChoice::Auto, &system, &[("en", EN), ("zh-CN", ZH)])
            .expect("a language");

    assert_eq!(&*language.catalog().text_by_id("greeting"), "你好");
    assert_eq!(language.resolved(), langid!("zh-CN"));
    assert_eq!(language.revision(), 0);

    language
        .set_choice(LanguageChoice::Fixed(langid!("en")))
        .expect("English is shipped");
    assert_eq!(&*language.catalog().text_by_id("greeting"), "Hello");
    assert_eq!(language.revision(), 1);

    // Asking for a language nothing ships resolves to the fallback rather than failing.
    language
        .set_choice(LanguageChoice::Fixed(langid!("fr")))
        .expect("a catalog still builds");
    assert_eq!(language.resolved(), langid!("en"));
}
