<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk implements line-level three-way merge for dirty buffers when external file changes are detected. The approach follows the validated prototype from the concurrent_edit_sync investigation (`docs/investigations/concurrent_edit_sync/prototypes/three_way_merge_test.rs`).

**High-level strategy:**

1. Add the `similar` crate as a workspace dependency for line-level diffing
2. Create a new `merge.rs` module in `crates/editor/src/` containing:
   - `MergeResult` enum (Clean/Conflict variants)
   - `three_way_merge(base, ours, theirs)` function implementing the diff3 algorithm
   - Supporting types (`Action`, `EditMap`) for tracking per-line edits
3. Wire the merge into the `FileChanged` handler in `drain_loop.rs`:
   - When `dirty == true`, call `merge_file_tab` instead of skipping
4. Add `merge_file_tab` method to `EditorState` that:
   - Performs the three-way merge
   - Updates buffer content with merged result
   - Preserves/adjusts cursor position
   - Updates `base_content` to the new disk content
   - Re-applies syntax highlighting

**Testing philosophy alignment:**

Per `docs/trunk/TESTING_PHILOSOPHY.md`, tests are written first for behavior with meaningful semantics. The merge algorithm has clear semantic properties (non-overlapping edits merge cleanly, conflicts produce markers, etc.) that map directly to test cases. The prototype already validated 12 scenarios — we'll adapt these as unit tests.

The integration with `handle_file_changed` touches the "humble object" boundary (event loop), so we'll focus unit tests on the pure merge function and the `merge_file_tab` state mutation, not the event routing.

## Sequence

### Step 1: Add `similar` crate to workspace

Add the `similar` crate to `crates/editor/Cargo.toml` as a dependency. This provides the `TextDiff::from_lines` API for computing line-level diffs.

**Location:** `crates/editor/Cargo.toml`

**Validation:** `cargo check` passes with the new dependency.

### Step 2: Create merge module with MergeResult type

Create `crates/editor/src/merge.rs` with the `MergeResult` enum:

```rust
// Chunk: docs/chunks/three_way_merge - Line-level three-way merge for concurrent edits

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
```

**Location:** `crates/editor/src/merge.rs`

**Validation:** Module compiles and `MergeResult` can be constructed.

### Step 3: Implement Action enum and EditMap

Add the supporting types that track what happened to each base line:

```rust
/// Represents the action taken on a base line.
#[derive(Debug, Clone, PartialEq)]
enum Action {
    /// Line unchanged from base
    Keep,
    /// Line deleted
    Delete,
    /// Line replaced with new content
    Replace(Vec<String>),
}

/// Tracks edits from base to a derived version (ours or theirs).
struct EditMap {
    /// Action for each base line index
    actions: Vec<Action>,
    /// Lines inserted before each base line index (key = base index)
    /// Index base_lines.len() means insertions at the end
    insertions: Vec<Vec<String>>,
}
```

Add `EditMap::action_at(base_idx)` and `EditMap::insertions_before(base_idx)` accessor methods.

Implement `fn build_edit_map(diff: &TextDiff, base_len: usize) -> EditMap` that walks the diff ops and populates the action/insertion vectors.

**Location:** `crates/editor/src/merge.rs`

**Validation:** Unit tests for `build_edit_map` with simple diffs.

### Step 4: Implement three_way_merge function

Implement the core merge function following the prototype's algorithm:

```rust
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
pub fn three_way_merge(base: &str, ours: &str, theirs: &str) -> MergeResult {
    // 1. Compute line-level diffs from base to each side
    // 2. Build edit maps for both sides
    // 3. Walk through base lines, merging non-overlapping edits
    // 4. For overlapping edits:
    //    - Same content → take it (convergent edits)
    //    - Different content → emit conflict markers
    // 5. Return Clean or Conflict based on whether markers were emitted
}
```

The algorithm handles:
- `Keep/Keep` → output base line
- `Keep/Delete` or `Delete/Keep` → accept the deletion
- `Delete/Delete` → agree on deletion
- `Keep/Replace` or `Replace/Keep` → accept the replacement
- `Replace/Replace` with same content → accept the convergent edit
- `Replace/Replace` with different content → conflict
- `Replace/Delete` or `Delete/Replace` → conflict
- Insertions before each base line are merged similarly

Conflict markers use the git-style format:
```
<<<<<<< buffer
[ours content]
=======
[theirs content]
>>>>>>> disk
```

**Location:** `crates/editor/src/merge.rs`

**Validation:** Comprehensive unit tests covering the 12 prototype scenarios plus edge cases.

### Step 5: Write unit tests for three_way_merge

Create test cases adapted from the investigation prototype:

1. Non-overlapping: edits at different locations
2. Non-overlapping: user adds above, external adds below
3. Non-overlapping: user deletes function, external adds different function
4. Convergent: both make the same change
5. Conflict: both edit the same line differently
6. Conflict: user deletes, external modifies same line
7. Claude adds function while user edits existing one
8. Claude refactors function body while user adds import
9. Adjacent edits: line N and line N+1 (should merge cleanly)
10. Empty base: external program writes full file
11. User appends at end while external prepends at top
12. Delete-vs-modify conflict (both directions)

