//! The chain against a real face: LaTeX in, display list out, and a non-math font refused.

use ui_math::{Error, MathFace, MathItem, probe};
use ui_text::provider::{FontProvider, FontRequest, GenericFamily};
use ui_text_fontique::FontiqueProvider;

/// A system family whose name says it is a math font, with its bytes and face index.
fn system_math_face() -> (Vec<u8>, u32) {
    let provider = FontiqueProvider::new_system();
    let family = provider
        .family_names()
        .into_iter()
        .find(|name| name.to_lowercase().contains("math"))
        .expect(
            "a system family with a MATH table; install one such as STIX Two Math, \
             Latin Modern Math or DejaVu Math TeX Gyre",
        );
    let request = FontRequest::new(GenericFamily::SystemUi).family(family.clone());
    let face = provider
        .resolve(&request)
        .unwrap_or_else(|| panic!("{family} resolves to a face"));
    let data = provider
        .face_data(face)
        .unwrap_or_else(|| panic!("{family}'s bytes load"));
    (data.bytes().to_vec(), provider.face_index(face) as u32)
}

/// A fraction is taller than a letter, sets its scripts smaller, and draws one bar across
/// its own width — the geometry the MATH table drives, so a broken font shows up here.
#[test]
fn a_fraction_adds_depth_and_a_bar() {
    let (bytes, index) = system_math_face();
    let face = MathFace::new(&bytes, index).expect("the chosen family is a math font");

    let plain = face.layout("x", 16.0, false).expect("a letter lays out");
    let fraction = face
        .layout(r"\frac{x}{y}", 16.0, false)
        .expect("a fraction lays out");

    assert!(
        fraction.ascent + fraction.descent > plain.ascent + plain.descent,
        "a stacked fraction is taller than one letter: {} vs {}",
        fraction.ascent + fraction.descent,
        plain.ascent + plain.descent
    );
    assert!(
        fraction.descent > plain.descent,
        "the denominator hangs below the baseline: {} vs {}",
        fraction.descent,
        plain.descent
    );
    let size = 16.0;
    let glyphs: Vec<(f32, f32)> = fraction
        .items
        .iter()
        .filter_map(|item| match item {
            MathItem::Glyph { size, advance, .. } => Some((*size, *advance)),
            _ => None,
        })
        .collect();
    assert_eq!(glyphs.len(), 2, "numerator and denominator are the glyphs");
    for (glyph_size, _) in &glyphs {
        assert!(
            *glyph_size < size,
            "a script is set smaller than the base size: {glyph_size} vs {size}"
        );
    }
    let bar = fraction
        .items
        .iter()
        .find_map(|item| match item {
            MathItem::Rule { w, .. } => Some(*w),
            _ => None,
        })
        .expect("a fraction draws its bar");
    assert!(
        (bar - fraction.width).abs() < 0.01,
        "the bar spans the fraction: {bar} vs {}",
        fraction.width
    );
}

/// `\cancel` is a stroked diagonal in the display list, not a dropped item: the engine now
/// has a line primitive for it.
#[test]
fn a_cancel_draws_its_diagonal() {
    let (bytes, index) = system_math_face();
    let face = MathFace::new(&bytes, index).expect("the chosen family is a math font");

    let formula = face
        .layout(r"\cancel{x}", 16.0, false)
        .expect("a cancelled letter lays out");
    let line = formula
        .items
        .iter()
        .find_map(|item| match item {
            MathItem::Line {
                thickness, color, ..
            } => Some((*thickness, *color)),
            _ => None,
        })
        .expect("the cancellation line survives conversion");
    assert!(line.0 > 0.0, "the diagonal has a width: {}", line.0);
    assert!(line.1.is_none(), "it takes the formula's ink: {:?}", line.1);
}

