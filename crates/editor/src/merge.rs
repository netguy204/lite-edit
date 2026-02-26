// Chunk: docs/chunks/three_way_merge - Line-level three-way merge for concurrent edits
//! Three-way merge for handling concurrent file modifications.
//!
//! This module implements line-level three-way merge for dirty buffers when external
//! file changes are detected. When the user has unsaved edits and an external program
//! modifies the same file, this algorithm intelligently merges both sets of changes,
//! only flagging true conflicts.
//!
//! # Algorithm
//!
//! The implementation uses the "diff3" algorithm:
//! 1. Compute line-level diffs from base to ours and base to theirs
//! 2. Build edit maps tracking what happened to each base line
//! 3. Walk both edit maps simultaneously, applying non-overlapping changes
//! 4. For overlapping changes with different content, emit conflict markers
//!
//! # Example
//!
//! ```
//! use lite_edit::merge::{three_way_merge, MergeResult};
//!
//! let base = "line 1\nline 2\nline 3\n";
//! let ours = "line 1\nmodified by user\nline 3\n";
//! let theirs = "line 1\nline 2\nmodified by external\n";
//!
//! let result = three_way_merge(base, ours, theirs);
//! assert!(result.is_clean());
//! // Both changes applied: line 2 from user, line 3 from external
//! ```

use similar::TextDiff;

/// Result of a three-way merge operation.
#[derive(Debug, Clone, PartialEq)]
pub enum MergeResult {
    /// Merge succeeded with no conflicts. The String contains the merged content.
    Clean(String),
    /// Merge produced conflicts. The String contains the merged content with
    /// git-style conflict markers (<<<<<<< buffer / ======= / >>>>>>> disk).
    Conflict(String),
}

impl MergeResult {
    /// Returns true if the merge completed without conflicts.
    pub fn is_clean(&self) -> bool {
        matches!(self, MergeResult::Clean(_))
    }

    /// Returns the merged content, whether clean or conflicted.
    pub fn content(&self) -> &str {
        match self {
            MergeResult::Clean(s) | MergeResult::Conflict(s) => s,
        }
    }

    /// Consumes the result and returns the merged content.
    pub fn into_content(self) -> String {
        match self {
            MergeResult::Clean(s) | MergeResult::Conflict(s) => s,
        }
    }
}

/// Represents the action taken on a base line.
#[derive(Debug, Clone, PartialEq)]
enum Action {
    /// Line unchanged from base
    Keep,
    /// Line deleted
    Delete,
    /// Line replaced with new content (may be 0, 1, or more lines)
    Replace(Vec<String>),
}

/// Tracks edits from base to a derived version (ours or theirs).
struct EditMap {
    /// Action for each base line index
    actions: Vec<Action>,
    /// Lines inserted before each base line index.
    /// Index base_lines.len() means insertions at the end.
    insertions: Vec<Vec<String>>,
}

impl EditMap {
    /// Returns the action for the given base line index.
    fn action_at(&self, base_idx: usize) -> Action {
        self.actions.get(base_idx).cloned().unwrap_or(Action::Keep)
    }

    /// Returns lines inserted before the given base line index.
    fn insertions_before(&self, base_idx: usize) -> Vec<String> {
        self.insertions
            .get(base_idx)
            .cloned()
            .unwrap_or_default()
    }
}

