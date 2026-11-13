//! Styled text: a paragraph is a list of runs, not one string with one style.

use crate::geom::{Color, Size};
use crate::id::Id;

/// Requested font weight, CSS 1..=1000.
///
/// A continuous scale with named anchors: a provider matches the closest available face
/// rather than switching between two buckets, so 600 is meaningful on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Weight(pub u16);

impl Weight {
    pub const THIN: Self = Self(100);
    pub const LIGHT: Self = Self(300);
    pub const REGULAR: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMIBOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
    pub const BLACK: Self = Self(900);
}

/// Visual style of one run.
///
/// Rich text (Markdown/HTML/XML) reduces to many runs sharing a small set of styles, so
/// this is the unit to grow: adding weight or underline happens here, not in new fields.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    /// Font weight; `REGULAR` for normal text.
    ///
    /// Weight picks the *face*, so it is not part of `Variations`: a run asking for 700
    /// must reach a Bold face, not just a heavier axis of Regular.
    pub weight: Weight,
    /// Italic (or oblique) face.
    pub italic: bool,
    /// Which family this run asks for.
    pub family: Family,
    /// Variable-font axes beyond weight; `NONE` keeps the face's defaults.
    pub variations: Variations,
    /// OpenType features; `NONE` keeps the shaper's defaults.
    pub features: Features,
    /// Extra advance after every cluster, in pixels: CSS `letter-spacing`.
    ///
    /// It goes after the last cluster too, which is how CSS counts it in the line box.
    pub letter_spacing: f32,
    /// Ink drawn around each glyph's outline, for text that has to read on any picture.
    pub outline: GlyphOutline,
}

/// A run's glyph outline.
///
/// It belongs to the run rather than to the primitive because it is rasterized with the
/// glyph: the coverage tile and the contour it came from have to be one answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphOutline {
    /// Width in the run's own units, centred on the contour; zero paints no outline.
    pub width: f32,
    /// The ink the outline is drawn in.
    pub color: Color,
}

impl Default for GlyphOutline {
    fn default() -> Self {
        Self::NONE
    }
}

impl GlyphOutline {
    pub const NONE: GlyphOutline = GlyphOutline {
        width: 0.0,
        color: Color::TRANSPARENT,
    };

    /// Whether this outline paints anything at all.
    pub fn paints(&self) -> bool {
        self.width > 0.0 && self.color.a > 0.0
    }
}

impl TextStyle {
    pub const fn new(size: f32) -> Self {
        Self {
            size,
            weight: Weight::REGULAR,
            italic: false,
            family: Family::Generic(GenericFamily::SystemUi),
            variations: Variations::NONE,
            features: Features::NONE,
            letter_spacing: 0.0,
            outline: GlyphOutline::NONE,
        }
    }

    /// Builder: draw an outline of `width` in `color` around every glyph.
    pub const fn outline(mut self, width: f32, color: Color) -> Self {
        self.outline = GlyphOutline { width, color };
        self
    }

    /// Builder: this run asks for `w`.
    pub const fn weight(mut self, w: Weight) -> Self {
        self.weight = w;
        self
    }

    /// Builder: this run asks for an italic face.
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Builder: this run asks for `family`, named or generic.
    pub const fn family(mut self, family: Family) -> Self {
        self.family = family;
        self
    }

    /// Builder: this run asks for a named family.
    pub const fn named(mut self, family: &'static str) -> Self {
        self.family = Family::Named(family);
        self
    }

    /// Builder: this run asks for a generic family.
    pub const fn generic(mut self, family: GenericFamily) -> Self {
        self.family = Family::Generic(family);
        self
    }

    /// Builder: variable-font axes for this run.
    pub const fn variations(mut self, variations: Variations) -> Self {
        self.variations = variations;
        self
    }

    /// Builder: OpenType features for this run.
    pub const fn features(mut self, features: Features) -> Self {
        self.features = features;
        self
    }

    /// Builder: extra advance after every cluster, in pixels.
    pub const fn tracking(mut self, spacing: f32) -> Self {
        self.letter_spacing = spacing;
        self
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::new(14.0)
    }
}

/// A family a run can ask for: a name, or the platform's generic slot for a class.
///
/// `&'static str` keeps `TextStyle` copyable and the cache key cheap; families are
/// compile-time constants, never user data. Mainline engines carry the same pair —
/// Flutter `fontFamily` / `fontFamilyFallback`, GPUI `font_family`, egui `FontFamily`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// The platform's own choice for a class of text.
    Generic(GenericFamily),
    /// A named family, resolved by the provider (system or registered).
    Named(&'static str),
}

impl Default for Family {
    fn default() -> Self {
        Self::Generic(GenericFamily::SystemUi)
    }
}

impl Family {
    /// Whether this family draws symbols rather than text.
    ///
    /// A symbol's ink *is* its glyph: one em-sized mark that fills the box, so it is
    /// centred by its ink. Text is centred by its line box, because its ink (cap height
    /// above the baseline) sits high inside it.
    pub fn is_symbol(self) -> bool {
        match self {
            Family::Named(name) => name.starts_with("Material Symbols") || name == "SFSymbols",
            Family::Generic(_) => false,
        }
    }
}

/// Which generic family a request maps to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GenericFamily {
    #[default]
    SansSerif,
    Serif,
    Monospace,
    /// The platform's own widget font.
    SystemUi,
}

/// Variable-font axes for one run, excluding weight (that picks the face).
///
/// The four axes Material Symbols exposes are `wght` (in `TextStyle::weight`), `opsz`,
/// `FILL` and `GRAD`; a style that leaves them unset gets the face's defaults.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Variations {
    /// `opsz`: optical size; `None` follows the font size.
    pub optical_size: Option<f32>,
    /// `FILL`: 0 outlined, 1 filled.
    pub fill: Option<f32>,
    /// `GRAD`: emphasis, typically -50..200.
    pub grade: Option<f32>,
}

