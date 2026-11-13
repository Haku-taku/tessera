//! Icon primitives: a glyph source described independently of any icon set.

use ui_core::geom::Color;
use ui_core::{AxisSize, Decl, Family, Id, TextAlign, Variations, Weight};

use crate::text::Text;

/// The three Material Symbols families, i.e. the shape of the icon set.
///
/// This stays as the convenience entrance for Material Symbols; an icon itself no longer
/// carries one — it carries a `Family` instead, so any glyph font can drive the slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconSet {
    #[default]
    Outlined,
    Rounded,
    Sharp,
}

impl IconSet {
    /// The family name the font provider resolves.
    pub const fn family(self) -> &'static str {
        match self {
            Self::Outlined => "Material Symbols Outlined",
            Self::Rounded => "Material Symbols Rounded",
            Self::Sharp => "Material Symbols Sharp",
        }
    }
}

/// The family the vendored SF Symbols font must be registered under.
pub const SF_SYMBOLS_FAMILY: &str = "SFSymbols";

/// What fills an icon slot: one glyph in a family, or an app-drawn mark.
///
/// This is the source *description* the slot stores — the same role Flutter's `IconData`,
/// GPUI's `IconSource` and Qt's `icon.name` play. It is `Copy`, so controls can hold it and
/// re-resolve it per state: a selected destination resolves its `FILL` axis to 1, an
/// unselected one to 0, without the caller swapping objects.
#[derive(Clone, Copy, Debug)]
pub enum IconSource {
    /// One glyph in a named family plus the variable axes that shape it.
    Glyph {
        family: Family,
        glyph: char,
        /// `FILL` axis: 0 outlined, 1 filled; fractional values animate.
        fill: f32,
        /// `wght` axis: stroke width, not a face choice.
        weight: Weight,
        /// `GRAD` axis: emphasis, typically -50..200.
        grade: f32,
    },
    /// An app-drawn mark: given a slot id and its ink, produce the declaration.
    Custom(fn(Id, Color) -> Decl),
}

impl IconSource {
    /// A Material Symbols glyph by name, resolved to its PUA codepoint.
    pub fn glyph(name: &str, set: IconSet) -> Self {
        Self::Glyph {
            family: Family::Named(set.family()),
            glyph: lookup(name),
            fill: 0.0,
            weight: Weight::REGULAR,
            grade: 0.0,
        }
    }

    /// An SF Symbols glyph by Apple's name; `.` and `_` are interchangeable, so
    /// `house.fill` and `house_fill` both land.
    pub fn sf(name: &str) -> Self {
        Self::from_parts(Family::Named(SF_SYMBOLS_FAMILY), sf_lookup(name))
    }

    /// Any glyph at a raw codepoint in `family` — the custom-font entrance.
    pub fn from_parts(family: Family, glyph: char) -> Self {
        Self::Glyph {
            family,
            glyph,
            fill: 0.0,
            weight: Weight::REGULAR,
            grade: 0.0,
        }
    }

    /// Builder: the `FILL` axis; the slot resolves it per state.
    pub fn fill(mut self, fill: f32) -> Self {
        if let Self::Glyph { fill: f, .. } = &mut self {
            *f = fill;
        }
        self
    }

    /// Builder: stroke width, per the `wght` axis.
    pub fn weight(mut self, weight: Weight) -> Self {
        if let Self::Glyph { weight: w, .. } = &mut self {
            *w = weight;
        }
        self
    }

    /// Builder: emphasis, per the `GRAD` axis.
    pub fn grade(mut self, grade: f32) -> Self {
        if let Self::Glyph { grade: g, .. } = &mut self {
            *g = grade;
        }
        self
    }

    /// Resolve into `id`'s slot at the MD3 icon size.
    pub fn resolve(&self, id: Id, ink: Color) -> Decl {
        self.resolve_with_size(id, ink, 24.0)
    }

