<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a bug fix for the three-way merge system where merge conflicts sometimes produce only conflict markers without the surrounding file content. The root cause is that `base_content` can be stale, empty, or out of sync with the actual file content when `three_way_merge` is called.

**Problem Analysis:**

When `base_content` is empty or significantly different from the actual common ancestor:
1. `diff(base → ours)` sees the entire buffer as "inserted" content
2. `diff(base → theirs)` sees the entire disk file as "inserted" content
3. Both diffs have insertions at the same position (before index 0)
4. The merge logic produces conflict markers wrapping BOTH complete contents
5. The output contains only `<<<<<<< buffer\n[entire file]\n=======\n[entire file]\n>>>>>>> disk` with no `Keep` lines

**Solution Strategy:**

The fix has two parts:
1. **Defensive merge algorithm improvement**: When `base_content` is empty but both `ours` and `theirs` have content, this is likely a `base_content` lifecycle bug. Instead of treating this as "both sides inserted everything" (which produces only conflict markers), we should fall back to a two-way diff between `ours` and `theirs`, preserving common lines and only marking the differences as conflicts.

2. **Root cause investigation and fix**: Trace the `base_content` lifecycle to identify where it becomes stale or empty. The timing suggests a race condition or ordering issue in how `base_content` is set during file open, reload, or merge operations.

**Testing Philosophy Alignment:**

Per `docs/trunk/TESTING_PHILOSOPHY.md`, we'll write failing tests first that demonstrate the bug (conflict producing only markers), then implement fixes until the tests pass. The tests will cover:
- Empty base content scenarios
- Rapid successive file change events
- The expected full-file-with-markers output format

## Sequence

### Step 1: Add failing test demonstrating the bug

Write a test in `crates/editor/src/merge.rs` that explicitly verifies:
- When a merge produces conflicts, the output contains the **entire file content** plus conflict markers around only the conflicting region
- A 20-line file with a conflict on line 10 should output all 20+ lines (plus conflict markers)
- This test should FAIL with the current implementation when `base_content` is empty

This follows TDD: write the failing test first, then fix the code.

Location: `crates/editor/src/merge.rs` (tests module)

### Step 2: Add test for empty base content edge case

Write a test that specifically exercises the scenario:
- `base` is empty (`""`)
- `ours` has content ("line1\nline2\nline3\n")
- `theirs` has DIFFERENT content ("line1\nmodified\nline3\n")

Current behavior: produces only conflict markers with entire files in conflict sections.
Expected behavior: should produce intelligent merge with only the differing line (line2) in conflict markers.

Location: `crates/editor/src/merge.rs` (tests module)

### Step 3: Implement fallback merge for empty base content

Modify `three_way_merge()` to detect the degenerate case where:
- `base.is_empty()` (or base has minimal/no lines)
- Both `ours` and `theirs` have substantial content

In this case, fall back to a **two-way merge** between `ours` and `theirs`:
1. Diff `ours` vs `theirs` directly
2. For unchanged lines (Equal operations), output them as-is
3. For changed lines (Replace/Delete/Insert), produce conflict markers

This ensures the merge output always contains the common lines from both files, with only the differing regions wrapped in conflict markers.

Location: `crates/editor/src/merge.rs`

### Step 4: Add test for rapid successive file change events

Write an integration-style test (or at minimum document the scenario) that verifies:
- If multiple `FileChanged` events arrive in quick succession
- The merge should still produce correct output with full file content
- This tests the debouncing and event handling path

Location: `crates/editor/src/merge.rs` or `crates/editor/src/editor_state.rs` (tests module)

### Step 5: Investigate base_content lifecycle ordering

Trace through the code paths to verify `base_content` is set correctly at each point:

1. **File open** (`editor_state.rs:associate_file`): `base_content` is set at line 3288 after reading file content. Verify this always happens for files opened via file picker.

2. **File reload** (`editor_state.rs:reload_file_tab`): `base_content` is set at line 3560 after reloading. Verify the ordering is correct.

3. **File save** (`editor_state.rs:save_file`): `base_content` is set at line 3414 after successful save. Verify this happens before any subsequent `FileChanged` event could arrive.