impl Variations {
    pub const NONE: Self = Self {
        optical_size: None,
        fill: None,
        grade: None,
    };

    /// Builder: filled or outlined, as the `FILL` axis.
    pub const fn fill(mut self, fill: f32) -> Self {
        self.fill = Some(fill);
        self
    }

    /// Builder: optical size, as the `opsz` axis.
    pub const fn optical_size(mut self, size: f32) -> Self {
        self.optical_size = Some(size);
        self
    }

    /// Builder: emphasis, as the `GRAD` axis.
    pub const fn grade(mut self, grade: f32) -> Self {
        self.grade = Some(grade);
        self
    }

    /// The axes at 1/64 precision, the form caches key on.
    ///
    /// Quantising is what keeps an animated `FILL` finite: 0..1 yields 65 instances, not one
    /// per frame.
    pub fn quantized(&self) -> [Option<i32>; 3] {
        [self.optical_size, self.fill, self.grade].map(|v| {
            v.filter(|v| v.is_finite())
                .map(|v| (v * 64.0).round() as i32)
        })
    }
}

/// At most `CAPACITY` OpenType feature switches for one run, as `(tag, value)` pairs.
///
/// Mainline engines carry a list here (Flutter `fontFeatures`, GPUI `FontFeatures`); a
/// fixed capacity keeps `TextStyle` copyable, which per-span styles depend on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Features {
    settings: [Option<([u8; 4], u32)>; FEATURE_CAPACITY],
}

/// Feature switches one run can carry.
pub const FEATURE_CAPACITY: usize = 4;

impl Features {
    pub const CAPACITY: usize = FEATURE_CAPACITY;
    /// The shaper's own defaults.
    pub const NONE: Self = Self {
        settings: [None; FEATURE_CAPACITY],
    };
    /// `tnum`: digits take one advance, so columns line up.
    pub const TABULAR: Self = Self::NONE.set(*b"tnum", 1);
    /// `calt`/`liga` off: no contextual or standard ligatures.
    pub const NO_LIGATURES: Self = Self::NONE.set(*b"calt", 0).set(*b"liga", 0);

    /// Builder: set `tag` to `value`; a full set ignores the extra setting.
    pub const fn set(mut self, tag: [u8; 4], value: u32) -> Self {
        let mut i = 0;
        while i < Self::CAPACITY {
            match self.settings[i] {
                Some((t, _)) if same_tag(t, tag) => {
                    self.settings[i] = Some((tag, value));
                    return self;
                }
                None => {
                    self.settings[i] = Some((tag, value));
                    return self;
                }
                Some(_) => i += 1,
            }
        }
        self
    }

    /// The switches in effect, in the order they were set.
    pub fn iter(&self) -> impl Iterator<Item = ([u8; 4], u32)> + '_ {
        self.settings.iter().flatten().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.settings.iter().all(Option::is_none)
    }
}

const fn same_tag(a: [u8; 4], b: [u8; 4]) -> bool {
    a[0] == b[0] && a[1] == b[1] && a[2] == b[2] && a[3] == b[3]
}

/// Object replacement character: the byte an atom occupies in the flattened text.
pub const OBJECT: char = '\u{FFFC}';

/// How an atom sits against its line's box; only atoms use this.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalAlign {
    /// The atom's own baseline meets the line's text baseline.
    #[default]
    Baseline,
    /// The atom's top meets the line box's top.
    Top,
    /// Split the leftover space above and below.
    Middle,
    /// The atom's bottom meets the line box's bottom.
    Bottom,
}

impl VerticalAlign {
    /// Space the atom claims above and below the baseline, as `(above, below)`.
    pub fn extents(self, size: Size, baseline: f32) -> (f32, f32) {
        match self {
            Self::Baseline => (baseline, size.h - baseline),
            Self::Top => (0.0, size.h),
            Self::Middle => (size.h * 0.5, size.h * 0.5),
            Self::Bottom => (size.h, 0.0),
        }
    }

    /// Where the atom's top sits inside a line box of `height` whose baseline is `ascent` down.
    pub fn top(self, size: Size, baseline: f32, ascent: f32, height: f32) -> f32 {
        let y = match self {
            Self::Baseline => ascent - baseline,
            Self::Top => 0.0,
            Self::Middle => (height - size.h) * 0.5,
            Self::Bottom => height - size.h,
        };
        y.max(0.0)
    }
}

/// An object embedded in a paragraph, occupying one U+FFFC at `at`.
#[derive(Clone, Debug, PartialEq)]
pub struct AtomBox {
    /// Byte offset of its U+FFFC in the flattened text.
    pub at: usize,
    /// The bytes it stands for: one U+FFFC, or the whole source range an editor replaced.
    ///
    /// Whatever it covers draws no glyph — the box takes its place — so the bytes after it
    /// keep their offsets, which is what lets an editor render an object and still read a
    /// caret position as a source offset.
    pub len: usize,
    pub size: Size,
    /// Its own baseline, from its top; the default sits it on the text baseline.
    pub baseline: f32,
    pub align: VerticalAlign,
    /// What to paint inside the box; `None` is a hole the painter fills with a placeholder.
    pub object: Option<crate::math::InlineObject>,
    /// A child node laid out inside this box, instead of an object the text pass paints.
    ///
    /// Layout measures the child at the text block's width and gives the atom its size and
    /// baseline; placement puts the node at the atom's rect, and the ordinary painter draws
    /// it. The text pass draws no placeholder for a holder.
    pub holder: Option<Id>,
}

impl AtomBox {
    /// An atom of `size` sitting on the text baseline, like a replaced element in CSS.
    pub fn new(at: usize, size: Size) -> Self {
        Self {
            at,
            len: OBJECT.len_utf8(),
            size,
            baseline: size.h,
            align: VerticalAlign::Baseline,
            object: None,
            holder: None,
        }
    }

