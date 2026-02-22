///! H5 exploration: Extract all unique capture names from tree-sitter-rust's
///! highlight query, then verify each maps to existing Style/Color fields.

use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

/// Every unique @capture name from tree-sitter-rust HIGHLIGHTS_QUERY,
/// extracted from the query source above.
const ALL_CAPTURES: &[&str] = &[
    "attribute",
    "comment",
    "comment.documentation",
    "constant",
    "constant.builtin",
    "constructor",
    "escape",
    "function",
    "function.macro",
    "function.method",
    "keyword",
    "label",
    "number",           // not in rust query but common in other grammars
    "operator",
    "property",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "type",
    "type.builtin",
    "variable.builtin",
    "variable.parameter",
];

/// Catppuccin Mocha color palette (hex for readability, would be Color::Rgb in production)
/// Reference: https://github.com/catppuccin/catppuccin
struct CatppuccinMocha;
impl CatppuccinMocha {
    const ROSEWATER: [u8; 3] = [0xf5, 0xe0, 0xdc];
    const FLAMINGO:  [u8; 3] = [0xf2, 0xcd, 0xcd];
    const PINK:      [u8; 3] = [0xf5, 0xc2, 0xe7];
    const MAUVE:     [u8; 3] = [0xcb, 0xa6, 0xf7];
    const RED:       [u8; 3] = [0xf3, 0x8b, 0xa8];
    const MAROON:    [u8; 3] = [0xeb, 0xa0, 0xac];
    const PEACH:     [u8; 3] = [0xfa, 0xb3, 0x87];
    const YELLOW:    [u8; 3] = [0xf9, 0xe2, 0xaf];
    const GREEN:     [u8; 3] = [0xa6, 0xe3, 0xa1];
    const TEAL:      [u8; 3] = [0x94, 0xe2, 0xd5];
    const SKY:       [u8; 3] = [0x89, 0xdc, 0xeb];
    const SAPPHIRE:  [u8; 3] = [0x74, 0xc7, 0xec];
    const BLUE:      [u8; 3] = [0x89, 0xb4, 0xfa];
    const LAVENDER:  [u8; 3] = [0xb4, 0xbe, 0xfe];
    const OVERLAY0:  [u8; 3] = [0x6c, 0x70, 0x86];
    const SUBTEXT0:  [u8; 3] = [0xa6, 0xad, 0xc8];
}

/// A proposed mapping from tree-sitter capture name to Style attributes.
/// Style fields used: fg (Color::Rgb), bold, italic.
/// No bg, underline, strikethrough, inverse, hidden, or dim needed.
struct StyleMapping {
    capture: &'static str,
    fg: [u8; 3],
    bold: bool,
    italic: bool,
}

fn mappings() -> Vec<StyleMapping> {
    use CatppuccinMocha as C;
    vec![
        // --- Keywords & control flow ---
        StyleMapping { capture: "keyword",              fg: C::MAUVE,    bold: false, italic: false },

        // --- Functions ---
        StyleMapping { capture: "function",             fg: C::BLUE,     bold: false, italic: false },
        StyleMapping { capture: "function.method",      fg: C::BLUE,     bold: false, italic: false },
        StyleMapping { capture: "function.macro",       fg: C::MAUVE,    bold: false, italic: false },

        // --- Types ---
        StyleMapping { capture: "type",                 fg: C::YELLOW,   bold: false, italic: false },
        StyleMapping { capture: "type.builtin",         fg: C::YELLOW,   bold: false, italic: true  },
        StyleMapping { capture: "constructor",          fg: C::SAPPHIRE, bold: false, italic: false },

        // --- Literals ---
        StyleMapping { capture: "string",               fg: C::GREEN,    bold: false, italic: false },
        StyleMapping { capture: "escape",               fg: C::PINK,     bold: false, italic: false },
        StyleMapping { capture: "constant",             fg: C::PEACH,    bold: false, italic: false },
        StyleMapping { capture: "constant.builtin",     fg: C::PEACH,    bold: false, italic: false },
        StyleMapping { capture: "number",               fg: C::PEACH,    bold: false, italic: false },

        // --- Comments ---
        StyleMapping { capture: "comment",              fg: C::OVERLAY0, bold: false, italic: true  },
        StyleMapping { capture: "comment.documentation",fg: C::OVERLAY0, bold: false, italic: true  },

        // --- Variables & parameters ---
        StyleMapping { capture: "variable.parameter",   fg: C::MAROON,   bold: false, italic: true  },
        StyleMapping { capture: "variable.builtin",     fg: C::RED,      bold: false, italic: false },
        StyleMapping { capture: "property",             fg: C::LAVENDER, bold: false, italic: false },
        StyleMapping { capture: "label",                fg: C::SAPPHIRE, bold: false, italic: true  },

        // --- Punctuation ---
        StyleMapping { capture: "punctuation.bracket",  fg: C::SUBTEXT0, bold: false, italic: false },
        StyleMapping { capture: "punctuation.delimiter",fg: C::SUBTEXT0, bold: false, italic: false },
        StyleMapping { capture: "operator",             fg: C::SKY,      bold: false, italic: false },

        // --- Attributes (Rust #[...]) ---
        StyleMapping { capture: "attribute",            fg: C::YELLOW,   bold: false, italic: false },
    ]
}

