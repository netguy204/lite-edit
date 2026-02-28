// Chunk: docs/chunks/treesitter_symbol_index - Cross-file symbol index for go-to-definition
//!
//! A workspace-wide symbol index built from tree-sitter `tags.scm` queries.
//!
//! This module provides a `SymbolIndex` that maps symbol names (functions, classes,
//! structs, modules, etc.) to their definition locations across all files in a
//! workspace. The index is built on a background thread and supports incremental
//! updates when files are saved.
//!
//! The symbol index enables cross-file go-to-definition: when same-file resolution
//! (via `LocalsResolver`) finds no match, the editor can fall back to searching the
//! symbol index for matching definitions in other files.
//!
//! # Example
//!
//! ```ignore
//! use lite_edit_syntax::{SymbolIndex, LanguageRegistry};
//! use std::path::PathBuf;
//! use std::sync::Arc;
//!
//! // Start indexing a workspace
//! let registry = Arc::new(LanguageRegistry::new());
//! let index = SymbolIndex::start_indexing(PathBuf::from("/path/to/workspace"), registry);
//!
//! // Wait for indexing to complete (or check is_indexing())
//! while index.is_indexing() {
//!     std::thread::sleep(std::time::Duration::from_millis(100));
//! }
//!
//! // Look up a symbol
//! let locations = index.lookup("my_function");
//! for loc in locations {
//!     println!("Found at {}:{}:{}", loc.file_path.display(), loc.line, loc.col);
//! }
//! ```

use crate::registry::LanguageRegistry;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

// =============================================================================
// SymbolKind
// =============================================================================

/// The kind of symbol definition.
///
/// Corresponds to the `@definition.*` capture names in tags.scm queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// A function or method definition
    Function,
    /// A class definition
    Class,
    /// A method definition
    Method,
    /// A module or namespace definition
    Module,
    /// An interface or protocol definition
    Interface,
    /// A macro definition
    Macro,
    /// A constant definition
    Constant,
    /// A type alias or typedef
    Type,
    /// A struct or record definition
    Struct,
    /// A trait definition
    Trait,
    /// An enum definition
    Enum,
    /// An unknown symbol kind
    Unknown,
}

impl SymbolKind {
    /// Parses a symbol kind from a tags.scm capture name.
    ///
    /// Tags capture names follow the pattern `@definition.{kind}` or just
    /// `@name` when nested inside a definition pattern.
    fn from_capture_name(name: &str) -> Option<Self> {
        // Handle both "definition.function" and "name" capture patterns
        let kind_str = if name.starts_with("definition.") {
            &name["definition.".len()..]
        } else if name == "name" {
            return None; // We need the definition capture, not the name
        } else {
            name
        };

        Some(match kind_str {
            "function" => SymbolKind::Function,
            "method" => SymbolKind::Method,
            "class" => SymbolKind::Class,
            "module" => SymbolKind::Module,
            "interface" => SymbolKind::Interface,
            "macro" => SymbolKind::Macro,
            "constant" => SymbolKind::Constant,
            "type" => SymbolKind::Type,
            "struct" => SymbolKind::Struct,
            "trait" => SymbolKind::Trait,
            "enum" => SymbolKind::Enum,
            _ => SymbolKind::Unknown,
        })
    }
}

// =============================================================================
// SymbolLocation
// =============================================================================

/// Location of a symbol definition in the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLocation {
    /// Absolute path to the file containing the definition
    pub file_path: PathBuf,
    /// Line number (0-indexed)
    pub line: usize,
    /// Column number (0-indexed)
    pub col: usize,
    /// The kind of symbol (function, class, etc.)
    pub kind: SymbolKind,
}

// =============================================================================
// SymbolIndex
// =============================================================================

/// Thread-safe symbol index for cross-file go-to-definition.
///
/// The index maps symbol names to their definition locations. Multiple files
/// can define symbols with the same name (e.g., `new()` in different modules).
pub struct SymbolIndex {
    /// Maps symbol name -> Vec<SymbolLocation>
    index: Arc<RwLock<HashMap<String, Vec<SymbolLocation>>>>,
    /// True while the initial indexing is in progress
    indexing: Arc<AtomicBool>,
    /// Handle to the walker thread (if background indexing)
    _walker_handle: Option<JoinHandle<()>>,
}

impl SymbolIndex {
    /// Creates an empty symbol index.
    pub fn new() -> Self {
        Self {
            index: Arc::new(RwLock::new(HashMap::new())),
            indexing: Arc::new(AtomicBool::new(false)),
            _walker_handle: None,
        }
    }

    /// Returns true while the initial workspace indexing is in progress.
    pub fn is_indexing(&self) -> bool {
        self.indexing.load(Ordering::Relaxed)
    }