    /// Builder: this atom holds `id`, a child of the text node, instead of a painted object.
    ///
    /// The child declares its own size class; the atom's size and baseline are overwritten
    /// with what the child measures, so the builder's `size` is only a fallback for a
    /// binding that does not resolve.
    pub fn holder(mut self, id: impl Into<Id>) -> Self {
        self.holder = Some(id.into());
        self
    }

    /// Builder: this atom stands for `len` bytes, whose text draws nothing.
    pub fn spanning(mut self, len: usize) -> Self {
        self.len = len;
        self
    }

    /// Builder: fill this atom with `object`, taking its size and baseline from it.
    pub fn object(mut self, object: crate::math::InlineObject) -> Self {
        self.size = object.size();
        self.baseline = object.baseline();
        self.object = Some(object);
        self
    }

    pub fn align(mut self, align: VerticalAlign) -> Self {
        self.align = align;
        self
    }

    pub fn baseline(mut self, baseline: f32) -> Self {
        self.baseline = baseline;
        self
    }
}

/// Horizontal placement of every line in a paragraph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    /// Leading edge: left in LTR, right in RTL.
    #[default]
    Start,
    Center,
    End,
    /// Every line but the last is stretched to the block's width, adding space at each
    /// break opportunity inside it. The last line keeps `Start`.
    Justify,
}

/// The shape of a decoration rule. Every style occupies the same band under or through
/// the text; they differ only in how the band is filled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Decoration {
    #[default]
    None,
    Single,
    Double,
    Dotted,
    Wavy,
}

impl Decoration {
    /// Whether this run draws a rule at all.
    pub fn drawn(self) -> bool {
        !matches!(self, Decoration::None)
    }
}

/// Whether a run paints, and whether its bytes occupy the flow.
///
/// Hiding syntax in a rendered form removes it from the text: the words around it close up.
/// A stand-in is different — it paints over bytes that keep their room, because the form
/// chooses what is painted, never where a position sits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Visibility {
    /// Painted, taking its advance.
    #[default]
    Drawn,
    /// Not painted, taking no advance: syntax the rendered form does not spell. Its bytes
    /// stay in the block so a caret offset still names the same source byte.
    Hidden,
    /// Not painted, keeping its advance: bytes a stand-in is painted over, so the caret can
    /// stand among them and a column step can land on the byte at its column.
    Covered,
}

impl Visibility {
    /// Whether the run paints its own glyphs.
    pub fn painted(self) -> bool {
        self == Visibility::Drawn
    }

    /// Whether the bytes still measure an advance in the flow.
    pub fn takes_room(self) -> bool {
        self != Visibility::Hidden
    }
}

/// A fixed width a run occupies, with its glyphs placed inside it: bytes and the stand-in
/// drawn over them take the same room, or revealing the source moves the text beside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Room {
    pub width: f32,
    pub align: TextAlign,
}

impl Room {
    pub fn new(width: f32, align: TextAlign) -> Self {
        Self { width, align }
    }

    /// Where a `content`-wide run sits inside this room, from the room's left edge.
    pub fn offset(self, content: f32) -> f32 {
        aligned_offset(self.align, content, self.width)
    }
}

/// A contiguous piece of text sharing one style.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub style: TextStyle,
    /// Ink for this run alone; `None` uses the node's ink.
    ///
    /// Colour stays out of `TextStyle` because the type ramp must not choose it: a link or an
    /// emphasised word is a component decision, and the run is where a component speaks.
    pub ink: Option<Color>,
    /// Colour painted behind this run's text, across the line boxes it occupies.
    pub background: Option<Color>,
    /// A rule under this run's text, in its ink.
    pub underline: Decoration,
    /// A rule through this run's text, in its ink.
    pub strikethrough: Decoration,
    /// What the run paints, and whether it keeps its room.
    pub visibility: Visibility,
    /// The room the run takes, whatever its glyphs measure.
    pub room: Option<Room>,
}

impl TextRun {
    pub fn new(text: impl Into<String>, style: TextStyle) -> Self {
        Self {
            text: text.into(),
            style,
            ink: None,
            background: None,
            underline: Decoration::None,
            strikethrough: Decoration::None,
            visibility: Visibility::Drawn,
            room: None,
        }
    }

    /// Builder: this run takes `width`, with its glyphs placed by `align` inside it.
    pub fn roomed(mut self, width: f32, align: TextAlign) -> Self {
        self.room = Some(Room::new(width, align));
        self
    }

    /// Builder: hide this run's text without removing its bytes, closing the text up.
    pub fn hidden(mut self) -> Self {
        self.visibility = Visibility::Hidden;
        self
    }

    /// Builder: draw this run in its own ink.
    pub fn inked(mut self, ink: Color) -> Self {
        self.ink = Some(ink);
        self
    }

    /// Builder: paint a background behind this run, the way an inline code span is tinted.
    pub fn on_background(mut self, background: Color) -> Self {
        self.background = Some(background);
        self
    }

    /// Builder: underline this run, the way a link is.
    pub fn underlined(self) -> Self {
        self.underlined_with(Decoration::Single)
    }

    /// Builder: underline this run in a given style.
    pub fn underlined_with(mut self, style: Decoration) -> Self {
        self.underline = style;
        self
    }

    /// Builder: strike this run through, the way deleted text is.
    pub fn struck_through(self) -> Self {
        self.struck_through_with(Decoration::Single)
    }

    /// Builder: strike this run through in a given style.
    pub fn struck_through_with(mut self, style: Decoration) -> Self {
        self.strikethrough = style;
        self
    }
}

/// Line numbers a block asks the painter to draw beside its lines.
///
/// The gutter's width is the block's own `indent`/`hanging`: the painter draws the number
/// right-aligned in it, ending `gap` before the line's text, and a line that wrapped is not
/// a line of its own, so it carries no number and starts at the same edge as the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineNumbers {
    /// The number the block's first line carries.
    pub start: u32,
    pub ink: Color,
    /// Size the digits are drawn at; the block's style decides the face.
    pub size: f32,
    /// Room between a number's right edge and the line's text.
    pub gap: f32,
}

