//! HTML entity decoding for Markdown text.
//!
//! The parser already decodes an entity into the text it lays out; this table turns the
//! same source bytes into those characters for an editor that draws the source itself.

/// Decode one complete entity: `&name;`, `&#123;` or `&#x1F600;`.
///
/// `None` for anything that is not a valid entity — an unknown name, a missing semicolon,
/// an empty number, or a code point outside Unicode's scalar values.
pub fn decode(source: &str) -> Option<String> {
    let name = source.strip_prefix('&')?.strip_suffix(';')?;
    if name.is_empty() {
        return None;
    }
    if let Some(number) = name.strip_prefix('#') {
        let (digits, radix) = match number
            .strip_prefix('x')
            .or_else(|| number.strip_prefix('X'))
        {
            Some(hex) => (hex, 16),
            None => (number, 10),
        };
        if digits.is_empty() {
            return None;
        }
        let code = u32::from_str_radix(digits, radix).ok()?;
        let character = char::from_u32(code).filter(|character| *character != '\0')?;
        return Some(character.to_string());
    }
    named(name).map(String::from)
}

/// The named entities Markdown in practice carries: Latin-1 letters, punctuation, arrows
/// and the common mathematical and currency symbols.
fn named(name: &str) -> Option<char> {
    Some(match name {
        "amp" | "AMP" => '&',
        "lt" | "LT" => '<',
        "gt" | "GT" => '>',
        "quot" | "QUOT" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "iexcl" => '¡',
        "cent" => '¢',
        "pound" => '£',
        "curren" => '¤',
        "yen" => '¥',
        "brvbar" => '¦',
        "sect" => '§',
        "uml" => '¨',
        "copy" | "COPY" => '©',
        "ordf" => 'ª',
        "laquo" => '«',
        "not" => '¬',
        "shy" => '\u{ad}',
        "reg" | "REG" => '®',
        "macr" => '¯',
        "deg" => '°',
        "plusmn" => '±',
        "sup2" => '²',
        "sup3" => '³',
        "acute" => '´',
        "micro" => 'µ',
        "para" => '¶',
        "middot" => '·',
        "cedil" => '¸',
        "sup1" => '¹',
        "ordm" => 'º',
        "raquo" => '»',
        "frac14" => '¼',
        "frac12" => '½',
        "frac34" => '¾',
        "iquest" => '¿',
        "Agrave" => 'À',
        "Aacute" => 'Á',
        "Acirc" => 'Â',
        "Atilde" => 'Ã',
        "Auml" => 'Ä',
        "Aring" => 'Å',
        "AElig" => 'Æ',
        "Ccedil" => 'Ç',
        "Egrave" => 'È',
        "Eacute" => 'É',
        "Ecirc" => 'Ê',
        "Euml" => 'Ë',
        "Igrave" => 'Ì',
        "Iacute" => 'Í',
        "Icirc" => 'Î',
        "Iuml" => 'Ï',
        "ETH" => 'Ð',
        "Ntilde" => 'Ñ',
        "Ograve" => 'Ò',
        "Oacute" => 'Ó',
        "Ocirc" => 'Ô',
        "Otilde" => 'Õ',
        "Ouml" => 'Ö',
        "times" => '×',
        "Oslash" => 'Ø',
        "Ugrave" => 'Ù',
        "Uacute" => 'Ú',
        "Ucirc" => 'Û',
        "Uuml" => 'Ü',
        "Yacute" => 'Ý',
        "THORN" => 'Þ',
        "szlig" => 'ß',
        "agrave" => 'à',
        "aacute" => 'á',
        "acirc" => 'â',
        "atilde" => 'ã',
        "auml" => 'ä',
        "aring" => 'å',
        "aelig" => 'æ',
        "ccedil" => 'ç',
        "egrave" => 'è',
        "eacute" => 'é',
        "ecirc" => 'ê',
        "euml" => 'ë',
        "igrave" => 'ì',
        "iacute" => 'í',
        "icirc" => 'î',
        "iuml" => 'ï',
        "eth" => 'ð',
        "ntilde" => 'ñ',
        "ograve" => 'ò',
        "oacute" => 'ó',
        "ocirc" => 'ô',
        "otilde" => 'õ',
        "ouml" => 'ö',
        "divide" => '÷',
        "oslash" => 'ø',
        "ugrave" => 'ù',
        "uacute" => 'ú',
        "ucirc" => 'û',
        "uuml" => 'ü',
        "yacute" => 'ý',
        "thorn" => 'þ',
        "yuml" => 'ÿ',
        "OElig" => 'Œ',
        "oelig" => 'œ',
        "Scaron" => 'Š',
        "scaron" => 'š',
        "Yuml" => 'Ÿ',
        "Zcaron" => 'Ž',
        "zcaron" => 'ž',
        "fnof" => 'ƒ',
        "circ" => 'ˆ',
        "tilde" => '˜',
        "ensp" => '\u{2002}',
        "emsp" => '\u{2003}',
        "thinsp" => '\u{2009}',
        "zwnj" => '\u{200c}',
        "zwj" => '\u{200d}',
        "lrm" => '\u{200e}',
        "rlm" => '\u{200f}',
        "ndash" => '–',
        "mdash" => '—',
        "lsquo" => '‘',
        "rsquo" => '’',
        "sbquo" => '‚',
        "ldquo" => '“',
        "rdquo" => '”',
        "bdquo" => '„',
        "dagger" => '†',
        "Dagger" => '‡',
        "bull" => '•',
        "hellip" => '…',
        "permil" => '‰',
        "prime" => '′',
        "Prime" => '″',
        "lsaquo" => '‹',
        "rsaquo" => '›',
        "oline" => '‾',
        "frasl" => '⁄',
        "euro" => '€',
        "trade" => '™',
        "larr" => '←',
        "uarr" => '↑',
        "rarr" => '→',
        "darr" => '↓',
        "harr" => '↔',
        "crarr" => '↵',
        "lArr" => '⇐',
        "uArr" => '⇑',
        "rArr" => '⇒',
        "dArr" => '⇓',
        "hArr" => '⇔',
        "forall" => '∀',
        "part" => '∂',
        "exist" => '∃',
        "empty" => '∅',
        "nabla" => '∇',
        "isin" => '∈',
        "notin" => '∉',
        "ni" => '∋',
        "prod" => '∏',
        "sum" => '∑',
        "minus" => '−',
        "lowast" => '∗',
        "radic" => '√',
        "prop" => '∝',
        "infin" => '∞',
        "ang" => '∠',
        "and" => '∧',
        "or" => '∨',
        "cap" => '∩',
        "cup" => '∪',
        "int" => '∫',
        "there4" => '∴',
        "sim" => '∼',
        "cong" => '≅',
        "asymp" => '≈',
        "ne" => '≠',
        "equiv" => '≡',
        "le" => '≤',
        "ge" => '≥',
        "sub" => '⊂',
        "sup" => '⊃',
        "nsub" => '⊄',
        "sube" => '⊆',
        "supe" => '⊇',
        "oplus" => '⊕',
        "otimes" => '⊗',
        "perp" => '⊥',
        "sdot" => '⋅',
        "lceil" => '⌈',
        "rceil" => '⌉',
        "lfloor" => '⌊',
        "rfloor" => '⌋',
        "lang" => '⟨',
        "rang" => '⟩',
        "loz" => '◊',
        "spades" => '♠',
        "clubs" => '♣',
        "hearts" => '♥',
        "diams" => '♦',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Named and numeric forms decode to the character they spell; anything incomplete or
    /// outside Unicode's scalar values stays source.
    #[test]
    fn an_entity_decodes_only_when_it_is_complete_and_valid() {
        assert_eq!(decode("&amp;").as_deref(), Some("&"));
        assert_eq!(decode("&lt;").as_deref(), Some("<"));
        assert_eq!(decode("&szlig;").as_deref(), Some("ß"));
        assert_eq!(decode("&hellip;").as_deref(), Some("…"));
        assert_eq!(decode("&#65;").as_deref(), Some("A"));
        assert_eq!(decode("&#x1F600;").as_deref(), Some("😀"));
        assert_eq!(decode("&#X41;").as_deref(), Some("A"));
        for invalid in [
            "&foo;",
            "&#;",
            "&#x;",
            "&#x110000;",
            "&#xD800;",
            "&#0;",
            "&amp",
            "amp;",
            "",
        ] {
            assert_eq!(decode(invalid), None, "{invalid:?} is not a valid entity");
        }
    }
}
