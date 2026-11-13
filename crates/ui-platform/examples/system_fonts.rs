//! Enumerate and exercise the real OS font stack through the provider trait.

use ui_text::fallback::FontStack;
use ui_text::{FontProvider, FontRequest, GenericFamily, Script};
use ui_text_fontique::{describe_generics, system_or_isolated};

fn main() {
    let provider = system_or_isolated();
    println!("provider: {}", provider.name());
    if !provider.has_system_fonts() {
        println!("no system fonts available; nothing to show");
        return;
    }

    let names = provider.family_names();
    println!("families: {}", names.len());
    println!("first 8: {:?}", &names[..names.len().min(8)]);

    println!("\n-- generic families --");
    for (g, name) in describe_generics(&provider) {
        println!("  {g:?} -> {name:?}");
    }

    println!("\n-- explicit family lookup (exact, no alias) --");
    for wanted in ["MiSans VF", "Noto Sans CJK SC", "Arial", "NoSuchFamily"] {
        let req = FontRequest::new(GenericFamily::SansSerif).family(wanted);
        match provider.resolve(&req) {
            Some(face) => println!(
                "  {wanted:20} -> face {} ({:?})",
                face.0,
                provider.face_family_name(face)
            ),
            None => println!("  {wanted:20} -> not found (falls back to generic)"),
        }
    }

    println!("\n-- fallback chain per script --");
    for script in [Script::LATIN, Script::HAN, Script::ARABIC, Script(*b"Thaa")] {
        let mut chain = provider.fallback_chain(script);
        chain.truncate(3);
        let named: Vec<String> = chain
            .iter()
            .filter_map(|f| provider.face_family_name(*f))
            .collect();
        println!("  {:<6} -> {named:?}", script.tag());
    }

    println!("\n-- glyph coverage of a resolved face --");
    let face = provider
        .resolve(&FontRequest::new(GenericFamily::SansSerif))
        .expect("a sans-serif face");
    for c in ['a', 'Z', '中', 'م', '\u{1F980}'] {
        println!("  {c:?} covered: {}", provider.has_glyph(face, c));
    }

    println!("\n-- through FontStack (fallback decisions) --");
    let mut stack = FontStack::new(system_or_isolated());
    for text in ["Hello", "中文字体", "Hello 中文", "مرحبا"] {
        let r = stack.resolve_for_text(text, 14.0);
        println!(
            "  {text:12} face={} width={:6.1} outcome={:?}",
            r.face.0, r.width, r.outcome
        );
    }
    println!(
        "  stats: resolves={} fallbacks={} unresolved={}",
        stack.stats.resolves, stack.stats.fallbacks, stack.stats.unresolved
    );
}