/// A range of source bytes that shares one value with its siblings: a block's indentation.
///
/// Every member is ordinary text — drawn, reachable by the caret, edited like any other
/// byte. Editing the member its block spells out carries the same signed delta to the
/// group's other members inside the one edit, so the value reads alike on every line
/// without a byte becoming invisible.
#[derive(Clone, Debug, PartialEq)]
pub struct Structure {
    pub group: u32,
    /// The member's bytes, in this block's own offsets.
    pub range: std::ops::Range<usize>,
    /// Whether this member is the one the source spells out.
    pub shown: bool,
}

/// One paragraph: runs plus the properties that apply to the whole block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextBlock {
    pub runs: Vec<TextRun>,
    pub align: TextAlign,
    /// Line cap; `None` wraps freely, `Some(n)` ellipsizes past `n` lines.
    pub max_lines: Option<u32>,
    /// Extra space between lines, not after the last one.
    ///
    /// Separate from the font's own line gap: that one belongs to the face, this one to
    /// the paragraph, and a caller tuning density must not have to fake a font metric.
    pub line_gap: f32,
    /// First-line indent; later lines start at `hanging`.
    pub indent: f32,
    /// Left offset of every line after the first: a hanging indent, so a paragraph whose
    /// first line opens with a marker wraps clear of it instead of sliding under it.
    pub hanging: f32,
    /// Objects embedded in the flow, each occupying a U+FFFC in `runs`.
    pub atoms: Vec<AtomBox>,
    /// Where this block's first byte sits in its input owner's buffer, when the input
    /// draws several blocks of one buffer. `None` for a block that is its own input.
    pub input_base: Option<usize>,
    /// Numbers to draw in the gutter `indent` reserves, one per source line.
    pub line_numbers: Option<LineNumbers>,
    /// Ranges that share one block value, such as a fenced block's indentation.
    pub structures: Vec<Structure>,
}

impl TextBlock {
    /// A block of one run, the shape most callers want.
    pub fn plain(text: impl Into<String>, style: TextStyle) -> Self {
        Self {
            runs: vec![TextRun::new(text, style)],
            align: TextAlign::Start,
            max_lines: None,
            line_gap: 0.0,
            indent: 0.0,
            hanging: 0.0,
            atoms: Vec::new(),
            input_base: None,
            line_numbers: None,
            structures: Vec::new(),
        }
    }

    /// The two left edges its lines start at: the first line's, and every later one's.
    pub fn edges(&self) -> LineEdges {
        LineEdges {
            first: self.indent,
            body: self.hanging,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|r| r.text.is_empty())
    }

    /// The atom occupying `byte`, if one does.
    pub fn atom_at(&self, byte: usize) -> Option<&AtomBox> {
        self.atoms.iter().find(|a| a.at == byte)
    }

    /// Builder: embed one more atom.
    pub fn atom(mut self, atom: AtomBox) -> Self {
        self.atoms.push(atom);
        self
    }

    /// Concatenated text plus the span table that maps byte ranges back to styles.
    ///
    /// The breaker and shaper work on the flat string; the span table is how they still
    /// see per-run styles. Runs are contiguous, so the table covers every byte exactly once.
    ///
    /// A span boundary never falls inside a grapheme: the mark takes the style of its base,
    /// as a browser applies it, and no shaper is handed a mark without the base it attaches
    /// to. A cut there would detach the mark in any direction, and in an RTL run it is the
    /// detached mark UAX #9 L3 exists to prevent.
    pub fn flatten(&self) -> (String, Vec<Span>) {
        let mut text = String::new();
        let mut raw = Vec::with_capacity(self.runs.len());
        for run in &self.runs {
            let start = text.len();
            text.push_str(&run.text);
            if text.len() > start {
                raw.push(Span {
                    range: start..text.len(),
                    style: run.style,
                    ink: run.ink,
                    background: run.background,
                    underline: run.underline,
                    strikethrough: run.strikethrough,
                    visibility: run.visibility,
                    room: run.room,
                });
            }
        }
        // Cuts arrive in run order, so one walk over the clusters answers all of them.
        // Snapping each cut on its own would rescan the text from byte 0 per run, which
        // makes flatten quadratic in a block's length — and every text on the page is
        // flattened once a frame for damage.
        let mut clusters = std::iter::once(0)
            .chain(crate::grapheme::clusters(&text).map(|(at, cluster)| at + cluster.len()))
            .peekable();
        let mut spans: Vec<Span> = Vec::with_capacity(raw.len());
        for span in raw {
            let Some(last) = spans.last_mut() else {
                spans.push(span);
                continue;
            };
            let cut = span.range.start;
            while clusters.peek().is_some_and(|&boundary| boundary < cut) {
                clusters.next();
            }
            let snapped = clusters.peek().copied().unwrap_or(text.len());
            if snapped == cut {
                spans.push(span);
                continue;
            }
            // The cluster that straddles `cut` belongs to the span it started in.
            last.range.end = snapped.min(span.range.end);
            if snapped < span.range.end {
                spans.push(Span {
                    range: snapped..span.range.end,
                    ..span
                });
            }
        }
        drop(clusters);
        (text, spans)
    }

    /// Style in effect at `offset`, used by painters walking a line.
    pub fn style_at(spans: &[Span], offset: usize) -> TextStyle {
        spans
            .iter()
            .find(|s| s.range.contains(&offset))
            .or_else(|| spans.last())
            .map_or_else(TextStyle::default, |s| s.style)
    }

    /// The byte ranges `Hidden` runs cover, adjacent ones merged. Covered bytes are left out:
    /// they keep their advance and stand under a painted element, so a caret may reach them.
    pub fn hidden_ranges(&self) -> Vec<std::ops::Range<usize>> {
        let mut out: Vec<std::ops::Range<usize>> = Vec::new();
        let mut at = 0usize;
        for run in &self.runs {
            let end = at + run.text.len();
            if run.visibility == Visibility::Hidden {
                match out.last_mut() {
                    Some(last) if last.end == at => last.end = end,
                    _ => out.push(at..end),
                }
            }
            at = end;
        }
        out
    }

