//! What a construct is drawn as this frame.
//!
//! One predicate answers it for every construct: does the selection touch the bytes its
//! class watches? The class decides which bytes those are, so a heading's `#` never lights
//! up from its title, and a fence lights up as one unit across all its lines.

use std::ops::Range;

/// What a construct is drawn as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// The parse result: a mark's effect, a formula's typesetting, a code card, a grid.
    Rendered,
    /// The bytes as written, delimiters and all.
    Source,
}

/// The bytes whose touch decides a construct's form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// One inline construct. A caret inside it, or resting against either delimiter, shows
    /// its source; a caret past it leaves the mark's effect.
    Inline,
    /// A construct that is a unit across all its lines: a fence, a display formula, a
    /// table. A caret anywhere inside shows the whole construct as source.
    Fence,
    /// One line's marker: a heading's `# `, an item's bullet, a quote's `> `. It belongs to
    /// that line alone, so a caret in the text it introduces never reveals it.
    Marker,
}

impl Scope {
    /// The bytes this class watches: the whole construct, or one line's marker.
    ///
    /// A marker construct always carries its line's marker; a caller that has none has
    /// nothing to watch, so the construct's own span stands in.
    pub fn watched<'a>(
        self,
        construct: &'a Range<usize>,
        marker: Option<&'a Range<usize>>,
    ) -> &'a Range<usize> {
        match self {
            Scope::Inline | Scope::Fence => construct,
            Scope::Marker => marker.unwrap_or(construct),
        }
    }

    /// How this construct is drawn while `selection` stands where it does.
    pub fn form(
        self,
        construct: &Range<usize>,
        marker: Option<&Range<usize>>,
        selection: &Range<usize>,
    ) -> Form {
        match touches(self.watched(construct, marker), selection) {
            true => Form::Source,
            false => Form::Rendered,
        }
    }
}

/// Whether `selection` touches `span`: the two share a byte or a boundary.
///
/// Boundaries count, so a caret resting against a delimiter is a caret in the construct.
/// That is what makes a marker reveal at the position right after it as well as inside it.
pub fn touches(span: &Range<usize>, selection: &Range<usize>) -> bool {
    selection.start <= span.end && span.start <= selection.end
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The class, not the construct's size, decides what a touch measures. Each row is a
    /// position that separates two classes: inside, at either end, one byte past, and —
    /// for a marker — inside the construct but outside the marker, which is the heading
    /// case this model exists to get right.
    #[test]
    fn the_class_of_a_construct_decides_which_bytes_a_touch_measures() {
        let inline = 0..5;
        let fence = 0..20;
        let block = 0..40;
        let marker = 0..2;
        let cases: &[(Scope, &str, Range<usize>, Form)] = &[
            (Scope::Inline, "a caret inside the mark", 2..2, Form::Source),
            (
                Scope::Inline,
                "a caret at the opening delimiter",
                0..0,
                Form::Source,
            ),
            (
                Scope::Inline,
                "a caret at the closing delimiter",
                5..5,
                Form::Source,
            ),
            (Scope::Inline, "a caret past the mark", 6..6, Form::Rendered),
            (
                Scope::Inline,
                "a selection inside the mark",
                1..3,
                Form::Source,
            ),
            (
                Scope::Inline,
                "a selection past the mark",
                6..9,
                Form::Rendered,
            ),
            (
                Scope::Fence,
                "a caret on the opening line",
                0..0,
                Form::Source,
            ),
            (
                Scope::Fence,
                "a caret on a middle line",
                10..10,
                Form::Source,
            ),
            (
                Scope::Fence,
                "a caret at the construct's end",
                20..20,
                Form::Source,
            ),
            (Scope::Fence, "a caret just past it", 21..21, Form::Rendered),
            (Scope::Marker, "a caret in the marker", 1..1, Form::Source),
            (
                Scope::Marker,
                "a caret at the marker's end",
                2..2,
                Form::Source,
            ),
            (
                Scope::Marker,
                "a caret in the text it introduces",
                3..3,
                Form::Rendered,
            ),
            (
                Scope::Marker,
                "a caret further down the block",
                10..10,
                Form::Rendered,
            ),
            (
                Scope::Marker,
                "a selection across the marker",
                0..4,
                Form::Source,
            ),
            (
                Scope::Marker,
                "a selection that starts past it",
                3..10,
                Form::Rendered,
            ),
        ];
        for (scope, what, selection, expected) in cases {
            let construct = match scope {
                Scope::Inline => &inline,
                Scope::Fence => &fence,
                Scope::Marker => &block,
            };
            let marker = matches!(scope, Scope::Marker).then_some(&marker);
            assert_eq!(
                scope.form(construct, marker, selection),
                *expected,
                "{what} on a {scope:?} construct"
            );
        }
    }

    /// A marker with no marker span to watch has nothing narrower than its construct, so it
    /// falls back to the whole thing instead of revealing from anywhere.
    #[test]
    fn a_marker_without_its_own_span_watches_the_construct() {
        assert_eq!(Scope::Marker.watched(&(0..9), None), &(0..9));
        assert_eq!(Scope::Marker.watched(&(0..9), Some(&(0..2))), &(0..2));
    }
}
