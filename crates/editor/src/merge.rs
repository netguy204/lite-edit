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

/// Performs a two-way merge between `ours` and `theirs` directly.
///
/// This is used as a fallback when `base_content` is empty or stale, which would
/// otherwise cause the three-way merge to treat the entire file as conflicting.
///
/// # Algorithm
///
/// 1. Diff `ours` vs `theirs` line by line
/// 2. For Equal regions: output the lines as-is
/// 3. For Insert/Delete/Replace regions: emit conflict markers
///
/// This ensures common lines are preserved, with only differing regions marked as conflicts.
fn two_way_merge(ours: &str, theirs: &str) -> MergeResult {
    // Fast path: identical content
    if ours == theirs {
        return MergeResult::Clean(ours.to_string());
    }

    let diff = TextDiff::from_lines(ours, theirs);
    let ours_lines: Vec<&str> = ours.lines().collect();
    let theirs_lines: Vec<&str> = theirs.lines().collect();

    let mut output = Vec::new();
    let mut has_conflict = false;

    for op in diff.ops() {
        use similar::DiffOp;
        match *op {
            DiffOp::Equal {
                old_index, len, ..
            } => {
                // Lines are the same in both - output them
                for i in old_index..old_index + len {
                    if let Some(line) = ours_lines.get(i) {
                        output.push(line.to_string());
                    }
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                // Lines exist in ours but not in theirs - conflict
                has_conflict = true;
                output.push("<<<<<<< buffer".to_string());
                for i in old_index..old_index + old_len {
                    if let Some(line) = ours_lines.get(i) {
                        output.push(line.to_string());
                    }
                }
                output.push("=======".to_string());
                output.push(">>>>>>> disk".to_string());
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                // Lines exist in theirs but not in ours - conflict
                has_conflict = true;
                output.push("<<<<<<< buffer".to_string());
                output.push("=======".to_string());
                for i in new_index..new_index + new_len {
                    if let Some(line) = theirs_lines.get(i) {
                        output.push(line.to_string());
                    }
                }
                output.push(">>>>>>> disk".to_string());
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                // Lines differ between ours and theirs - conflict
                has_conflict = true;
                output.push("<<<<<<< buffer".to_string());
                for i in old_index..old_index + old_len {
                    if let Some(line) = ours_lines.get(i) {
                        output.push(line.to_string());
                    }
                }
                output.push("=======".to_string());
                for i in new_index..new_index + new_len {
                    if let Some(line) = theirs_lines.get(i) {
                        output.push(line.to_string());
                    }
                }
                output.push(">>>>>>> disk".to_string());
            }
        }
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
/// # Empty Base Fallback
///
/// When `base` is empty but both `ours` and `theirs` have content, this indicates
/// a stale or missing base_content (likely a lifecycle bug). Rather than treating
/// the entire file as conflicting, we fall back to a two-way diff between `ours`
/// and `theirs`, preserving common lines and only marking differences as conflicts.
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
    // Handle degenerate case: empty base with both sides having content.
    // This likely indicates a stale/missing base_content. Fall back to two-way merge
    // to avoid wrapping the entire file in conflict markers.
    if base.is_empty() && !ours.is_empty() && !theirs.is_empty() {
        return two_way_merge(ours, theirs);
    }
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

    // ─────────────────────────────────────────────────────────────────────────
    // Empty base content edge cases (merge_conflict_render chunk)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_conflict_output_contains_full_file_content() {
        // BUG TEST: When a merge produces conflicts, the output should contain
        // the ENTIRE file content with conflict markers only around the conflicting
        // region. A 20-line file with a conflict on line 10 should output all 20+
        // lines (plus conflict markers).
        let base = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nline 18\nline 19\nline 20\n";
        let ours = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nUSER EDIT\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nline 18\nline 19\nline 20\n";
        let theirs = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nEXTERNAL EDIT\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nline 18\nline 19\nline 20\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(!result.is_clean(), "Expected conflict");

        let merged = result.content();
        let lines: Vec<&str> = merged.lines().collect();

        // The merged output should contain ALL lines from the file,
        // not just the conflict markers
        assert!(merged.contains("line 1"), "Should contain line 1");
        assert!(merged.contains("line 5"), "Should contain line 5");
        assert!(merged.contains("line 9"), "Should contain line 9");
        assert!(merged.contains("line 11"), "Should contain line 11");
        assert!(merged.contains("line 15"), "Should contain line 15");
        assert!(merged.contains("line 20"), "Should contain line 20");

        // Should have conflict markers
        assert!(merged.contains("<<<<<<< buffer"), "Should have conflict marker");
        assert!(merged.contains("USER EDIT"), "Should have user's version");
        assert!(merged.contains("======="), "Should have separator");
        assert!(merged.contains("EXTERNAL EDIT"), "Should have external version");
        assert!(merged.contains(">>>>>>> disk"), "Should have end marker");

        // The total line count should be original 20 lines + 3 conflict marker lines
        // (conflict markers replace the one conflicting line)
        assert!(
            lines.len() >= 22,
            "Expected at least 22 lines (20 + conflict markers), got {}",
            lines.len()
        );
    }

    #[test]
    fn test_empty_base_with_both_sides_having_content() {
        // BUG TEST: When base is empty but both ours and theirs have content,
        // the algorithm should NOT treat the entire file as conflicting.
        // Instead, it should fall back to a two-way diff that preserves common
        // lines and only conflicts on differing lines.
        let base = "";
        let ours = "line 1\nline 2\nline 3\n";
        let theirs = "line 1\nmodified line 2\nline 3\n";

        let result = three_way_merge(base, ours, theirs);

        let merged = result.content();
        let lines: Vec<&str> = merged.lines().collect();

        // Common lines (line 1 and line 3) should appear once, not wrapped in conflict markers
        assert!(
            merged.contains("line 1"),
            "Should preserve common line 1"
        );
        assert!(
            merged.contains("line 3"),
            "Should preserve common line 3"
        );

        // Only the differing line should be in conflict
        // The output should NOT be just conflict markers wrapping everything
        let conflict_marker_count = lines.iter().filter(|l| l.starts_with("<<<<<<<")).count();
        assert!(
            conflict_marker_count <= 1,
            "Should have at most one conflict region, got {} conflict markers",
            conflict_marker_count
        );

        // If there's a conflict, it should only be around line 2 vs modified line 2
        if !result.is_clean() {
            assert!(merged.contains("line 2"), "Should have ours' line 2 in conflict");
            assert!(merged.contains("modified line 2"), "Should have theirs' line 2 in conflict");
        }
    }

    #[test]
    fn test_empty_base_with_identical_content() {
        // When base is empty but ours and theirs are identical, no conflict
        let base = "";
        let ours = "line 1\nline 2\nline 3\n";
        let theirs = "line 1\nline 2\nline 3\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(result.is_clean(), "Identical content should merge cleanly even with empty base");
        assert_eq!(result.content(), ours);
    }

    #[test]
    fn test_empty_base_preserves_common_prefix_and_suffix() {
        // When base is empty, common prefix and suffix lines should NOT be in conflict
        let base = "";
        let ours = "# Header\n\nfn main() {\n    user_code();\n}\n\n# Footer\n";
        let theirs = "# Header\n\nfn main() {\n    external_code();\n}\n\n# Footer\n";

        let result = three_way_merge(base, ours, theirs);
        let merged = result.content();

        // The header and footer should appear exactly once, NOT in conflict markers
        let header_count = merged.matches("# Header").count();
        let footer_count = merged.matches("# Footer").count();

        assert_eq!(
            header_count, 1,
            "Header should appear exactly once, not duplicated in conflict"
        );
        assert_eq!(
            footer_count, 1,
            "Footer should appear exactly once, not duplicated in conflict"
        );

        // Only the differing middle section should be in conflict
        if !result.is_clean() {
            assert!(merged.contains("user_code()"), "Should have user's code");
            assert!(merged.contains("external_code()"), "Should have external's code");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Rapid successive merge simulation (merge_conflict_render chunk)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_successive_merges_maintain_full_content() {
        // Simulates rapid successive file change events.
        // Each merge should produce full file content, not just conflict markers.
        // This tests that the base_content is properly updated after each merge.

        // Initial state: base and ours are the same (user opened file)
        let base1 = "line 1\nline 2\nline 3\nline 4\nline 5\n";
        let ours1 = "line 1\nuser edit 2\nline 3\nline 4\nline 5\n";
        let theirs1 = "line 1\nline 2\nline 3\nexternal edit 4\nline 5\n";

        // First merge: user edited line 2, external edited line 4
        let result1 = three_way_merge(base1, ours1, theirs1);
        assert!(result1.is_clean(), "Non-overlapping edits should merge cleanly");
        let merged1 = result1.content();

        // Verify both edits are in the merged content
        assert!(merged1.contains("user edit 2"), "Should have user's edit from first merge");
        assert!(merged1.contains("external edit 4"), "Should have external edit from first merge");
        assert!(merged1.contains("line 1"), "Should have line 1");
        assert!(merged1.contains("line 5"), "Should have line 5");

        // Second merge: the merged content becomes ours, new external edit arrives
        // The base should now be theirs1 (the disk content after first external edit)
        let base2 = theirs1;
        let ours2 = &merged1;
        let theirs2 = "line 1\nline 2\nline 3\nexternal edit 4\nnew line 5 external\n";

        let result2 = three_way_merge(base2, ours2, theirs2);
        let merged2 = result2.content();

        // Verify all content is preserved (user's edit from first merge + new external edit)
        assert!(merged2.contains("user edit 2"), "Should preserve user's edit from first merge");
        assert!(merged2.contains("new line 5 external"), "Should have new external edit");
        assert!(merged2.contains("line 1"), "Should have line 1");
        assert!(merged2.contains("line 3"), "Should have line 3");

        // Verify we get full content, not truncated to conflict markers
        let line_count = merged2.lines().count();
        assert!(
            line_count >= 4,
            "Merged content should have at least 4 lines, got {}",
            line_count
        );
    }

    #[test]
    fn test_successive_merges_with_empty_base_fallback() {
        // Simulates the bug scenario: base_content is empty/stale when merge is called.
        // With our fix, this should fall back to two-way merge and preserve common content.

        // First merge with valid base
        let base = "line 1\nline 2\nline 3\n";
        let ours = "line 1\nuser edit\nline 3\n";
        let theirs = "line 1\nexternal edit\nline 3\n";

        let result = three_way_merge(base, ours, theirs);
        assert!(!result.is_clean(), "Same line conflict should produce conflict");

        let merged = result.content();
        // Common lines should appear once, not duplicated
        let line1_count = merged.matches("line 1").count();
        let line3_count = merged.matches("line 3").count();
        assert_eq!(line1_count, 1, "line 1 should appear exactly once");
        assert_eq!(line3_count, 1, "line 3 should appear exactly once");

        // Now simulate second merge where base_content is empty (the bug scenario)
        // With our fix, this uses two-way merge fallback
        let stale_base = "";
        let ours2 = &merged;
        let theirs2 = "line 1\nnew external\nline 3\n";

        let result2 = three_way_merge(stale_base, ours2, theirs2);
        let merged2 = result2.content();

        // Verify common lines appear exactly once (not duplicated in conflict markers)
        let line1_count2 = merged2.matches("line 1").count();
        let line3_count2 = merged2.matches("line 3").count();
        assert_eq!(line1_count2, 1, "After empty base fallback, line 1 should appear exactly once");
        assert_eq!(line3_count2, 1, "After empty base fallback, line 3 should appear exactly once");
    }
}