    /// The room in effect at `offset`, if the run there declared one.
    pub fn room_at(spans: &[Span], offset: usize) -> Option<Room> {
        spans
            .iter()
            .find(|s| s.range.contains(&offset))
            .and_then(|s| s.room)
    }

    /// Visibility in effect at `offset`.
    pub fn visibility_at(spans: &[Span], offset: usize) -> Visibility {
        spans
            .iter()
            .find(|s| s.range.contains(&offset))
            .map_or(Visibility::Drawn, |s| s.visibility)
    }

    /// Run ink in effect at `offset`, if the run there asked for one.
    pub fn ink_at(spans: &[Span], offset: usize) -> Option<Color> {
        spans
            .iter()
            .find(|s| s.range.contains(&offset))
            .or_else(|| spans.last())
            .and_then(|s| s.ink)
    }
}

/// The boxes one decoration rule occupies, in the run's own space.
///
/// `x .. x + width` is the band the rule covers, `top` its upper edge and `thickness` its
/// height; the style only decides how the band is filled, so every style is placed the
/// same way and cannot drift from the text it decorates.
pub fn decoration_rects(
    style: Decoration,
    x: f32,
    width: f32,
    top: f32,
    thickness: f32,
    origin: f32,
) -> Vec<crate::geom::Rect> {
    use crate::geom::Rect;
    let width = width.max(0.0);
    let thickness = thickness.max(1.0);
    let rule = |x: f32, top: f32, w: f32| Rect::new(x, top, w.max(0.0), thickness);
    match style {
        Decoration::None => Vec::new(),
        Decoration::Single => vec![rule(x, top, width)],
        // The band a face reports holds one rule, so the pair grows away from the glyphs.
        Decoration::Double => vec![rule(x, top, width), rule(x, top + thickness * 2.0, width)],
        Decoration::Dotted => {
            // A square dot and a gap of two thicknesses, the cadence of an insertion mark.
            // The pattern starts at `origin`, so a piece cut by a selection continues it.
            let period = thickness * 3.0;
            let (left, right) = (x, x + width);
            let mut out = Vec::new();
            let mut at = origin;
            while at < right {
                let (from, to) = (at.max(left), (at + thickness).min(right));
                if from < to {
                    out.push(rule(from, top, to - from));
                }
                at += period;
            }
            out
        }
        // A sine sampled every two thicknesses: enough steps to read as a wave at rule
        // sizes, and at most half the run's width in boxes. Same phase rule as the dots.
        Decoration::Wavy => {
            let step = (thickness * 2.0).max(1.0);
            let period = thickness * 6.0;
            let amplitude = thickness * 1.2;
            let (left, right) = (x, x + width);
            let mut out = Vec::new();
            let mut at = origin;
            while at < right {
                let (from, to) = (at.max(left), (at + step).min(right));
                if from < to {
                    let phase = (at - origin) / period * std::f32::consts::TAU;
                    let y = top + amplitude * 0.5 * (1.0 + phase.sin());
                    out.push(rule(from, y, to - from));
                }
                at += step;
            }
            out
        }
    }
}

/// Bytes of the line break starting at `at`: two for `\r\n`, one for `\n` or `\r`, and
/// zero when no break starts there.
pub fn break_len(text: &str, at: usize) -> usize {
    match text.get(at..) {
        Some(rest) if rest.starts_with("\r\n") => 2,
        Some(rest) if matches!(rest.chars().next(), Some('\n' | '\r')) => 1,
        _ => 0,
    }
}

/// One line's slice of a decorated run, split where the selection starts and ends.
///
/// The rule changes ink exactly where the glyphs do: the selected half of a link takes the
/// selection's ink and the rest keeps the link's, with a hard cut at the boundary.
pub fn decoration_pieces(
    range: std::ops::Range<usize>,
    selection: Option<&(std::ops::Range<usize>, Color)>,
) -> Vec<(std::ops::Range<usize>, bool)> {
    let Some((selection, _)) = selection else {
        return vec![(range, false)];
    };
    let start = range.start.max(selection.start);
    let end = range.end.min(selection.end);
    if start >= end {
        return vec![(range, false)];
    }
    let mut out = Vec::with_capacity(3);
    if range.start < start {
        out.push((range.start..start, false));
    }
    out.push((start..end, true));
    if end < range.end {
        out.push((end..range.end, false));
    }
    out
}

/// A byte range and the style that applies to it.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub range: std::ops::Range<usize>,
    pub style: TextStyle,
    /// The run's own ink, when it carries one.
    pub ink: Option<Color>,
    /// The run's own background, when it carries one.
    pub background: Option<Color>,
    /// The rule under the run's text.
    pub underline: Decoration,
    /// The rule through the run's text.
    pub strikethrough: Decoration,
    /// What the span paints, and whether its bytes keep their room.
    pub visibility: Visibility,
    /// The room the span takes, whatever its glyphs measure.
    pub room: Option<Room>,
}

/// One laid-out line: where it sits, how tall it is, and the runs it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct InlineLine {
    /// The bytes this line covers. An empty line a hard break opened covers none, but
    /// still names where it sits, which is what places the caret on it.
    pub range: std::ops::Range<usize>,
    /// Top edge, relative to the block's origin.
    pub y: f32,
    /// Line box height: `max(line_height, ascent + descent)`.
    pub height: f32,
    /// Baseline, from this line's top edge.
    pub baseline: f32,
    /// Ink width of the line, which alignment measures.
    pub width: f32,
    /// Truncated by `max_lines`; a painter appends the ellipsis itself.
    pub ellipsis: bool,
    /// The runs on this line, in logical order.
    pub runs: Vec<InlineRun>,
    /// Atoms sitting on this line, in logical order.
    pub atoms: Vec<InlineAtom>,
}

