// Chunk: docs/chunks/syntax_highlighting - Language registry for 13 languages
// Chunk: docs/chunks/syntax_highlight_perf - LanguageConfig highlights_query for direct QueryCursor usage

//! Language registry mapping file extensions to tree-sitter configurations.
//!
//! This module provides `LanguageRegistry` which maps file extensions to
//! tree-sitter `Language` objects and their associated highlight queries.

use std::collections::HashMap;
use tree_sitter::Language;

/// Configuration for a language's syntax highlighting.
///
/// Contains the tree-sitter `Language` and highlight queries needed
/// for syntax highlighting via `QueryCursor`.
pub struct LanguageConfig {
    /// The tree-sitter language
    pub language: Language,
    /// The highlights query (tree-sitter query syntax)
    pub highlights_query: &'static str,
    // Chunk: docs/chunks/highlight_injection - Injection query now used for embedded language highlighting
    /// The injections query (for embedded languages like Markdown code blocks, HTML script tags)
    pub injections_query: &'static str,
    /// The locals query (for scope-based highlighting)
    #[allow(dead_code)] // Reserved for future locals support
    pub locals_query: &'static str,
}

impl LanguageConfig {
    /// Creates a new language configuration.
    pub fn new(
        language: Language,
        highlights_query: &'static str,
        injections_query: &'static str,
        locals_query: &'static str,
    ) -> Self {
        Self {
            language,
            highlights_query,
            injections_query,
            locals_query,
        }
    }
}

/// Registry mapping file extensions to language configurations.
///
/// Supports 13 languages: Rust, C++, C, Python, TypeScript, JavaScript,
/// Go, JSON, TOML, Markdown, HTML, CSS, and Bash.
pub struct LanguageRegistry {
    /// Map from extension (without leading dot) to language config
    configs: HashMap<&'static str, LanguageConfig>,
}

impl LanguageRegistry {
    // Chunk: docs/chunks/highlight_injection - Empty registry for non-injection languages
    /// Creates an empty language registry.
    ///
    /// This is used for languages that don't have injection support, avoiding
    /// the overhead of initializing all language configs.
    pub fn empty() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    /// Creates a new language registry with all supported languages.
    pub fn new() -> Self {
        let mut configs = HashMap::new();

        // Rust (uses HIGHLIGHTS_QUERY)
        let rust_config = LanguageConfig::new(
            tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        );
        configs.insert("rs", rust_config);

        // C++ needs the C highlight query as a base, with C++-specific additions layered on top.
        // The C++ grammar's HIGHLIGHT_QUERY only covers C++-specific constructs (templates,
        // namespaces, `this`, etc.), while fundamental constructs like types, keywords, and
        // functions are defined in the C grammar's query.
        let cpp_combined_query: &'static str = Box::leak(
            format!("{}\n{}", tree_sitter_c::HIGHLIGHT_QUERY, tree_sitter_cpp::HIGHLIGHT_QUERY)
                .into_boxed_str(),
        );
        let cpp_config = LanguageConfig::new(
            tree_sitter_cpp::LANGUAGE.into(),
            cpp_combined_query,
            "",
            "",
        );
        configs.insert("cpp", cpp_config.clone());
        configs.insert("cc", cpp_config.clone());
        configs.insert("cxx", cpp_config.clone());
        configs.insert("hpp", cpp_config.clone());
        configs.insert("h", cpp_config); // .h is ambiguous, default to C++

        // C (uses HIGHLIGHT_QUERY - no S)
        let c_config = LanguageConfig::new(
            tree_sitter_c::LANGUAGE.into(),
            tree_sitter_c::HIGHLIGHT_QUERY,
            "",
            "",
        );
        configs.insert("c", c_config);

        // Python (uses HIGHLIGHTS_QUERY)
        let python_config = LanguageConfig::new(
            tree_sitter_python::LANGUAGE.into(),
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "",
            "",
        );
        configs.insert("py", python_config);