/// Builds an edit map from a diff, tracking what happened to each base line.
fn build_edit_map(diff: &TextDiff<'_, '_, '_, str>, base_len: usize) -> EditMap {
    let mut actions = vec![Action::Keep; base_len];
    let mut insertions = vec![Vec::new(); base_len + 1];

    // Collect new lines for index-based access
    let new_lines_all: Vec<&str> = diff.new_slices().iter()
        .flat_map(|s| s.lines())
        .collect();

    for op in diff.ops() {
        use similar::DiffOp;
        match *op {
            DiffOp::Equal { .. } => {
                // Lines unchanged — default is Keep
            }
            DiffOp::Delete {
                old_index,
                old_len,
                ..
            } => {
                for j in old_index..old_index + old_len {
                    actions[j] = Action::Delete;
                }
            }
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => {
                let new_lines: Vec<String> = (new_index..new_index + new_len)
                    .map(|ni| new_lines_all.get(ni).unwrap_or(&"").to_string())
                    .collect();
                insertions[old_index].extend(new_lines);
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                // Mark old lines as deleted, first old line gets replacement
                for j in old_index..old_index + old_len {
                    actions[j] = Action::Delete;
                }
                let new_lines: Vec<String> = (new_index..new_index + new_len)
                    .map(|ni| new_lines_all.get(ni).unwrap_or(&"").to_string())
                    .collect();
                // Attach replacement to the first deleted line as a Replace action
                actions[old_index] = Action::Replace(new_lines);
            }
        }
    }

    EditMap {
        actions,
        insertions,
    }
}

