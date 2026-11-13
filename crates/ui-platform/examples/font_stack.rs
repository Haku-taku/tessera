//! Show the font stack resolving real text, with and without system fonts.

use ui_text::{FontRequest, FontStack, GenericFamily, Script, StaticProvider};

fn main() {
    // 1. A deterministic provider: proves the abstraction works with no system fonts.
    let static_provider = StaticProvider::new("static")
        .with_partial_face(
            "Latin Only",
            0.55,
            &['a', 'b', 'c', 'H', 'e', 'l', 'o', ' ', 'w', 'r', 'd'],
        )
        .with_partial_face("CJK Only", 0.95, &['中', '文', '字', '体', ' ', '测', '试']);
    let mut stack = FontStack::new(static_provider);

    println!("== provider: {} ==", stack.provider_name());
    for text in ["Hello world", "中文字体", "Hello 中文"] {
        let r = stack.resolve_for_text(text, 14.0);
        println!(
            "  {text:14} face={} width={:6.1} steps={} outcome={:?}",
            r.face.0, r.width, r.fallback_steps, r.outcome
        );
    }
    println!(
        "  stats: resolves={} fallbacks={} rejected={} unresolved={}",
        stack.stats.resolves, stack.stats.fallbacks, stack.stats.rejected, stack.stats.unresolved
    );

    // 3. Query a named family, showing the request shape.
    let req = FontRequest::new(GenericFamily::Monospace)
        .family("Latin Only")
        .script(Script::HAN);
    println!("\n  explicit request -> {req:?}");
}
