// Chunk: docs/chunks/syntax_highlighting - Language registry for 13 languages

//! Language registry mapping file extensions to tree-sitter configurations.
//!
//! This module provides `LanguageRegistry` which maps file extensions to
//! tree-sitter `Language` objects and their associated highlight queries.

use std::collections::HashMap;
use tree_sitter::Language;
use tree_sitter_highlight::HighlightConfiguration;

/// Configuration for a language's syntax highlighting.
///
/// Contains the tree-sitter `Language` and highlight queries needed
/// to create a `HighlightConfiguration`.
pub struct LanguageConfig {
    /// The tree-sitter language
    pub language: Language,
    /// The highlights query (tree-sitter query syntax)
    pub highlights_query: &'static str,
    /// The injections query (for embedded languages)
    pub injections_query: &'static str,
    /// The locals query (for scope-based highlighting)
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

    /// Creates a `HighlightConfiguration` for this language.
    ///
    /// The `capture_names` parameter determines the mapping from capture
    /// names to highlight IDs.
    pub fn highlight_config(
        &self,
        capture_names: &[&str],
    ) -> Option<HighlightConfiguration> {
        let mut config = HighlightConfiguration::new(
            self.language.clone(),
            "source",
            self.highlights_query,
            self.injections_query,
            self.locals_query,
        ).ok()?;
        config.configure(capture_names);
        Some(config)
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

        // C++ (uses HIGHLIGHT_QUERY - no S)
        let cpp_config = LanguageConfig::new(
            tree_sitter_cpp::LANGUAGE.into(),
            tree_sitter_cpp::HIGHLIGHT_QUERY,
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

        // TypeScript (uses HIGHLIGHTS_QUERY)
        let typescript_config = LanguageConfig::new(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        );
        configs.insert("ts", typescript_config);

        // TSX (TypeScript JSX, uses HIGHLIGHTS_QUERY)
        let tsx_config = LanguageConfig::new(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
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
}