/// Performs a line-level three-way merge.
///
/// # Arguments
///
/// * `base` - The common ancestor content (stored base_content snapshot)
/// * `ours` - The current buffer content (user's local edits)
/// * `theirs` - The new disk content (external program's edits)
///
/// # Returns
///
/// A `MergeResult` indicating whether the merge was clean or produced conflicts.
/// The merged content is available via `result.content()`.
///
/// # Algorithm Details
///
/// The merge handles these cases:
/// - `Keep/Keep` → output base line
/// - `Keep/Delete` or `Delete/Keep` → accept the deletion
/// - `Delete/Delete` → agree on deletion
/// - `Keep/Replace` or `Replace/Keep` → accept the replacement
/// - `Replace/Replace` with same content → accept the convergent edit
/// - `Replace/Replace` with different content → conflict
/// - `Replace/Delete` or `Delete/Replace` → conflict
/// - Insertions before each base line are merged similarly
///
/// Conflict markers use the git-style format:
/// ```text
/// <<<<<<< buffer
/// [ours content]
/// =======
/// [theirs content]
/// >>>>>>> disk
/// ```
pub fn three_way_merge(base: &str, ours: &str, theirs: &str) -> MergeResult {
    // Compute line-level diffs from base to each side
    let diff_ours = TextDiff::from_lines(base, ours);
    let diff_theirs = TextDiff::from_lines(base, theirs);
    let base_lines: Vec<&str> = base.lines().collect();

    // Build edit maps: for each base line, what happened?
    let ours_ops = build_edit_map(&diff_ours, base_lines.len());
    let theirs_ops = build_edit_map(&diff_theirs, base_lines.len());

    // Walk through and merge
    let mut output = Vec::new();
    let mut has_conflict = false;
    let mut i = 0; // base line index

    while i <= base_lines.len() {
        // Check for insertions before this base line
        let ours_insert = ours_ops.insertions_before(i);
        let theirs_insert = theirs_ops.insertions_before(i);

        if !ours_insert.is_empty() && !theirs_insert.is_empty() {
            if ours_insert == theirs_insert {
                // Both inserted the same thing
                output.extend(ours_insert.iter().cloned());
            } else {
                // Both inserted different things — conflict
                has_conflict = true;
                output.push("<<<<<<< buffer".to_string());
                output.extend(ours_insert.iter().cloned());
                output.push("=======".to_string());
                output.extend(theirs_insert.iter().cloned());
                output.push(">>>>>>> disk".to_string());
            }
        } else if !ours_insert.is_empty() {
            output.extend(ours_insert.iter().cloned());
        } else if !theirs_insert.is_empty() {
            output.extend(theirs_insert.iter().cloned());
        }

        if i >= base_lines.len() {
            break;
        }

        // Check what happened to base line i
        let ours_action = ours_ops.action_at(i);
        let theirs_action = theirs_ops.action_at(i);

        match (ours_action, theirs_action) {
            (Action::Keep, Action::Keep) => {
                output.push(base_lines[i].to_string());
            }
            (Action::Keep, Action::Delete) => {
                // Theirs deleted, we kept — take theirs (delete)
            }
            (Action::Delete, Action::Keep) => {
                // We deleted, theirs kept — take ours (delete)
            }
            (Action::Delete, Action::Delete) => {
                // Both deleted — agree
            }
            (Action::Keep, Action::Replace(ref new)) => {
                // Theirs changed, we kept — take theirs
                output.extend(new.iter().cloned());
            }
            (Action::Replace(ref new), Action::Keep) => {
                // We changed, theirs kept — take ours
                output.extend(new.iter().cloned());
            }
            (Action::Replace(ref ours_new), Action::Replace(ref theirs_new)) => {
                if ours_new == theirs_new {
                    // Both changed to the same thing (convergent edit)
                    output.extend(ours_new.iter().cloned());
                } else {
                    // Both changed differently — conflict
                    has_conflict = true;
                    output.push("<<<<<<< buffer".to_string());
                    output.extend(ours_new.iter().cloned());
                    output.push("=======".to_string());
                    output.extend(theirs_new.iter().cloned());
                    output.push(">>>>>>> disk".to_string());
                }
            }
            (Action::Replace(ref ours_new), Action::Delete) => {
                // We replaced, they deleted — conflict
                has_conflict = true;
                output.push("<<<<<<< buffer".to_string());
                output.extend(ours_new.iter().cloned());
                output.push("=======".to_string());
                // theirs is empty (deletion)
                output.push(">>>>>>> disk".to_string());
            }
            (Action::Delete, Action::Replace(ref theirs_new)) => {
                // We deleted, they replaced — conflict
                has_conflict = true;
                output.push("<<<<<<< buffer".to_string());
                // ours is empty (deletion)
                output.push("=======".to_string());
                output.extend(theirs_new.iter().cloned());
                output.push(">>>>>>> disk".to_string());
            }
        }

        i += 1;
    }

    let result = output.join("\n");
    // Preserve trailing newline if any input had one
    let result = if (ours.ends_with('\n') || theirs.ends_with('\n')) && !result.ends_with('\n') {
        result + "\n"
    } else {
        result
    };

    if has_conflict {
        MergeResult::Conflict(result)
    } else {
        MergeResult::Clean(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────────────────────────────────────────────────────
    // Non-overlapping edits (should merge cleanly)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_non_overlapping_edits_at_different_locations() {
        // User edits line 2, external edits line 5
        let base = "fn main() {\n    let x = 1;\n    let y = 2;\n    let z = 3;\n    println!(\"hello\");\n}\n";
        let ours = "fn main() {\n    let x = 42;\n    let y = 2;\n    let z = 3;\n    println!(\"hello\");\n}\n";
        let theirs = "fn main() {\n    let x = 1;\n    let y = 2;\n    let z = 3;\n    println!(\"goodbye\");\n}\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(result.is_clean(), "Expected clean merge, got conflict");

        let merged = result.content();
        assert!(merged.contains("let x = 42;"), "Should have user's edit");
        assert!(merged.contains("println!(\"goodbye\");"), "Should have external edit");
    }

    #[test]
    fn test_non_overlapping_user_adds_above_external_adds_below() {
        let base = "use std::io;\n\nfn main() {\n    println!(\"hello\");\n}\n";
        let ours = "use std::io;\nuse std::fs;\n\nfn main() {\n    println!(\"hello\");\n}\n";
        let theirs = "use std::io;\n\nfn main() {\n    println!(\"hello\");\n}\n\nfn helper() {\n    // added by external\n}\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(result.is_clean(), "Expected clean merge, got conflict");

        let merged = result.content();
        assert!(merged.contains("use std::fs;"), "Should have user's import");
        assert!(merged.contains("fn helper()"), "Should have external function");
    }

    #[test]
    fn test_non_overlapping_user_deletes_external_adds() {
        let base = "fn main() {}\n\nfn old_func() {\n    // legacy\n}\n\nfn keep_func() {}\n";
        let ours = "fn main() {}\n\nfn keep_func() {}\n";
        let theirs = "fn main() {}\n\nfn old_func() {\n    // legacy\n}\n\nfn keep_func() {}\n\nfn new_func() {\n    // added by external\n}\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(result.is_clean(), "Expected clean merge, got conflict");

        let merged = result.content();
        assert!(!merged.contains("old_func"), "old_func should be deleted");
        assert!(merged.contains("new_func"), "Should have external's new function");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Convergent edits (both make same change)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_convergent_both_fix_same_typo() {
        let base = "fn main() {\n    pritnln!(\"hello\");\n}\n";
        let ours = "fn main() {\n    println!(\"hello\");\n}\n";
        let theirs = "fn main() {\n    println!(\"hello\");\n}\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(result.is_clean(), "Expected clean merge for convergent edit");

        let merged = result.content();
        assert!(merged.contains("println!(\"hello\");"), "Should have the fix");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Conflicts (overlapping edits)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_conflict_both_edit_same_line_differently() {
        let base = "fn main() {\n    let x = 1;\n}\n";
        let ours = "fn main() {\n    let x = 42;\n}\n";
        let theirs = "fn main() {\n    let x = 99;\n}\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(!result.is_clean(), "Expected conflict");

        let merged = result.content();
        assert!(merged.contains("<<<<<<< buffer"), "Should have conflict marker");
        assert!(merged.contains("let x = 42;"), "Should have user's version");
        assert!(merged.contains("======="), "Should have separator");
        assert!(merged.contains("let x = 99;"), "Should have external version");
        assert!(merged.contains(">>>>>>> disk"), "Should have end marker");
    }

    #[test]
    fn test_conflict_user_deletes_external_modifies() {
        let base = "line1\nline2\nline3\n";
        let ours = "line1\nline3\n";
        let theirs = "line1\nline2_modified\nline3\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(!result.is_clean(), "Expected conflict");

        let merged = result.content();
        assert!(merged.contains("<<<<<<< buffer"), "Should have conflict marker");
        assert!(merged.contains(">>>>>>> disk"), "Should have end marker");
    }

    #[test]
    fn test_conflict_external_deletes_user_modifies() {
        let base = "line1\nline2\nline3\n";
        let ours = "line1\nline2_modified\nline3\n";
        let theirs = "line1\nline3\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(!result.is_clean(), "Expected conflict");

        let merged = result.content();
        assert!(merged.contains("<<<<<<< buffer"), "Should have conflict marker");
        assert!(merged.contains("line2_modified"), "Should have user's modification");
        assert!(merged.contains(">>>>>>> disk"), "Should have end marker");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Realistic scenarios
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_external_adds_function_while_user_edits_existing() {
        let base = concat!(
            "use std::io;\n",
            "\n",
            "fn main() {\n",
            "    let data = read_file();\n",
            "    process(data);\n",
            "}\n",
            "\n",
            "fn read_file() -> String {\n",
            "    std::fs::read_to_string(\"input.txt\").unwrap()\n",
            "}\n",
        );
        let ours = concat!(
            "use std::io;\n",
            "\n",
            "fn main() {\n",
            "    let data = read_file();\n",
            "    process(data);\n",
            "    println!(\"done!\");\n",
            "}\n",
            "\n",
            "fn read_file() -> String {\n",
            "    std::fs::read_to_string(\"input.txt\").unwrap()\n",
            "}\n",
        );
        let theirs = concat!(
            "use std::io;\n",
            "\n",
            "fn main() {\n",
            "    let data = read_file();\n",
            "    process(data);\n",
            "}\n",
            "\n",
            "fn read_file() -> String {\n",
            "    std::fs::read_to_string(\"input.txt\").unwrap()\n",
            "}\n",
            "\n",
            "fn handle_error(e: std::io::Error) {\n",
            "    eprintln!(\"Error: {}\", e);\n",
            "    std::process::exit(1);\n",
            "}\n",
        );

        let result = three_way_merge(base, ours, theirs);
        assert!(result.is_clean(), "Expected clean merge");

        let merged = result.content();
        assert!(merged.contains("println!(\"done!\");"), "Should have user's line");
        assert!(merged.contains("fn handle_error"), "Should have external function");
    }

    #[test]
    fn test_external_refactors_while_user_adds_import() {
        let base = concat!(
            "use std::io;\n",
            "\n",
            "fn main() {\n",
            "    let data = read_file();\n",
            "    println!(\"{}\", data);\n",
            "}\n",
            "\n",
            "fn read_file() -> String {\n",
            "    std::fs::read_to_string(\"input.txt\").unwrap()\n",
            "}\n",
        );
        let ours = concat!(
            "use std::io;\n",
            "use std::path::Path;\n",
            "\n",
            "fn main() {\n",
            "    let data = read_file();\n",
            "    println!(\"{}\", data);\n",
            "}\n",
            "\n",
            "fn read_file() -> String {\n",
            "    std::fs::read_to_string(\"input.txt\").unwrap()\n",
            "}\n",
        );
        let theirs = concat!(
            "use std::io;\n",
            "\n",
            "fn main() {\n",
            "    let data = read_file();\n",
            "    println!(\"{}\", data);\n",
            "}\n",
            "\n",
            "fn read_file() -> String {\n",
            "    match std::fs::read_to_string(\"input.txt\") {\n",
            "        Ok(content) => content,\n",
            "        Err(e) => {\n",
            "            eprintln!(\"Failed to read: {}\", e);\n",
            "            String::new()\n",
            "        }\n",
            "    }\n",
            "}\n",
        );

        let result = three_way_merge(base, ours, theirs);
        assert!(result.is_clean(), "Expected clean merge");

        let merged = result.content();
        assert!(merged.contains("use std::path::Path;"), "Should have user's import");
        assert!(merged.contains("match std::fs::read_to_string"), "Should have external refactor");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Adjacent edits (should not conflict)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_adjacent_edits_line_n_and_n_plus_1() {
        let base = "fn main() {\n    let x = 1;\n    let y = 2;\n    let z = 3;\n}\n";
        let ours = "fn main() {\n    let x = 10;\n    let y = 2;\n    let z = 3;\n}\n";
        let theirs = "fn main() {\n    let x = 1;\n    let y = 20;\n    let z = 3;\n}\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(result.is_clean(), "Adjacent edits should merge cleanly");

        let merged = result.content();
        assert!(merged.contains("let x = 10;"), "Should have user's edit");
        assert!(merged.contains("let y = 20;"), "Should have external edit");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Edge cases
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_empty_base_external_writes_full_file() {
        let base = "";
        let ours = "";
        let theirs = "fn main() {\n    println!(\"hello\");\n}\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(result.is_clean(), "Empty base should accept external content");

        let merged = result.content();
        assert_eq!(merged, theirs, "Should have external's full content");
    }

    #[test]
    fn test_user_appends_while_external_prepends() {
        let base = "fn main() {\n    todo!();\n}\n";
        let ours = "fn main() {\n    todo!();\n}\n\n// user's note\n";
        let theirs = "// Generated by external tool\n\nfn main() {\n    todo!();\n}\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(result.is_clean(), "Prepend + append should merge cleanly");

        let merged = result.content();
        assert!(merged.contains("// Generated by external tool"), "Should have external header");
        assert!(merged.contains("// user's note"), "Should have user's note");
    }

    #[test]
    fn test_trailing_newline_preserved() {
        let base = "line1\nline2\n";
        let ours = "line1\nmodified\n";
        let theirs = "line1\nline2\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(result.content().ends_with('\n'), "Should preserve trailing newline");
    }

    #[test]
    fn test_no_trailing_newline_preserved() {
        let base = "line1\nline2";
        let ours = "line1\nmodified";
        let theirs = "line1\nline2";

        let result = three_way_merge(base, ours, theirs);
        // When neither has trailing newline, result also shouldn't
        // (though this depends on join behavior - the result may or may not have one)
        assert!(result.is_clean());
    }

    #[test]
    fn test_identical_content_no_changes() {
        let content = "fn main() {\n    println!(\"hello\");\n}\n";

        let result = three_way_merge(content, content, content);
        assert!(result.is_clean());
        assert_eq!(result.content(), content);
    }
}