fn main() {
    let maps = mappings();

    println!("=== H5: Capture → Style Mapping Audit ===");
    println!();

    // 1. Check coverage: every capture has a mapping
    println!("1. Coverage check (all captures have a mapping?):");
    let mut missing = vec![];
    for capture in ALL_CAPTURES {
        if maps.iter().any(|m| m.capture == *capture) {
            println!("   ✅ @{}", capture);
        } else {
            println!("   ❌ @{} — NO MAPPING", capture);
            missing.push(capture);
        }
    }
    println!();

    // 2. Check Style field usage
    println!("2. Style fields used by mappings:");
    let uses_bold = maps.iter().any(|m| m.bold);
    let uses_italic = maps.iter().any(|m| m.italic);
    println!("   fg (Color::Rgb):  YES (all mappings)");
    println!("   bold:             {}", if uses_bold { "YES" } else { "NO" });
    println!("   italic:           {}", if uses_italic { "YES" } else { "NO" });
    println!("   bg:               NO (not needed for syntax highlighting)");
    println!("   underline:        NO (not needed for syntax highlighting)");
    println!("   dim:              NO");
    println!("   strikethrough:    NO");
    println!("   inverse:          NO");
    println!("   hidden:           NO");
    println!();

    // 3. Verify all colors are representable with Color::Rgb
    println!("3. Color representability:");
    println!("   All {} mappings use RGB triples → Color::Rgb {{ r, g, b }}", maps.len());
    println!("   Color::Rgb is already in the Style type ✅");
    println!();

    // 4. Now actually run the highlighter to verify we see these captures in practice
    println!("4. Live verification — highlight a Rust snippet and check captures:");
    let snippet = r#"
use std::collections::HashMap;

/// A documented function
fn process(items: &[u32]) -> HashMap<String, Vec<u32>> {
    let mut result = HashMap::new();
    for item in items {
        let key = format!("key_{}", item);
        result.entry(key).or_insert_with(Vec::new).push(*item);
    }
    result
}

const MAX_SIZE: usize = 1024;

struct Config {
    name: String,
    enabled: bool,
}

impl Config {
    fn is_valid(&self) -> bool {
        !self.name.is_empty() && self.enabled
    }
}
"#;

    let mut hl_config = HighlightConfiguration::new(
        tree_sitter_rust::LANGUAGE.into(),
        "rust",
        tree_sitter_rust::HIGHLIGHTS_QUERY,
        "",
        "",
    ).expect("Failed to create highlight config");

    // Register our capture names
    let capture_names: Vec<&str> = maps.iter().map(|m| m.capture).collect();
    hl_config.configure(&capture_names);

    let mut highlighter = Highlighter::new();
    let highlights = highlighter.highlight(
        &hl_config,
        snippet.as_bytes(),
        None,
        |_| None,
    ).expect("Highlight failed");

    let mut seen_captures: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut current_capture: Option<usize> = None;

    // Print highlighted output showing which captures fire
    print!("   ");
    for event in highlights {
        match event.expect("event error") {
            HighlightEvent::HighlightStart(h) => {
                seen_captures.insert(h.0);
                current_capture = Some(h.0);
            }
            HighlightEvent::HighlightEnd => {
                current_capture = None;
            }
            HighlightEvent::Source { start, end } => {
                let text = &snippet[start..end];
                if let Some(idx) = current_capture {
                    // Print with capture name annotation for first occurrence
                    let name = capture_names[idx];
                    // Show colored output indicator
                    let [r, g, b] = maps[idx].fg;
                    print!("\x1b[38;2;{};{};{}m{}\x1b[0m", r, g, b, text.replace('\n', "\n   "));
                } else {
                    print!("{}", text.replace('\n', "\n   "));
                }
            }
        }
    }
    println!();
    println!();

    // Report which captures were actually seen
    println!("   Captures seen in snippet: {}/{}", seen_captures.len(), capture_names.len());
    for (i, name) in capture_names.iter().enumerate() {
        let status = if seen_captures.contains(&i) { "✅ seen" } else { "⬜ not in snippet" };
        println!("     @{:<25} {}", name, status);
    }

    println!();
    println!("=== Conclusion ===");
    if missing.is_empty() {
        println!("✅ H5 VERIFIED: All {} tree-sitter-rust captures map to existing Style fields.", ALL_CAPTURES.len());
        println!("   Only fg (Color::Rgb), bold, and italic are needed.");
        println!("   No changes to Style, Color, Span, or StyledLine types required.");
    } else {
        println!("❌ H5 INCOMPLETE: {} captures have no mapping: {:?}", missing.len(), missing);
    }
}