/// A mistyped command inside a fraction leaves the fraction standing: the refusal becomes a
/// marked run in the denominator, and the frame the author is editing survives.
#[test]
fn a_mistyped_command_keeps_the_structure_it_sits_in() {
    let (bytes, index) = system_math_face();
    let face = MathFace::new(&bytes, index).expect("the chosen family is a math font");

    let formula = face
        .layout_recovering(r"\frac{1}{1+\frc{1}{n}}", 16.0, false, None)
        .expect("a source with a typo still lays out");

    assert_eq!(
        formula.errors.len(),
        1,
        "only the unknown command is marked"
    );
    let bar = formula
        .items
        .iter()
        .filter_map(|item| match item {
            MathItem::Rule { w, .. } => Some(*w),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    assert!(
        (bar - formula.width).abs() < 0.01,
        "the fraction still draws its bar across the formula: {bar} vs {}",
        formula.width
    );
    let mark = &formula.errors[0];
    assert!(
        mark.x > 0.0 && mark.x + mark.w <= formula.width + 0.01,
        "the marked run sits inside the formula: {mark:?}"
    );
    assert!(
        mark.y + mark.h > 0.0,
        "the denominator's refusal reaches below the baseline: {mark:?}"
    );
    assert!(
        !mark.message.is_empty(),
        "the run carries the parser's reason"
    );
}

/// A source the author is still typing keeps the structure above the open group: the missing
/// closer and argument are supplied, not reported as a failure.
#[test]
fn an_unclosed_group_still_lays_out_the_structure_above_it() {
    let (bytes, index) = system_math_face();
    let face = MathFace::new(&bytes, index).expect("the chosen family is a math font");

    let formula = face
        .layout_recovering(r"\frac{1}{1+", 16.0, false, None)
        .expect("an incomplete source lays out");

    assert!(
        formula.errors.is_empty(),
        "an incomplete group is repaired, not marked"
    );
    let bar = formula
        .items
        .iter()
        .filter_map(|item| match item {
            MathItem::Rule { w, .. } => Some(*w),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    assert!(
        (bar - formula.width).abs() < 0.01,
        "the fraction is still there: {bar} vs {}",
        formula.width
    );
}

/// The gallery picks a family by name and asks this first, so a font without the table has
/// to be answerable without laying anything out.
#[test]
fn a_font_without_a_math_table_is_refused() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../assets/fonts/InterVariable.ttf"
    );
    let bytes = std::fs::read(path).expect("the bundled text face reads");
    assert!(!probe(&bytes, 0), "Inter has no MATH table");
    assert!(
        matches!(MathFace::new(&bytes, 0), Err(Error::Font(_))),
        "a text face is refused, not laid out"
    );
}

/// The editor's rects are in the formula's space: a click at an empty slot's centre puts
/// the caret in that slot, which is only true when the baseline offset is applied.
#[test]
fn a_click_in_a_slot_lands_that_slot() {
    let (bytes, index) = system_math_face();
    let face = MathFace::new(&bytes, index).expect("the chosen family is a math font");
    let mut editor = face.editor(r"\frac{}{}", 16.0, true);

    let formula = editor.formula(None);
    let slots = editor.placeholders();
    assert_eq!(slots.len(), 2, "an empty fraction has two slots: {slots:?}");
    for slot in slots {
        let caret = editor
            .click(slot.x + slot.w * 0.5, slot.y + slot.h * 0.5, false)
            .then(|| editor.caret())
            .flatten()
            .expect("a click in the slot leaves a caret");
        assert!(
            caret.top <= slot.y + slot.h && caret.bottom >= slot.y,
            "the caret stands in the clicked slot: {caret:?} vs {slot:?}"
        );
        assert!(
            caret.top >= -formula.ascent - 0.5 && caret.bottom <= formula.descent + 0.5,
            "the caret stays inside the formula: {caret:?}"
        );
    }
}

/// A source the host replaces keeps the parser's refusal, so the edit view marks the run
/// with the same pass a read-only formula uses.
#[test]
fn a_refused_source_is_marked_in_the_edited_formula() {
    let (bytes, index) = system_math_face();
    let face = MathFace::new(&bytes, index).expect("the chosen family is a math font");
    let mut editor = face.editor("x", 16.0, false);
    editor.set_source(&face, r"\frc");

    let formula = editor.formula(None);
    assert_eq!(formula.errors.len(), 1, "the unknown command is marked");
    assert!(
        formula.errors[0].w > 0.0,
        "the mark covers the refused run: {:?}",
        formula.errors[0]
    );
}
