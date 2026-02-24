// Chunk: docs/chunks/dragdrop_file_paste - Shell escaping for drag-and-drop file paths
//! Shell escaping utilities for file paths.
//!
//! When files are dropped onto the terminal, their paths need to be shell-escaped
//! to handle spaces, single quotes, and other special characters. We use single-quote
//! escaping (the standard POSIX approach) which handles most paths correctly.

/// Shell-escapes a single file path for safe use in shell commands.
///
/// The path is wrapped in single quotes, with any internal single quotes escaped
/// using the `'\''` pattern (end quote, escaped quote, start quote).
///
/// # Examples
///
/// ```ignore
/// assert_eq!(shell_escape_path("/Users/test/file.txt"), "'/Users/test/file.txt'");
/// assert_eq!(shell_escape_path("/path/with spaces"), "'/path/with spaces'");
/// assert_eq!(shell_escape_path("/path/with'quote"), "'/path/with'\\''quote'");
/// ```
// Chunk: docs/chunks/dragdrop_file_paste - Shell escape path function
pub fn shell_escape_path(path: &str) -> String {
    // Single-quote escaping: wrap in single quotes, escape internal single quotes
    // by ending the quote, adding an escaped quote, and resuming.
    // Example: foo's bar -> 'foo'\''s bar'
    let mut result = String::with_capacity(path.len() + 2);
    result.push('\'');
    for c in path.chars() {
        if c == '\'' {
            // End the single-quoted string, add an escaped single quote, resume
            result.push_str("'\\''");
        } else {
            result.push(c);
        }
    }
    result.push('\'');
    result
}

/// Shell-escapes multiple file paths and joins them with spaces.
///
/// Each path is individually escaped, then joined with a single space separator.
/// This is the standard format for dropping multiple files onto a terminal.
// Chunk: docs/chunks/dragdrop_file_paste - Shell escape multiple paths function
pub fn shell_escape_paths(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| shell_escape_path(p))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_path() {
        assert_eq!(
            shell_escape_path("/Users/test/file.txt"),
            "'/Users/test/file.txt'"
        );
    }

    #[test]
    fn test_path_with_space() {
        assert_eq!(
            shell_escape_path("/Users/test/my file.txt"),
            "'/Users/test/my file.txt'"
        );
    }

    #[test]
    fn test_path_with_single_quote() {
        assert_eq!(
            shell_escape_path("/Users/test/foo's.txt"),
            "'/Users/test/foo'\\''s.txt'"
        );
    }

    #[test]
    fn test_path_with_space_and_quote() {
        assert_eq!(
            shell_escape_path("/Users/test/foo's file.txt"),
            "'/Users/test/foo'\\''s file.txt'"
        );
    }

    #[test]
    fn test_path_with_multiple_quotes() {
        assert_eq!(
            shell_escape_path("it's a 'test' path"),
            "'it'\\''s a '\\''test'\\'' path'"
        );
    }

    #[test]
    fn test_empty_path() {
        assert_eq!(shell_escape_path(""), "''");
    }

    #[test]
    fn test_path_with_special_chars() {
        // Characters that need escaping in double-quotes but not in single-quotes
        assert_eq!(
            shell_escape_path("/path/$HOME/file"),
            "'/path/$HOME/file'"
        );
        assert_eq!(
            shell_escape_path("/path/with`backtick`"),
            "'/path/with`backtick`'"
        );
        assert_eq!(
            shell_escape_path("/path/with\\backslash"),
            "'/path/with\\backslash'"
        );
    }

    #[test]
    fn test_multiple_paths_single() {
        let paths = vec!["/Users/test/file.txt".to_string()];
        assert_eq!(shell_escape_paths(&paths), "'/Users/test/file.txt'");
    }

    #[test]
    fn test_multiple_paths_two() {
        let paths = vec![
            "/Users/test/file1.txt".to_string(),
            "/Users/test/file2.txt".to_string(),
        ];
        assert_eq!(
            shell_escape_paths(&paths),
            "'/Users/test/file1.txt' '/Users/test/file2.txt'"
        );
    }

    #[test]
    fn test_multiple_paths_with_spaces_and_quotes() {
        let paths = vec![
            "/path/to/foo's file.txt".to_string(),
            "/other/path/bar.txt".to_string(),
        ];
        assert_eq!(
            shell_escape_paths(&paths),
            "'/path/to/foo'\\''s file.txt' '/other/path/bar.txt'"
        );
    }

    #[test]
    fn test_multiple_paths_empty() {
        let paths: Vec<String> = vec![];
        assert_eq!(shell_escape_paths(&paths), "");
    }
}