Tests should be in a `#[cfg(test)] mod tests` block within `merge.rs`.

**Location:** `crates/editor/src/merge.rs`

**Validation:** `cargo test -p lite-edit merge` passes all tests.

### Step 6: Add merge module to editor lib

Add `pub mod merge;` to `crates/editor/src/lib.rs` to expose the module.

**Location:** `crates/editor/src/lib.rs`

**Validation:** Module is accessible from other files in the crate.

### Step 7: Implement merge_file_tab method in EditorState

Add a method to `EditorState` that handles the dirty buffer merge case:

```rust
// Chunk: docs/chunks/three_way_merge - Merge dirty buffer with external changes
/// Merges external file changes into a dirty buffer using three-way merge.
///
/// This is called when a FileChanged event arrives for a tab with dirty == true.
///
/// # Behavior
///
/// - Reads the new disk content
/// - Performs three-way merge: base_content → buffer, base_content → disk
/// - On clean merge: replaces buffer content, preserves/adjusts cursor
/// - On conflict: replaces buffer content (including markers)
/// - Updates base_content to new disk content
/// - Dirty flag remains true (user still has unsaved changes)
/// - Re-applies syntax highlighting
///
/// # Returns
///
/// `Some(MergeResult)` if merge was performed, `None` if tab not found or
/// not a dirty file tab.
pub fn merge_file_tab(&mut self, path: &Path) -> Option<MergeResult> {
    // 1. Find workspace and tab (similar to reload_file_tab)
    // 2. Verify tab.dirty == true
    // 3. Read disk content
    // 4. Get base_content (must exist for dirty buffer)
    // 5. Get current buffer content
    // 6. Call three_way_merge(base, buffer, disk)
    // 7. Replace buffer with merged content
    // 8. Clamp cursor to new buffer bounds
    // 9. Update base_content = disk_content
    // 10. Re-apply syntax highlighting
    // 11. Mark full viewport dirty
    // 12. Return the MergeResult
}
```

The method follows the same pattern as `reload_file_tab` but performs merge instead of reload.

**Location:** `crates/editor/src/editor_state.rs`

**Validation:** Unit tests for merge_file_tab behavior.

### Step 8: Wire merge into handle_file_changed

Modify the `handle_file_changed` method in `drain_loop.rs` to call `merge_file_tab` for dirty buffers:

```rust
fn handle_file_changed(&mut self, path: std::path::PathBuf) {
    if self.state.is_file_change_suppressed(&path) {
        return;
    }

    // Try reload first (handles clean buffers)
    if self.state.reload_file_tab(&path) {
        return;
    }

    // If reload returned false and a matching dirty tab exists, try merge
    // Chunk: docs/chunks/three_way_merge - Merge for dirty buffers
    let _merge_result = self.state.merge_file_tab(&path);
    // Note: merge_result is returned but we don't need to act on it here.
    // The conflict_mode_lifecycle chunk will add handling for conflicts.
}
```

**Location:** `crates/editor/src/drain_loop.rs`

**Validation:** Manual test: edit a file, make external change, observe merge behavior.

### Step 9: Update GOAL.md with code_paths

Update the chunk's GOAL.md frontmatter with the files being created/modified:

```yaml
code_paths:
  - crates/editor/src/merge.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/drain_loop.rs
  - crates/editor/src/lib.rs
  - crates/editor/Cargo.toml
```

**Location:** `docs/chunks/three_way_merge/GOAL.md`

## Dependencies

- **base_snapshot_reload chunk (ACTIVE)**: This chunk depends on `Tab.base_content` and `reload_file_tab` being implemented. The `base_content` field provides the merge base, and the reload logic provides the pattern for file tab updates.

- **similar crate**: Must be added to workspace dependencies. Pure Rust, no system deps, compatible with the existing dependency profile.

## Risks and Open Questions

### Trailing newline handling

The prototype handles trailing newlines by checking if either input ends with `\n` and ensuring the output does too. Need to verify this works correctly with the `TextBuffer` which may or may not store trailing newlines consistently.

**Mitigation:** Add explicit test cases for trailing newline scenarios.

### Large file performance

The merge algorithm is O(n) in the number of lines for diff computation and merge walking. For very large files (100k+ lines), this could cause a perceptible pause.

**Mitigation:** This is acceptable for the initial implementation. The investigation noted that files this large are rare in typical usage. If needed, future work could add progress indication or async merge.

### base_content missing for dirty buffer

If somehow a dirty buffer has `base_content = None`, the merge cannot proceed. This shouldn't happen in normal operation (base_content is set on file load), but defensive handling is needed.

**Mitigation:** If base_content is None, treat as if we can't merge and leave the buffer unchanged. Log a warning for debugging.

### Cursor position after conflict

When conflict markers are inserted, the cursor position may end up in an unexpected location. The goal states "cursor position is preserved or adjusted to the first changed line."

**Mitigation:** Use the same cursor clamping as reload_file_tab. For conflicts specifically, could position cursor at first conflict marker, but this is a nice-to-have for the conflict_mode_lifecycle chunk.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