    /// Resolve into `id`'s slot at `size` points, in a `size`-square em box.
    pub fn resolve_with_size(&self, id: Id, ink: Color, size: f32) -> Decl {
        match *self {
            Self::Glyph {
                family,
                glyph,
                fill,
                weight,
                grade,
            } => {
                let variations = Variations::NONE.fill(fill).optical_size(size).grade(grade);
                let mut decl = Text::new(id, glyph.to_string(), size, ink)
                    // A face may advance less than one em per glyph (Segoe Fluent Icons
                    // draws its narrow marks on a 0.6 em canvas); centre the glyph's
                    // advance in the slot instead of leaving it at the pen origin.
                    .align(TextAlign::Center)
                    .family(family)
                    .weight(weight)
                    .variations(variations)
                    .resolve();
                // An icon occupies its em box, not a text line box: a 1.4 em slot hangs the
                decl.size_class.width = AxisSize::fixed(size);
                decl.size_class.height = AxisSize::fixed(size);
                decl
            }
            Self::Custom(build) => build(id, ink),
        }
    }
}

/// One icon in a slot: a source, a size and an ink.
///
/// The glyph is addressed by codepoint rather than by ligature name, so drawing it needs
/// no shaping features and its advance is exactly one em — Flutter's `Icons` does the same.
#[derive(Clone, Copy, Debug)]
pub struct Icon {
    pub id: Id,
    pub source: IconSource,
    pub size: f32,
    pub ink: Color,
}

impl Icon {
    /// The Material Symbols icon named `name`; an unknown name panics instead of drawing
    /// `.notdef`. A convenience over `IconSource::glyph` for callers that want one line.
    pub fn new(id: Id, name: &str, size: f32, ink: Color) -> Self {
        Self {
            id,
            source: IconSource::glyph(name, IconSet::Outlined),
            size,
            ink,
        }
    }

    /// The icon at a raw PUA codepoint, for names the generated table does not carry.
    pub fn from_char(id: Id, glyph: char, size: f32, ink: Color) -> Self {
        Self {
            id,
            source: IconSource::from_parts(Family::Named(IconSet::Outlined.family()), glyph),
            size,
            ink,
        }
    }

    /// Builder: Outlined / Rounded / Sharp.
    pub fn set(mut self, set: IconSet) -> Self {
        if let IconSource::Glyph { family, .. } = &mut self.source {
            *family = Family::Named(set.family());
        }
        self
    }

    /// Builder: filled or outlined, per the `FILL` axis; fractional values animate.
    pub fn fill(mut self, fill: f32) -> Self {
        self.source = self.source.fill(fill);
        self
    }

    /// Builder: stroke width, per the `wght` axis.
    pub fn weight(mut self, weight: Weight) -> Self {
        self.source = self.source.weight(weight);
        self
    }

    /// Builder: emphasis, per the `GRAD` axis.
    pub fn grade(mut self, grade: f32) -> Self {
        self.source = self.source.grade(grade);
        self
    }

    /// The box is the glyph's em square: the font centres the outline inside it, and a
    /// larger slot centres that box with `stack_align`, never with padding.
    pub fn resolve(&self) -> Decl {
        self.source.resolve_with_size(self.id, self.ink, self.size)
    }
}

/// The PUA codepoint for `name`, from the generated table.
fn lookup(name: &str) -> char {
    match crate::icon_codepoints::ICONS.binary_search_by_key(&name, |(n, _)| *n) {
        Ok(index) => crate::icon_codepoints::ICONS[index].1,
        Err(_) => panic!("unknown Material Symbols icon: {name}"),
    }
}