    /// Looks up all definitions of a symbol by name.
    ///
    /// Returns a cloned vector of locations to avoid holding the lock.
    pub fn lookup(&self, name: &str) -> Vec<SymbolLocation> {
        let guard = self.index.read().unwrap();
        guard.get(name).cloned().unwrap_or_default()
    }

    /// Inserts a symbol location into the index.
    pub fn insert(&self, name: String, loc: SymbolLocation) {
        let mut guard = self.index.write().unwrap();
        guard.entry(name).or_default().push(loc);
    }

    /// Removes all symbols from a specific file.
    ///
    /// Used for incremental updates when a file is re-indexed.
    pub fn remove_file(&self, path: &Path) {
        let mut guard = self.index.write().unwrap();
        for locations in guard.values_mut() {
            locations.retain(|loc| loc.file_path != path);
        }
        // Remove any entries that now have empty location vectors
        guard.retain(|_, locs| !locs.is_empty());
    }

    /// Clears all entries from the index.
    #[allow(dead_code)]
    pub fn clear(&self) {
        let mut guard = self.index.write().unwrap();
        guard.clear();
    }

    /// Returns the number of unique symbol names in the index.
    #[allow(dead_code)]
    pub fn symbol_count(&self) -> usize {
        let guard = self.index.read().unwrap();
        guard.len()
    }

    /// Starts indexing a workspace in a background thread.
    ///
    /// Returns immediately with an index that will be populated asynchronously.
    /// Use `is_indexing()` to check if the initial indexing is complete.
    ///
    /// # Arguments
    ///
    /// * `root` - The root directory of the workspace to index
    /// * `registry` - The language registry for extension-to-language mapping
    pub fn start_indexing(root: PathBuf, registry: Arc<LanguageRegistry>) -> Self {
        let index = Arc::new(RwLock::new(HashMap::new()));
        let indexing = Arc::new(AtomicBool::new(true));

        let index_clone = Arc::clone(&index);
        let indexing_clone = Arc::clone(&indexing);
        let root_clone = root.clone();

        let handle = thread::spawn(move || {
            index_workspace(&root_clone, &index_clone, &registry);
            indexing_clone.store(false, Ordering::Relaxed);
        });

        Self {
            index,
            indexing,
            _walker_handle: Some(handle),
        }
    }

    /// Updates the index for a single file.
    ///
    /// Removes all existing entries for the file and re-indexes it.
    /// Used for incremental updates when a file is saved.
    pub fn update_file(&self, file_path: &Path, registry: &LanguageRegistry) {
        // Remove existing entries for this file
        self.remove_file(file_path);

        // Re-index the file
        if let Err(e) = index_file(&self.index, file_path, registry) {
            eprintln!("Failed to index file {:?}: {}", file_path, e);
        }
    }
}

impl Default for SymbolIndex {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Indexing Implementation
// =============================================================================

/// Error type for indexing operations.
#[derive(Debug)]
pub enum IndexError {
    /// File could not be read
    IoError(std::io::Error),
    /// File extension not recognized
    UnknownExtension,
    /// Language has no tags query
    NoTagsQuery,
    /// Failed to compile query
    QueryError(String),
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexError::IoError(e) => write!(f, "IO error: {}", e),
            IndexError::UnknownExtension => write!(f, "Unknown file extension"),
            IndexError::NoTagsQuery => write!(f, "No tags query for this language"),
            IndexError::QueryError(e) => write!(f, "Query error: {}", e),
        }
    }
}

