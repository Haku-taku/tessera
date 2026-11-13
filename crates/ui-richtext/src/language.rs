//! Presentation names for code fence info strings.

/// The canonical name for a fence language, or `name` untouched when unknown.
///
/// An empty result means no language was given; the caller supplies its own wording.
pub fn language_label(name: &str) -> String {
    let key = name.trim().to_lowercase();
    if key.is_empty() {
        return String::new();
    }
    let canonical = match key.as_str() {
        "js" | "javascript" => "JavaScript",
        "ts" | "typescript" => "TypeScript",
        "rs" | "rust" => "Rust",
        "py" | "python" => "Python",
        "sh" | "bash" | "shell" => "Shell",
        "c" => "C",
        "cpp" | "c++" | "cxx" => "C++",
        "cs" | "csharp" | "c#" => "C#",
        "go" => "Go",
        "java" => "Java",
        "kt" | "kotlin" => "Kotlin",
        "swift" => "Swift",
        "rb" | "ruby" => "Ruby",
        "php" => "PHP",
        "sql" => "SQL",
        "html" => "HTML",
        "css" => "CSS",
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "xml" => "XML",
        "md" | "markdown" => "Markdown",
        "lua" => "Lua",
        "zig" => "Zig",
        "dart" => "Dart",
        "ex" | "elixir" => "Elixir",
        "hs" | "haskell" => "Haskell",
        "scala" => "Scala",
        "r" => "R",
        "pl" | "perl" => "Perl",
        "diff" => "Diff",
        "ini" => "INI",
        "tex" | "latex" => "LaTeX",
        _ => return name.to_owned(),
    };
    canonical.to_owned()
}

#[cfg(test)]
mod tests {
    use super::language_label;

    #[test]
    fn aliases_case_and_unknowns_map_to_display_names() {
        assert_eq!(language_label("js"), "JavaScript");
        assert_eq!(language_label(" JavaScript "), "JavaScript");
        assert_eq!(language_label("CPP"), "C++");
        assert_eq!(language_label("c#"), "C#");
        assert_eq!(language_label("yml"), "YAML");
        assert_eq!(language_label("Latex"), "LaTeX");
        assert_eq!(language_label("brainfuck"), "brainfuck");
        assert_eq!(language_label("  brainfuck  "), "  brainfuck  ");
        assert_eq!(language_label(""), "");
        assert_eq!(language_label("   "), "");
    }
}