/// The codepoint for an SF Symbols name, accepting Apple's dotted spelling.
fn sf_lookup(name: &str) -> char {
    let table = crate::sf_symbol_codepoints::SF_SYMBOLS;
    let found = |n: &str| table.binary_search_by_key(&n, |(k, _)| *k).ok();
    let index = found(name).or_else(|| found(&name.replace('.', "_")));
    match index {
        Some(i) => table[i].1,
        None => panic!("unknown SF Symbols icon: {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::NodeData;

    #[test]
    fn every_name_in_the_generated_table_is_found_by_the_binary_search() {
        // Binary search needs the table sorted; an unsorted or truncated table would still
        // compile and silently stop finding names near the ends.
        let table = crate::icon_codepoints::ICONS;
        for (name, glyph) in table {
            assert_eq!(lookup(name), *glyph, "{name}");
        }
    }

    /// The slot's glyph, laid out exactly as the engine lays an icon out.
    fn icon_glyph(
        pass: &mut ui_platform::TextPass,
        source: IconSource,
        size: f32,
        ink: Color,
    ) -> ui_core::draw::GlyphPrim {
        let decl = source.resolve_with_size(Id::str("icon"), ink, size);
        let NodeData::Text { block, .. } = decl.data else {
            panic!("an icon glyph must resolve to text");
        };
        let mut list = ui_core::DrawList::new(ui_core::geom::Rect::new(0.0, 0.0, size, size));
        list.push_text(
            ui_core::NodeId::from_raw(1, 0),
            ui_core::draw::TextPrim {
                block,
                origin: ui_core::geom::Point::new(0.0, 0.0),
                ink,
                max_width: size,
                layout: None,
                height: size,
                caret: None,
                selection: None,
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let out = pass.resolve(&list, 1.0);
        let mut glyphs = out.prims.glyphs;
        assert_eq!(glyphs.len(), 1, "an icon is one glyph");
        glyphs.remove(0)
    }

    #[test]
    fn an_icon_is_centred_by_its_advance_and_a_full_em_glyph_does_not_move() {
        // Third-party icon sets do not all draw on the em box: Segoe Fluent Icons' narrow
        // marks advance 0.6 em. A text face's narrow glyph has the same geometry without
        // needing that font installed, so each direction is checked on a bundled face.
        let mut pass = ui_platform::TextPass::new();
        let inter: std::sync::Arc<[u8]> = include_bytes!("../../assets/fonts/InterVariable.ttf")
            .as_slice()
            .into();
        assert!(
            pass.register("Test Narrow Icons", inter),
            "the bundled face must register"
        );
        let narrow = icon_glyph(
            &mut pass,
            IconSource::from_parts(Family::Named("Test Narrow Icons"), 'l'),
            32.0,
            Color::WHITE,
        );
        let centre = narrow.rect.origin.x + narrow.rect.size.w * 0.5;
        assert!(
            (centre - 16.0).abs() <= 2.0,
            "a sub-em glyph must land in the middle of the slot, got {centre}"
        );

        let sf: std::sync::Arc<[u8]> = include_bytes!("../../assets/fonts/SFSymbols.ttf")
            .as_slice()
            .into();
        assert!(
            pass.register(SF_SYMBOLS_FAMILY, sf),
            "the bundled face must register"
        );
        let full = icon_glyph(&mut pass, IconSource::sf("house"), 32.0, Color::WHITE);
        let centre = full.rect.origin.x + full.rect.size.w * 0.5;
        assert!(
            (centre - 16.0).abs() <= 1.0,
            "a one-em glyph must stay where the pen put it, got {centre}"
        );
    }

    #[test]
    fn an_icon_in_a_colour_font_paints_the_fonts_own_colours() {
        // A colour icon (COLR or bitmap layers) must reach the colour page, where the
        // painter ignores the slot's ink; a coverage glyph would draw it as one flat tint.
        let mut pass = ui_platform::TextPass::new();
        let mut font = None;
        for path in [
            "/usr/share/fonts/TTF/seguiemj.ttf",
            "/usr/share/fonts/twemoji/twemoji.ttf",
            "/usr/share/fonts/apple-color-emoji/apple-color-emoji.ttf",
        ] {
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let bytes: std::sync::Arc<[u8]> = bytes.into();
            if pass.register("Test Colour Icons", bytes) {
                font = Some(path);
                break;
            }
        }
        if font.is_none() {
            eprintln!("skipped: no colour font on this machine");
            return;
        }
        let source = IconSource::from_parts(Family::Named("Test Colour Icons"), '\u{1F600}');
        let glyph = icon_glyph(&mut pass, source, 32.0, Color::rgba(1.0, 0.0, 0.0, 1.0));
        assert_eq!(
            glyph.kind,
            ui_core::draw::GlyphKind::Color,
            "a colour icon must not be drawn from its coverage"
        );
    }

    #[test]
    fn a_custom_mark_resolves_through_the_custom_branch() {
        // A `Custom` source ignores the axes and calls its builder — the escape hatch that
        // keeps the slot open to anything the app can paint.
        let src = IconSource::Custom(|id, ink| {
            Decl::container(id, ui_core::NodeKind::Box).shapes(vec![ui_core::draw::Shape::Box {
                corners: ui_core::Corners::all(0.0),
                fill: ink,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }])
        });
        let decl = src.fill(1.0).resolve(Id::str("c"), Color::WHITE);
        // A custom mark resolves to whatever its builder produced — here a container.
        assert!(matches!(decl.data, NodeData::Container(_)));
    }
}