/// Indexes a single file and adds its symbols to the index.
fn index_file(
    index: &Arc<RwLock<HashMap<String, Vec<SymbolLocation>>>>,
    file_path: &Path,
    registry: &LanguageRegistry,
) -> Result<(), IndexError> {
    // Get file extension
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or(IndexError::UnknownExtension)?;

    // Get language config
    let config = registry
        .config_for_extension(ext)
        .ok_or(IndexError::UnknownExtension)?;

    // Skip if no tags query
    if config.tags_query.is_empty() {
        return Err(IndexError::NoTagsQuery);
    }

    // Read file content
    let content = fs::read_to_string(file_path).map_err(IndexError::IoError)?;

    // Parse the file
    let mut parser = Parser::new();
    parser
        .set_language(&config.language)
        .map_err(|e| IndexError::QueryError(format!("{:?}", e)))?;

    let tree = parser
        .parse(&content, None)
        .ok_or_else(|| IndexError::QueryError("Failed to parse file".to_string()))?;

    // Compile and run the tags query
    let query = Query::new(&config.language, config.tags_query)
        .map_err(|e| IndexError::QueryError(format!("{:?}", e)))?;

    let mut cursor = QueryCursor::new();

    // Collect all captures - QueryCaptures is a StreamingIterator, not Iterator
    let mut captures_iter = cursor.captures(&query, tree.root_node(), content.as_bytes());

    // Track the current match to group captures together
    let mut current_match_id: Option<u32> = None;
    let mut symbol_name: Option<String> = None;
    let mut symbol_kind: Option<SymbolKind> = None;
    let mut name_start_byte: Option<usize> = None;

    while let Some((mat, capture_idx)) = captures_iter.next() {
        let capture = &mat.captures[*capture_idx];
        let capture_name = query.capture_names()[capture.index as usize];

        // Check if we're starting a new match
        if current_match_id != Some(mat.id()) {
            // Process the previous match if we had both name and kind
            if let (Some(name), Some(kind), Some(start_byte)) =
                (symbol_name.take(), symbol_kind.take(), name_start_byte.take())
            {
                let (line, col) = byte_offset_to_position(&content, start_byte);
                let loc = SymbolLocation {
                    file_path: file_path.to_path_buf(),
                    line,
                    col,
                    kind,
                };
                let mut guard = index.write().unwrap();
                guard.entry(name).or_default().push(loc);
            }

            current_match_id = Some(mat.id());
            symbol_name = None;
            symbol_kind = None;
            name_start_byte = None;
        }

        // Process this capture
        if capture_name == "name" {
            let node = capture.node;
            symbol_name = node.utf8_text(content.as_bytes()).ok().map(String::from);
            name_start_byte = Some(node.start_byte());
        } else if let Some(kind) = SymbolKind::from_capture_name(capture_name) {
            symbol_kind = Some(kind);
        }
    }

    // Process the last match
    if let (Some(name), Some(kind), Some(start_byte)) = (symbol_name, symbol_kind, name_start_byte) {
        let (line, col) = byte_offset_to_position(&content, start_byte);
        let loc = SymbolLocation {
            file_path: file_path.to_path_buf(),
            line,
            col,
            kind,
        };
        let mut guard = index.write().unwrap();
        guard.entry(name).or_default().push(loc);
    }

    Ok(())
}

/// Converts a byte offset to a (line, col) position.
fn byte_offset_to_position(content: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    let mut current_byte = 0;

    for ch in content.chars() {
        if current_byte >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        current_byte += ch.len_utf8();
    }

    (line, col)
}