        // Chunk: docs/chunks/typescript_highlight_layering - Combined JS/TS highlight queries
        // TypeScript needs the JavaScript highlight query as a base, with TypeScript-specific
        // additions layered on top. Same pattern as C/C++.
        let ts_combined_query: &'static str = Box::leak(
            format!("{}\n{}", tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_typescript::HIGHLIGHTS_QUERY)
                .into_boxed_str(),
        );
        let typescript_config = LanguageConfig::new(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            ts_combined_query,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        );
        configs.insert("ts", typescript_config);

        // TSX also needs the JavaScript base (it extends TypeScript which extends JavaScript)
        let tsx_config = LanguageConfig::new(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            ts_combined_query,  // Reuse the combined query
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        );
        configs.insert("tsx", tsx_config);

        // JavaScript (uses HIGHLIGHT_QUERY - no S)
        let javascript_config = LanguageConfig::new(
            tree_sitter_javascript::LANGUAGE.into(),
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        );
        configs.insert("js", javascript_config.clone());
        configs.insert("jsx", javascript_config.clone());
        configs.insert("mjs", javascript_config);

        // Go (uses HIGHLIGHTS_QUERY)
        let go_config = LanguageConfig::new(
            tree_sitter_go::LANGUAGE.into(),
            tree_sitter_go::HIGHLIGHTS_QUERY,
            "",
            "",
        );
        configs.insert("go", go_config);

        // JSON (uses HIGHLIGHTS_QUERY)
        let json_config = LanguageConfig::new(
            tree_sitter_json::LANGUAGE.into(),
            tree_sitter_json::HIGHLIGHTS_QUERY,
            "",
            "",
        );
        configs.insert("json", json_config);

        // TOML (uses tree-sitter-toml-ng with LANGUAGE and HIGHLIGHTS_QUERY)
        let toml_config = LanguageConfig::new(
            tree_sitter_toml_ng::LANGUAGE.into(),
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            "",
            "",
        );
        configs.insert("toml", toml_config);

        // Markdown (uses HIGHLIGHT_QUERY_BLOCK for the block parser)
        let md_config = LanguageConfig::new(
            tree_sitter_md::LANGUAGE.into(),
            tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
            tree_sitter_md::INJECTION_QUERY_BLOCK,
            "",
        );
        configs.insert("md", md_config.clone());
        configs.insert("markdown", md_config);

        // HTML (uses HIGHLIGHTS_QUERY)
        let html_config = LanguageConfig::new(
            tree_sitter_html::LANGUAGE.into(),
            tree_sitter_html::HIGHLIGHTS_QUERY,
            tree_sitter_html::INJECTIONS_QUERY,
            "",
        );
        configs.insert("html", html_config.clone());
        configs.insert("htm", html_config);

        // CSS (uses HIGHLIGHTS_QUERY)
        let css_config = LanguageConfig::new(
            tree_sitter_css::LANGUAGE.into(),
            tree_sitter_css::HIGHLIGHTS_QUERY,
            "",
            "",
        );
        configs.insert("css", css_config);

        // Bash (uses HIGHLIGHT_QUERY - no S)
        let bash_config = LanguageConfig::new(
            tree_sitter_bash::LANGUAGE.into(),
            tree_sitter_bash::HIGHLIGHT_QUERY,
            "",
            "",
        );
        configs.insert("sh", bash_config.clone());
        configs.insert("bash", bash_config.clone());
        configs.insert("zsh", bash_config);

        Self { configs }
    }

    /// Returns the language configuration for a file extension.
    ///
    /// The extension can be with or without a leading dot (e.g., ".rs" or "rs").
    pub fn config_for_extension(&self, ext: &str) -> Option<&LanguageConfig> {
        let ext = ext.strip_prefix('.').unwrap_or(ext);
        self.configs.get(ext)
    }

    /// Returns an iterator over all supported extensions.
    pub fn supported_extensions(&self) -> impl Iterator<Item = &str> {
        self.configs.keys().copied()
    }

    // Chunk: docs/chunks/highlight_injection - Language name lookup for injection support
    /// Returns the language configuration for a language name.
    ///
    /// This method maps common language names (as used in fenced code blocks
    /// and injection queries) to their corresponding `LanguageConfig`. This
    /// enables injection support where the embedded language is identified
    /// by name (e.g., "rust", "python") rather than file extension.
    ///
    /// # Supported mappings
    ///
    /// - "rust" → "rs"
    /// - "python" → "py"
    /// - "javascript", "js" → "js"
    /// - "typescript", "ts" → "ts"
    /// - "tsx" → "tsx"
    /// - "json" → "json"
    /// - "toml" → "toml"
    /// - "html" → "html"
    /// - "css" → "css"
    /// - "bash", "shell", "sh" → "sh"
    /// - "c" → "c"
    /// - "cpp", "c++" → "cpp"
    /// - "go", "golang" → "go"
    /// - "markdown", "md" → "md"
    ///
    /// # Returns
    ///
    /// Returns `Some(&LanguageConfig)` if the language name is recognized,
    /// `None` otherwise (unknown languages gracefully fall back to no highlighting).
    pub fn config_for_language_name(&self, name: &str) -> Option<&LanguageConfig> {
        // Normalize: lowercase and trim
        let name = name.to_lowercase();
        let name = name.trim();

        // Map language name to extension
        let ext = match name {
            "rust" => "rs",
            "python" => "py",
            "javascript" | "js" => "js",
            "typescript" | "ts" => "ts",
            "tsx" => "tsx",
            "json" => "json",
            "toml" => "toml",
            "html" => "html",
            "css" => "css",
            "bash" | "shell" | "sh" => "sh",
            "c" => "c",
            "cpp" | "c++" => "cpp",
            "go" | "golang" => "go",
            "markdown" | "md" => "md",
            // Pass through extension names directly
            other => other,
        };

        self.config_for_extension(ext)
    }
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// LanguageConfig needs Clone for the registry initialization
impl Clone for LanguageConfig {
    fn clone(&self) -> Self {
        Self {
            language: self.language.clone(),
            highlights_query: self.highlights_query,
            injections_query: self.injections_query,
            locals_query: self.locals_query,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_extension() {
        let registry = LanguageRegistry::new();
        assert!(registry.config_for_extension("rs").is_some());
        assert!(registry.config_for_extension(".rs").is_some());
    }

    #[test]
    fn test_cpp_extensions() {
        let registry = LanguageRegistry::new();
        for ext in ["cpp", "cc", "cxx", "hpp", "h"] {
            assert!(
                registry.config_for_extension(ext).is_some(),
                "Extension '{}' should be supported",
                ext
            );
        }
    }

    #[test]
    fn test_c_extension() {
        let registry = LanguageRegistry::new();
        assert!(registry.config_for_extension("c").is_some());
    }

    #[test]
    fn test_python_extension() {
        let registry = LanguageRegistry::new();
        assert!(registry.config_for_extension("py").is_some());
    }

    #[test]
    fn test_typescript_extensions() {
        let registry = LanguageRegistry::new();
        assert!(registry.config_for_extension("ts").is_some());
        assert!(registry.config_for_extension("tsx").is_some());
    }

    #[test]
    fn test_javascript_extensions() {
        let registry = LanguageRegistry::new();
        for ext in ["js", "jsx", "mjs"] {
            assert!(
                registry.config_for_extension(ext).is_some(),
                "Extension '{}' should be supported",
                ext
            );
        }
    }

    #[test]
    fn test_go_extension() {
        let registry = LanguageRegistry::new();
        assert!(registry.config_for_extension("go").is_some());
    }

    #[test]
    fn test_json_extension() {
        let registry = LanguageRegistry::new();
        assert!(registry.config_for_extension("json").is_some());
    }

    #[test]
    fn test_toml_extension() {
        let registry = LanguageRegistry::new();
        assert!(registry.config_for_extension("toml").is_some());
    }

    #[test]
    fn test_markdown_extensions() {
        let registry = LanguageRegistry::new();
        assert!(registry.config_for_extension("md").is_some());
        assert!(registry.config_for_extension("markdown").is_some());
    }

    #[test]
    fn test_html_extensions() {
        let registry = LanguageRegistry::new();
        assert!(registry.config_for_extension("html").is_some());
        assert!(registry.config_for_extension("htm").is_some());
    }

    #[test]
    fn test_css_extension() {
        let registry = LanguageRegistry::new();
        assert!(registry.config_for_extension("css").is_some());
    }

    #[test]
    fn test_bash_extensions() {
        let registry = LanguageRegistry::new();
        for ext in ["sh", "bash", "zsh"] {
            assert!(
                registry.config_for_extension(ext).is_some(),
                "Extension '{}' should be supported",
                ext
            );
        }
    }

    #[test]
    fn test_unknown_extension() {
        let registry = LanguageRegistry::new();
        assert!(registry.config_for_extension("xyz").is_none());
        assert!(registry.config_for_extension("txt").is_none());
    }

    #[test]
    fn test_extension_with_and_without_dot() {
        let registry = LanguageRegistry::new();
        // Both should work
        let with_dot = registry.config_for_extension(".rs");
        let without_dot = registry.config_for_extension("rs");
        assert!(with_dot.is_some());
        assert!(without_dot.is_some());
    }

    #[test]
    fn test_supported_extensions_count() {
        let registry = LanguageRegistry::new();
        let count = registry.supported_extensions().count();
        // We have 24 extension mappings (some languages have multiple extensions)
        assert!(count >= 20, "Expected at least 20 extension mappings, got {}", count);
    }

    // Chunk: docs/chunks/typescript_highlight_layering - TypeScript/JS highlight query layering tests

    #[test]
    fn test_typescript_highlights_javascript_keywords() {
        use crate::highlighter::SyntaxHighlighter;
        use crate::theme::SyntaxTheme;
        use lite_edit_buffer::Color;

        let registry = LanguageRegistry::new();
        let config = registry.config_for_extension("ts").expect("TypeScript should be supported");
        let theme = SyntaxTheme::catppuccin_mocha();

        // TypeScript source containing JavaScript-level constructs
        let source = r#"const message: string = "hello";"#;
        let hl = SyntaxHighlighter::new(config, source, theme)
            .expect("Should create highlighter");

        let styled = hl.highlight_line(0);

        // Check that "const" keyword is highlighted (not default color)
        let const_span = styled.spans.iter().find(|span| span.text.contains("const"));
        assert!(
            const_span.is_some(),
            "Should have a span containing 'const', got spans: {:?}",
            styled.spans.iter().map(|s| &s.text).collect::<Vec<_>>()
        );
        let const_span = const_span.unwrap();
        assert!(
            !matches!(const_span.style.fg, Color::Default),
            "'const' keyword should be styled, not default. Span: {:?}",
            const_span
        );
    }

    #[test]
    fn test_typescript_highlights_string_literals() {
        use crate::highlighter::SyntaxHighlighter;
        use crate::theme::SyntaxTheme;
        use lite_edit_buffer::Color;

        let registry = LanguageRegistry::new();
        let config = registry.config_for_extension("ts").expect("TypeScript should be supported");
        let theme = SyntaxTheme::catppuccin_mocha();

        // TypeScript source with a string literal
        let source = r#"const message: string = "hello";"#;
        let hl = SyntaxHighlighter::new(config, source, theme)
            .expect("Should create highlighter");

        let styled = hl.highlight_line(0);

        // Check that the string literal is highlighted
        let string_span = styled.spans.iter().find(|span| span.text.contains("hello"));
        assert!(
            string_span.is_some(),
            "Should have a span containing 'hello', got spans: {:?}",
            styled.spans.iter().map(|s| &s.text).collect::<Vec<_>>()
        );
        let string_span = string_span.unwrap();
        assert!(
            !matches!(string_span.style.fg, Color::Default),
            "String literal should be styled, not default. Span: {:?}",
            string_span
        );
    }

    // =========================================================================
    // Language name lookup tests (Chunk: docs/chunks/highlight_injection)
    // =========================================================================

    #[test]
    fn test_language_name_lookup_rust() {
        let registry = LanguageRegistry::new();
        // "rust" should return the same config as "rs"
        let by_name = registry.config_for_language_name("rust");
        let by_ext = registry.config_for_extension("rs");
        assert!(by_name.is_some());
        assert!(by_ext.is_some());
        // Compare highlights_query pointers to verify same config
        assert_eq!(
            by_name.unwrap().highlights_query as *const str,
            by_ext.unwrap().highlights_query as *const str
        );
    }

    #[test]
    fn test_language_name_lookup_javascript() {
        let registry = LanguageRegistry::new();
        // "javascript" and "js" should return the same config
        let by_full_name = registry.config_for_language_name("javascript");
        let by_short_name = registry.config_for_language_name("js");
        let by_ext = registry.config_for_extension("js");
        assert!(by_full_name.is_some());
        assert!(by_short_name.is_some());
        assert!(by_ext.is_some());
        assert_eq!(
            by_full_name.unwrap().highlights_query as *const str,
            by_ext.unwrap().highlights_query as *const str
        );
        assert_eq!(
            by_short_name.unwrap().highlights_query as *const str,
            by_ext.unwrap().highlights_query as *const str
        );
    }

    #[test]
    fn test_language_name_lookup_python() {
        let registry = LanguageRegistry::new();
        let by_name = registry.config_for_language_name("python");
        let by_ext = registry.config_for_extension("py");
        assert!(by_name.is_some());
        assert!(by_ext.is_some());
        assert_eq!(
            by_name.unwrap().highlights_query as *const str,
            by_ext.unwrap().highlights_query as *const str
        );
    }

    #[test]
    fn test_language_name_lookup_bash_variants() {
        let registry = LanguageRegistry::new();
        let by_bash = registry.config_for_language_name("bash");
        let by_shell = registry.config_for_language_name("shell");
        let by_sh = registry.config_for_language_name("sh");
        let by_ext = registry.config_for_extension("sh");
        assert!(by_bash.is_some());
        assert!(by_shell.is_some());
        assert!(by_sh.is_some());
        // All should map to the same config
        assert_eq!(
            by_bash.unwrap().highlights_query as *const str,
            by_ext.unwrap().highlights_query as *const str
        );
        assert_eq!(
            by_shell.unwrap().highlights_query as *const str,
            by_ext.unwrap().highlights_query as *const str
        );
    }

    #[test]
    fn test_language_name_lookup_unknown() {
        let registry = LanguageRegistry::new();
        // "fortran" is not supported
        assert!(registry.config_for_language_name("fortran").is_none());
        assert!(registry.config_for_language_name("cobol").is_none());
        assert!(registry.config_for_language_name("").is_none());
    }

    #[test]
    fn test_language_name_lookup_case_insensitive() {
        let registry = LanguageRegistry::new();
        // Should be case insensitive
        assert!(registry.config_for_language_name("RUST").is_some());
        assert!(registry.config_for_language_name("Rust").is_some());
        assert!(registry.config_for_language_name("JavaScript").is_some());
        assert!(registry.config_for_language_name("PYTHON").is_some());
    }

    #[test]
    fn test_language_name_lookup_with_whitespace() {
        let registry = LanguageRegistry::new();
        // Should trim whitespace
        assert!(registry.config_for_language_name(" rust ").is_some());
        assert!(registry.config_for_language_name("  python  ").is_some());
    }

    #[test]
    fn test_rust_injections_query_length() {
        let registry = LanguageRegistry::new();
        let config = registry.config_for_extension("rs").unwrap();
        eprintln!("Rust injections_query length: {}", config.injections_query.len());
        eprintln!("Is empty: {}", config.injections_query.is_empty());
        // The Rust injections query is NOT empty - it contains patterns for doc comments
        // This is why we need to optimize the injection path
        assert!(config.injections_query.len() > 0, "Expected non-empty injections query");
    }

    #[test]
    fn test_tsx_highlights_javascript_keywords() {
        use crate::highlighter::SyntaxHighlighter;
        use crate::theme::SyntaxTheme;
        use lite_edit_buffer::Color;

        let registry = LanguageRegistry::new();
        let config = registry.config_for_extension("tsx").expect("TSX should be supported");
        let theme = SyntaxTheme::catppuccin_mocha();

        // TSX source containing JavaScript-level constructs
        let source = r#"const Component = () => { return <div>hello</div>; };"#;
        let hl = SyntaxHighlighter::new(config, source, theme)
            .expect("Should create highlighter");

        let styled = hl.highlight_line(0);

        // Check that "const" keyword is highlighted
        let const_span = styled.spans.iter().find(|span| span.text.contains("const"));
        assert!(
            const_span.is_some(),
            "Should have a span containing 'const', got spans: {:?}",
            styled.spans.iter().map(|s| &s.text).collect::<Vec<_>>()
        );
        let const_span = const_span.unwrap();
        assert!(
            !matches!(const_span.style.fg, Color::Default),
            "'const' keyword should be styled in TSX, not default. Span: {:?}",
            const_span
        );
    }
}