/// Where an atom landed on its line: relative to the line's left edge and top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InlineAtom {
    /// Byte offset of the first byte it stands for.
    pub at: usize,
    /// The bytes it stands for; their text draws nothing, the box takes their place.
    pub len: usize,
    /// Left edge, from the line's left edge.
    pub x: f32,
    /// Top edge, from the line's top edge.
    pub y: f32,
    pub size: Size,
    pub align: VerticalAlign,
}

impl InlineAtom {
    /// Its rect on the surface, given the line's top-left corner.
    pub fn rect(&self, line_origin: crate::geom::Point) -> crate::geom::Rect {
        crate::geom::Rect::new(
            line_origin.x + self.x,
            line_origin.y + self.y,
            self.size.w,
            self.size.h,
        )
    }
}

impl InlineLine {
    /// Byte range this line covers; empty for the caret line of an empty block.
    pub fn range(&self) -> std::ops::Range<usize> {
        self.range.clone()
    }

    /// Bottom edge, relative to the block's origin.
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// One styled piece of a line, in logical order.
#[derive(Clone, Debug, PartialEq)]
pub struct InlineRun {
    pub range: std::ops::Range<usize>,
    pub style: TextStyle,
    /// An object embedded in the flow at this run's start (U+FFFC); P4b places it.
    pub atomic: Option<Size>,
}

/// The two left edges a block's lines can start at.
///
/// One type because the pair is always passed together and both are lengths: as separate
/// arguments a swap compiles and every line after the first drifts by the first line's
/// indent.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineEdges {
    /// The first line's left offset.
    pub first: f32,
    /// Every later line's: a hanging indent under a marker.
    pub body: f32,
}

impl LineEdges {
    /// The left offset of line `index`, never negative.
    pub fn at(&self, index: usize) -> f32 {
        if index == 0 { self.first } else { self.body }.max(0.0)
    }
}

/// A block wrapped once: layout sizes from it and paint draws from it.
///
/// Sharing one result is what keeps a caret, a selection and a painted line on the same
/// fact; measuring twice lets them drift apart.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineLayout {
    pub lines: Vec<InlineLine>,
    /// Widest line plus that line's own left offset.
    pub width: f32,
    /// Sum of the line boxes plus the gaps *between* them.
    pub height: f32,
    /// First line's baseline from the block's top edge; 0 when there are no lines.
    pub baseline: f32,
    /// The block's base direction, from the first strong character of its paragraph. It is
    /// the paragraph's, not a line's: a line that opens with digits has no strong character
    /// of its own and must still follow the paragraph.
    pub rtl: bool,
}

impl InlineLayout {
    /// The size layout reserves for the block.
    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// Top-left corner of line `index` inside a placed text node.
    ///
    /// The one place that turns a line and an atom into surface coordinates: placement, paint
    /// and hit testing all call it, so a widget inside a line lands where its caret does.
    /// `box_height` is the node's height as the painter reads it, and the block is centred in
    /// it; `wrap_width` is the box it was measured in (`Node::wrap_width`), which an
    /// unbreakable line still aligns inside. `edges` are the two left edges a marker opens.
    pub fn line_origin(
        &self,
        index: usize,
        origin: crate::geom::Point,
        box_height: f32,
        wrap_width: f32,
        edges: LineEdges,
        align: TextAlign,
    ) -> crate::geom::Point {
        let Some(line) = self.lines.get(index) else {
            return origin;
        };
        let left = edges.at(index);
        let spare = (box_height - self.height).max(0.0);
        let align = align_for_direction(align, self.rtl);
        // The inset a marker opens sits on the reading edge, so an RTL paragraph takes it
        // from its right: the room still shrinks by `left`, but its start is the box edge.
        let room = (wrap_width - left).max(0.0);
        let start = if self.rtl { 0.0 } else { left };
        crate::geom::Point::new(
            origin.x + start + aligned_offset(align, line.width, room),
            origin.y + spare * 0.5 + line.y,
        )
    }
}

/// The alignment a line takes once the paragraph's own direction is known.
///
/// `Start` and `End` are relative, as CSS `text-align: start` is: in a right-to-left
/// paragraph the first line of a list sits at the right. `Justify` is relative too, for the
/// one line it does not stretch: the last one, which starts at the reading edge. `Center`
/// names no edge and is returned unchanged.
pub fn align_for_direction(align: TextAlign, rtl: bool) -> TextAlign {
    match (align, rtl) {
        (TextAlign::Start | TextAlign::Justify, true) => TextAlign::End,
        (TextAlign::End, true) => TextAlign::Start,
        _ => align,
    }
}

/// How far a line's left edge moves inside the wrap width, per the block's alignment.
///
/// `Justify` stretches inside the line and starts flush, like `Start`.
pub fn aligned_offset(align: TextAlign, line_width: f32, max_width: f32) -> f32 {
    let spare = if max_width.is_finite() {
        (max_width - line_width).max(0.0)
    } else {
        0.0
    };
    match align {
        TextAlign::Start | TextAlign::Justify => 0.0,
        TextAlign::Center => spare * 0.5,
        TextAlign::End => spare,
    }
}

/// The mark a hidden field draws in place of `real`, with the same UTF-8 length so every
/// character boundary keeps its byte offset and carets, selections and edits need no map.
pub fn mask_char(real: char) -> &'static str {
    match real.len_utf8() {
        1 => "*",
        2 => "\u{b7}",
        3 => "\u{2022}",
        // No common font draws a four-byte mark, and two dots still add up to four bytes.
        _ => "\u{b7}\u{b7}",
    }
}

/// `text` with every character marked, byte for byte as long as the original.
pub fn masked(text: &str) -> String {
    text.chars().map(mask_char).collect()
}