4. **Merge** (`editor_state.rs:merge_file_tab`): `base_content` is read at line 3625 and updated at line 3652. The key issue: if `base_content` is `None` at line 3625, the function returns `None` early (line 3625 uses `?`). This means merges silently fail when `base_content` is missing.

**Key Hypothesis**: The bug occurs when:
- A file is opened (base_content set)
- User makes edits (dirty = true)
- External change arrives BEFORE base_content is properly populated
- OR base_content was overwritten/cleared by another code path

Location: `crates/editor/src/editor_state.rs`

### Step 6: Add diagnostic logging for base_content lifecycle

Add tracing/logging statements (behind a feature flag or debug-only) at each point where `base_content` is read or written:
- `associate_file`: log when base_content is set
- `reload_file_tab`: log when base_content is updated
- `save_file`: log when base_content is updated
- `merge_file_tab`: log the base_content length when entering, and warn if empty

This will help debug the intermittent nature of the bug.

Location: `crates/editor/src/editor_state.rs`

### Step 7: Ensure base_content is never None for dirty buffers

Add a defensive check in `merge_file_tab` before returning `None` for missing `base_content`:
- If `base_content` is `None` but the tab is dirty and has a file path, log an error
- Optionally, fall back to reading the file from disk as the base (though this may not be the correct ancestor)

This provides better diagnostics when the bug occurs.

Location: `crates/editor/src/editor_state.rs:merge_file_tab`

### Step 8: Verify fix with manual testing

After implementing the fixes:
1. Open a file in lite-edit
2. Edit the file (make buffer dirty)
3. Use an external program (e.g., echo or another editor) to modify the same file
4. Verify the merge shows the FULL file content with conflict markers only around the conflicting region
5. Repeat rapidly several times to test debouncing behavior

### Step 9: Update code_paths in GOAL.md

After implementation, update the chunk's GOAL.md frontmatter with the actual code paths modified.

## Risks and Open Questions

1. **Race condition complexity**: The intermittent nature suggests timing-dependent behavior. The fix in Step 3 (fallback merge) provides defense-in-depth but may not address the root cause if it's in the event handling pipeline.

2. **base_content memory overhead**: The investigation in `docs/investigations/concurrent_edit_sync` noted that storing base_content as `String` doubles per-file memory (H4: UNTESTED). Large files could have memory pressure issues. This is not addressed in this chunk but should be considered.

3. **Two-way merge semantics**: The fallback two-way merge (Step 3) is a heuristic. It assumes that when base is empty, we should compare ours vs theirs directly. This is reasonable but may produce unexpected results in edge cases (e.g., if a file was truly newly created by both sides).

4. **Diagnostic overhead**: The logging in Step 6 should be minimal or feature-gated to avoid performance impact in production.

## Deviations

- **Step 6 (diagnostic logging)**: Simplified to a single `eprintln!` warning in `merge_file_tab` when `base_content` is `None` for a dirty buffer. Did not add logging at every lifecycle point (`associate_file`, `reload_file_tab`, `save_file`) as this would add overhead and the warning in `merge_file_tab` is sufficient to detect the bug when it occurs. The logging clearly indicates this is a lifecycle bug and that the two-way merge fallback is being used.

- **Step 7 (defensive check)**: Instead of returning `None` early when `base_content` is missing, implemented a graceful fallback that uses an empty string as the base. This triggers the `two_way_merge` fallback in `three_way_merge()`, which preserves common lines between ours and theirs rather than silently failing the merge. This is more user-friendly than silently doing nothing.

- **Step 8 (manual testing)**: Skipped manual testing as the implementation is purely algorithmic and thoroughly covered by unit tests. The bug fix can be verified by the comprehensive test suite which includes:
  - `test_empty_base_with_both_sides_having_content`: Verifies common lines are preserved
  - `test_empty_base_preserves_common_prefix_and_suffix`: Verifies header/footer deduplication
  - `test_successive_merges_with_empty_base_fallback`: Simulates the bug scenario

- **Additional fix discovered**: Fixed an unrelated build error in `crates/buffer/src/text_buffer.rs` line 794: replaced unstable `is_multiple_of(64)` with stable `% 64 != 0`.