//! The language list the session's environment names.
//!
//! POSIX has no locale query an application can call; the convention is a set of
//! environment variables, and GNU gettext defines how they combine. This module is
//! that convention, kept in the platform crate because it is the platform speaking.

/// The language tags the environment prefers, most preferred first.
///
/// Empty when the user asked for no translation, and on a session that states no
/// preference at all.
pub fn posix_system_languages() -> Vec<String> {
    languages_from(|name| std::env::var(name).ok())
}

/// The same list, with the environment supplied as `lookup` so a test can drive it.
///
/// `LANGUAGE` is a colon-separated list and wins when the message category is
/// translatable; `LC_ALL`, `LC_MESSAGES` and `LANG` follow it. A category of `C` or
/// `POSIX` means no translation, and gettext then ignores `LANGUAGE` too.
pub(crate) fn languages_from(lookup: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let category = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| lookup(name))
        .find(|value| !value.trim().is_empty());
    if category.is_some_and(|value| is_c_locale(&value)) {
        return Vec::new();
    }
    let mut tags: Vec<String> = Vec::new();
    if let Some(list) = lookup("LANGUAGE").filter(|value| !value.trim().is_empty()) {
        tags.extend(list.split(':').filter_map(normalize));
    }
    for name in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(value) = lookup(name) {
            tags.extend(normalize(&value));
        }
    }
    // Stable dedup: the same tag often arrives from both `LANGUAGE` and `LANG`.
    let mut unique = Vec::with_capacity(tags.len());
    for tag in tags {
        if !unique.contains(&tag) {
            unique.push(tag);
        }
    }
    unique
}

/// `zh_CN.UTF-8@euro` as a language tag: the codeset and modifier are not part of it.
fn normalize(value: &str) -> Option<String> {
    let base = base(value);
    if base.is_empty() || matches!(base.as_str(), "C" | "POSIX") {
        return None;
    }
    Some(base.replace('_', "-"))
}

fn is_c_locale(value: &str) -> bool {
    matches!(base(value).as_str(), "C" | "POSIX")
}

fn base(value: &str) -> String {
    let value = value.split('.').next().unwrap_or_default();
    let value = value.split('@').next().unwrap_or_default();
    value.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::languages_from;

    fn languages(pairs: &[(&str, &str)]) -> Vec<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        languages_from(|name| {
            pairs
                .iter()
                .find(|(known, _)| known == name)
                .map(|(_, value)| value.clone())
        })
    }

    #[test]
    fn the_gettext_order_and_the_c_locale() {
        assert_eq!(
            languages(&[("LANGUAGE", "ja:ko"), ("LANG", "zh_CN.UTF-8")]),
            ["ja", "ko", "zh-CN"]
        );
        // A C category means no translation; `LANGUAGE` does not override that.
        assert!(languages(&[("LANGUAGE", "de:fr"), ("LC_ALL", "C.UTF-8")]).is_empty());
        assert!(languages(&[("LANG", "POSIX")]).is_empty());
        assert!(languages(&[]).is_empty());
    }
}