/// A line's box from its text metrics and the atoms it carries: `(height, baseline)`.
pub fn line_metrics(
    text_ascent: f32,
    text_descent: f32,
    text_height: f32,
    atoms: &[&AtomBox],
) -> (f32, f32) {
    let mut ascent = text_ascent;
    let mut descent = text_descent;
    for atom in atoms {
        let (above, below) = atom.align.extents(atom.size, atom.baseline);
        ascent = ascent.max(above);
        descent = descent.max(below);
    }
    (text_height.max(ascent + descent).max(1.0), ascent)
}

/// Split every span overlapping `range` into runs clipped to it, cutting at atoms.
pub fn runs_in(block: &TextBlock, spans: &[Span], range: std::ops::Range<usize>) -> Vec<InlineRun> {
    let mut out = Vec::new();
    for span in spans
        .iter()
        .filter(|s| s.range.start < range.end && s.range.end > range.start)
    {
        let start = span.range.start.max(range.start);
        let end = span.range.end.min(range.end);
        let mut from = start;
        for atom in block.atoms.iter().filter(|a| a.at >= start && a.at < end) {
            if atom.at > from {
                out.push(InlineRun {
                    range: from..atom.at,
                    style: span.style,
                    atomic: None,
                });
            }
            let stop = (atom.at + atom.len).min(end);
            from = stop;
            out.push(InlineRun {
                range: atom.at..stop,
                style: span.style,
                atomic: Some(atom.size),
            });
        }
        if from < end {
            out.push(InlineRun {
                range: from..end,
                style: span.style,
                atomic: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(size: f32) -> TextStyle {
        TextStyle::new(size)
    }

    #[test]
    fn a_relative_alignment_follows_the_paragraph_direction() {
        // Start, End and Justify are edges of the reading direction: a justified paragraph's
        // last line is the one it does not stretch, so it starts at that edge.
        assert_eq!(
            align_for_direction(TextAlign::Start, false),
            TextAlign::Start
        );
        assert_eq!(align_for_direction(TextAlign::Start, true), TextAlign::End);
        assert_eq!(align_for_direction(TextAlign::End, true), TextAlign::Start);
        assert_eq!(align_for_direction(TextAlign::End, false), TextAlign::End);
        assert_eq!(
            align_for_direction(TextAlign::Center, true),
            TextAlign::Center
        );
        assert_eq!(
            align_for_direction(TextAlign::Justify, true),
            TextAlign::End
        );
        assert_eq!(
            align_for_direction(TextAlign::Justify, false),
            TextAlign::Justify
        );
    }

    #[test]
    fn a_right_to_left_block_starts_its_lines_at_the_right_edge() {
        // Start is the reading edge, so the same layout with `rtl` set puts a short line at
        // the other end of the wrap box; a centred one does not move.
        let line = |width| InlineLine {
            range: 0..2,
            y: 0.0,
            height: 20.0,
            baseline: 15.0,
            width,
            ellipsis: false,
            runs: Vec::new(),
            atoms: Vec::new(),
        };
        let layout = |rtl, width| InlineLayout {
            lines: vec![line(width)],
            width,
            height: 20.0,
            baseline: 15.0,
            rtl,
        };
        let origin = crate::geom::Point::new(0.0, 0.0);
        let x = |rtl, align| {
            layout(rtl, 50.0)
                .line_origin(
                    0,
                    origin,
                    20.0,
                    200.0,
                    LineEdges {
                        first: 0.0,
                        body: 0.0,
                    },
                    align,
                )
                .x
        };
        assert_eq!(x(false, TextAlign::Start), 0.0);
        assert_eq!(x(true, TextAlign::Start), 150.0);
        assert_eq!(x(true, TextAlign::Center), 75.0);
    }

    /// The inset a marker opens is on the reading edge, so an RTL paragraph takes it from
    /// its right: the first line gives up `indent` at the right edge, and every later one
    /// `hanging` there. Without the mirror the lines still right-align but never clear the
    /// marker, which would sit at the wrong end of the item.
    #[test]
    fn an_rtl_block_takes_its_insets_from_the_right_edge() {
        let line = |width| InlineLine {
            range: 0..2,
            y: 0.0,
            height: 20.0,
            baseline: 15.0,
            width,
            ellipsis: false,
            runs: Vec::new(),
            atoms: Vec::new(),
        };
        let layout = InlineLayout {
            lines: vec![line(30.0), line(50.0)],
            width: 80.0,
            height: 40.0,
            baseline: 15.0,
            rtl: true,
        };
        let origin = crate::geom::Point::new(0.0, 0.0);
        let at = |index| {
            layout
                .line_origin(
                    index,
                    origin,
                    40.0,
                    200.0,
                    LineEdges {
                        first: 10.0,
                        body: 30.0,
                    },
                    TextAlign::Start,
                )
                .x
        };
        assert_eq!(
            at(0),
            160.0,
            "the first line clears the indent at the right"
        );
        assert_eq!(at(1), 120.0, "every later line clears the hanging inset");
    }

    #[test]
    fn a_masked_text_keeps_every_character_boundary_where_it_was() {
        // The whole reason masking needs no index map: a caret at byte 4 of the buffer can
        // be drawn at byte 4 of the marks, whatever the characters cost in UTF-8.
        for text in ["hunter2", "é", "密码", "\u{1f600}a"] {
            let marks = masked(text);
            assert_eq!(
                marks.len(),
                text.len(),
                "{text:?} must keep its byte length"
            );
            for (at, _) in text.char_indices() {
                assert!(marks.is_char_boundary(at), "byte {at} of {text:?} moved");
            }
        }
    }

    #[test]
    fn flatten_joins_runs_and_covers_every_byte() {
        let block = TextBlock {
            runs: vec![
                TextRun::new("ab", style(10.0)),
                TextRun::new("cd", style(20.0)),
            ],
            align: TextAlign::Start,
            max_lines: None,
            line_gap: 0.0,
            indent: 0.0,
            hanging: 0.0,
            atoms: Vec::new(),
            input_base: None,
            line_numbers: None,
            structures: Vec::new(),
        };
        let (text, spans) = block.flatten();
        assert_eq!(text, "abcd");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].range, 0..2);
        assert_eq!(spans[1].range, 2..4);
        // Contiguous and non-overlapping: the table is a partition of the text.
        assert_eq!(spans[1].range.start, spans[0].range.end);
        assert_eq!(spans[1].range.end, text.len());
    }

    /// A run boundary inside a cluster moves out to the cluster's edge, so the mark keeps
    /// its base's style: no shaper is handed a mark without the base it attaches to.
    #[test]
    fn a_run_boundary_inside_a_cluster_snaps_to_its_edge() {
        let block = TextBlock {
            runs: vec![
                TextRun::new("e", style(10.0)),
                TextRun::new("\u{301}x", style(20.0)),
            ],
            align: TextAlign::Start,
            max_lines: None,
            line_gap: 0.0,
            indent: 0.0,
            hanging: 0.0,
            atoms: Vec::new(),
            input_base: None,
            line_numbers: None,
            structures: Vec::new(),
        };
        let (text, spans) = block.flatten();
        assert_eq!(text, "e\u{301}x");
        assert_eq!(spans[0].range, 0..3, "the mark stays with its base");
        assert_eq!(spans[1].range, 3..4);
    }

    /// A rule changes ink exactly where the glyphs do: the selected part is its own piece,
    /// and the run's ink resumes after it.
    #[test]
    fn a_decoration_is_cut_where_the_selection_starts_and_ends() {
        let selection = (4..7, Color::WHITE);
        assert_eq!(
            decoration_pieces(0..10, Some(&selection)),
            vec![(0..4, false), (4..7, true), (7..10, false)]
        );
        assert_eq!(
            decoration_pieces(2..6, Some(&selection)),
            vec![(2..4, false), (4..6, true)],
            "a run that starts before the selection keeps its ink up to the boundary"
        );
        assert_eq!(
            decoration_pieces(7..10, Some(&selection)),
            vec![(7..10, false)],
            "a slice past the selection is one unselected piece"
        );
        assert_eq!(
            decoration_pieces(0..10, None),
            vec![(0..10, false)],
            "no selection keeps one piece"
        );
    }

    /// The first line gives up `indent` and every later one `hanging`: the two edges a
    /// marker opens, so a wrapped item clears its marker instead of sliding under it.
    #[test]
    fn a_hanging_indent_moves_every_line_after_the_first() {
        let line = |range| InlineLine {
            range,
            y: 0.0,
            height: 20.0,
            baseline: 15.0,
            width: 50.0,
            ellipsis: false,
            runs: Vec::new(),
            atoms: Vec::new(),
        };
        let layout = InlineLayout {
            lines: vec![line(0..2), line(2..4)],
            width: 80.0,
            height: 40.0,
            baseline: 15.0,
            rtl: false,
        };
        let origin = crate::geom::Point::new(0.0, 0.0);
        let at = |index| {
            layout
                .line_origin(
                    index,
                    origin,
                    40.0,
                    200.0,
                    LineEdges {
                        first: 10.0,
                        body: 30.0,
                    },
                    TextAlign::Start,
                )
                .x
        };
        assert_eq!(at(0), 10.0, "the first line takes the indent");
        assert_eq!(at(1), 30.0, "every later line takes the hanging indent");
    }

    /// Every style fills the same band under the rule's own top edge; the styles differ in
    /// the boxes they put there, not in where the band sits.
    #[test]
    fn each_decoration_style_fills_the_same_band_in_its_own_way() {
        use crate::geom::Rect;
        assert_eq!(
            decoration_rects(Decoration::Single, 10.0, 100.0, 40.0, 1.0, 10.0),
            vec![Rect::new(10.0, 40.0, 100.0, 1.0)]
        );
        let double = decoration_rects(Decoration::Double, 10.0, 100.0, 40.0, 1.0, 10.0);
        assert_eq!(
            double.len(),
            2,
            "a double rule is two lines, not one thick one"
        );
        assert_eq!(double[0].origin.y, 40.0);
        assert_eq!(
            double[1].origin.y, 42.0,
            "the pair grows away from the glyphs"
        );

        let dotted = decoration_rects(Decoration::Dotted, 10.0, 60.0, 40.0, 1.0, 10.0);
        assert!(dotted.len() >= 3, "dashes, not one rule");
        assert!(
            dotted
                .iter()
                .all(|r| r.origin.x >= 10.0 && r.max_x() <= 70.01 && r.size.w <= 1.01),
            "every dot is one thickness wide and inside the run"
        );

        let wavy = decoration_rects(Decoration::Wavy, 10.0, 120.0, 40.0, 1.0, 10.0);
        let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
        for rect in &wavy {
            low = low.min(rect.origin.y);
            high = high.max(rect.origin.y);
            assert!(rect.origin.x >= 10.0 && rect.max_x() <= 130.01);
        }
        assert!(high - low > 0.5, "the wave leaves the rule's own line");
        assert!(
            wavy.len() <= 61,
            "one step per two thicknesses, not per pixel"
        );

        // A piece cut out of a run keeps the run's phase: the wave it starts with is the
        // one the unbroken run would have drawn at that offset.
        let whole = decoration_rects(Decoration::Wavy, 0.0, 100.0, 40.0, 1.0, 0.0);
        let cut = decoration_rects(Decoration::Wavy, 50.0, 50.0, 40.0, 1.0, 0.0);
        let at_cut = whole
            .iter()
            .find(|r| (r.origin.x - 50.0).abs() < 0.01)
            .map(|r| r.origin.y);
        assert_eq!(
            cut.first().map(|r| r.origin.y),
            at_cut,
            "a cut wave continues the run's phase instead of restarting it"
        );
        assert!(decoration_rects(Decoration::None, 0.0, 10.0, 0.0, 1.0, 0.0).is_empty());
    }
}