/// Indexes all source files in a workspace.
fn index_workspace(
    root: &Path,
    index: &Arc<RwLock<HashMap<String, Vec<SymbolLocation>>>>,
    registry: &LanguageRegistry,
) {
    // Use the `ignore` crate to respect .gitignore patterns
    let walker = WalkBuilder::new(root)
        .hidden(true) // Skip hidden files and directories
        .git_ignore(true) // Respect .gitignore
        .git_global(true) // Respect global gitignore
        .git_exclude(true) // Respect .git/info/exclude
        .build();

    // Supported file extensions for indexing
    let supported_extensions: Vec<&str> = vec![
        "rs", "py", "go", "js", "jsx", "mjs", "ts", "tsx",
    ];

    for entry in walker.flatten() {
        let path = entry.path();

        // Skip directories
        if !path.is_file() {
            continue;
        }

        // Check if this is a source file we should index
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => continue,
        };

        if !supported_extensions.contains(&ext) {
            continue;
        }

        // Index the file
        if let Err(_e) = index_file(index, path, registry) {
            // Silently skip files that fail to index
            // (e.g., unrecognized extensions, parse errors)
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_index_insert_lookup() {
        let index = SymbolIndex::new();

        let loc1 = SymbolLocation {
            file_path: PathBuf::from("src/foo.rs"),
            line: 10,
            col: 4,
            kind: SymbolKind::Function,
        };

        index.insert("my_function".to_string(), loc1.clone());

        let results = index.lookup("my_function");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, PathBuf::from("src/foo.rs"));
        assert_eq!(results[0].line, 10);
    }

    #[test]
    fn test_symbol_index_multiple_definitions() {
        let index = SymbolIndex::new();

        let loc1 = SymbolLocation {
            file_path: PathBuf::from("src/foo.rs"),
            line: 10,
            col: 4,
            kind: SymbolKind::Function,
        };

        let loc2 = SymbolLocation {
            file_path: PathBuf::from("src/bar.rs"),
            line: 20,
            col: 4,
            kind: SymbolKind::Function,
        };

        index.insert("new".to_string(), loc1);
        index.insert("new".to_string(), loc2);

        let results = index.lookup("new");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_symbol_index_remove_file() {
        let index = SymbolIndex::new();

        let loc1 = SymbolLocation {
            file_path: PathBuf::from("src/foo.rs"),
            line: 10,
            col: 4,
            kind: SymbolKind::Function,
        };

        let loc2 = SymbolLocation {
            file_path: PathBuf::from("src/bar.rs"),
            line: 20,
            col: 4,
            kind: SymbolKind::Function,
        };

        index.insert("func_a".to_string(), loc1);
        index.insert("func_b".to_string(), loc2);

        // Remove foo.rs
        index.remove_file(Path::new("src/foo.rs"));

        // func_a should be gone
        let results_a = index.lookup("func_a");
        assert!(results_a.is_empty());

        // func_b should still exist
        let results_b = index.lookup("func_b");
        assert_eq!(results_b.len(), 1);
    }

    #[test]
    fn test_symbol_index_lookup_nonexistent() {
        let index = SymbolIndex::new();
        let results = index.lookup("does_not_exist");
        assert!(results.is_empty());
    }

    #[test]
    fn test_symbol_kind_from_capture_name() {
        assert_eq!(
            SymbolKind::from_capture_name("definition.function"),
            Some(SymbolKind::Function)
        );
        assert_eq!(
            SymbolKind::from_capture_name("definition.class"),
            Some(SymbolKind::Class)
        );
        assert_eq!(
            SymbolKind::from_capture_name("definition.method"),
            Some(SymbolKind::Method)
        );
        assert_eq!(
            SymbolKind::from_capture_name("name"),
            None
        );
    }

    #[test]
    fn test_byte_offset_to_position() {
        let content = "line1\nline2\nline3";

        // Start of file
        assert_eq!(byte_offset_to_position(content, 0), (0, 0));

        // Middle of first line
        assert_eq!(byte_offset_to_position(content, 2), (0, 2));

        // Start of second line
        assert_eq!(byte_offset_to_position(content, 6), (1, 0));

        // Middle of second line
        assert_eq!(byte_offset_to_position(content, 8), (1, 2));

        // Start of third line
        assert_eq!(byte_offset_to_position(content, 12), (2, 0));
    }

    #[test]
    fn test_index_file_rust() {
        use tempfile::TempDir;
        use std::fs::File;
        use std::io::Write;

        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.rs");

        let mut file = File::create(&file_path).unwrap();
        // Note: We use raw string without escaping because writeln! doesn't need {{}}
        writeln!(file, r#"
pub fn hello_world() {{
    println!("Hello, world!");
}}

pub struct MyStruct {{
    field: i32,
}}

impl MyStruct {{
    pub fn new() -> Self {{
        Self {{ field: 0 }}
    }}
}}
"#).unwrap();

        let registry = LanguageRegistry::new();
        let index = Arc::new(RwLock::new(HashMap::new()));

        index_file(&index, &file_path, &registry).unwrap();

        let guard = index.read().unwrap();

        // Debug: print what we found
        eprintln!("Found symbols: {:?}", guard.keys().collect::<Vec<_>>());

        // Check that hello_world function was indexed
        assert!(guard.contains_key("hello_world"), "Expected to find hello_world, got: {:?}", guard.keys().collect::<Vec<_>>());

        // Check that MyStruct was indexed (captured as @definition.class in Rust tags query)
        assert!(guard.contains_key("MyStruct"), "Expected to find MyStruct, got: {:?}", guard.keys().collect::<Vec<_>>());

        // Note: Methods inside impl blocks may not be captured depending on the query structure.
        // The Rust tags query captures methods via:
        //   (declaration_list (function_item name: (identifier) @name) @definition.method)
        // This requires the function_item to be inside a declaration_list (impl block).
        // For now, we'll just verify we got the top-level items.
    }

    #[test]
    fn test_index_file_python() {
        use tempfile::TempDir;
        use std::fs::File;
        use std::io::Write;

        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.py");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, r#"
def greet(name):
    print(f"Hello, {{name}}!")

class Greeter:
    def __init__(self, name):
        self.name = name

    def say_hello(self):
        print(f"Hello, {{self.name}}!")
"#).unwrap();

        let registry = LanguageRegistry::new();
        let index = Arc::new(RwLock::new(HashMap::new()));

        index_file(&index, &file_path, &registry).unwrap();

        let guard = index.read().unwrap();

        // Check that greet function was indexed
        assert!(guard.contains_key("greet"), "Expected to find greet");

        // Check that Greeter class was indexed
        assert!(guard.contains_key("Greeter"), "Expected to find Greeter");
    }

    #[test]
    fn test_index_file_no_tags_query() {
        use tempfile::TempDir;
        use std::fs::File;
        use std::io::Write;

        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.json");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, r#"{{"key": "value"}}"#).unwrap();

        let registry = LanguageRegistry::new();
        let index = Arc::new(RwLock::new(HashMap::new()));

        // Should return NoTagsQuery error
        let result = index_file(&index, &file_path, &registry);
        assert!(matches!(result, Err(IndexError::NoTagsQuery)));
    }
}
