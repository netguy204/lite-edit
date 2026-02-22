<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk implements three improvements to the content tab bar:

1. **Clickable tab titles** – Verify and exercise the existing `handle_tab_bar_click` path end-to-end
2. **Labels derived from associated file path** – Change `TabInfo` to compute labels at render time from `Tab.associated_file` rather than copying the stale `Tab.label`
3. **Left-truncation** – When a label exceeds available width, truncate from the left and prepend `…`, preserving the file extension and end of filename

The implementation follows the Humble View Architecture: all label derivation and truncation logic is pure computation, easily unit-tested without GPU or platform dependencies. The click-to-switch behavior is already implemented but needs verification tests.

**Key insight:** The current `TabInfo::from_tab` copies `tab.label` verbatim. We need to change this to derive the label from `tab.associated_file` for file tabs (`TabKind::File` with `Some(path)`), falling back to `tab.label` only for non-file tabs (terminals, agent output) or when `associated_file` is `None`.

## Subsystem Considerations

No subsystems are directly relevant to this chunk. The work is entirely within the tab bar rendering module.

## Sequence

### Step 1: Write failing tests for derived labels

Add tests in `tab_bar.rs` that verify:
- File tab label is derived from `associated_file.file_name()`, not from `tab.label`
- When `associated_file` is `None`, label falls back to `"Untitled"`
- Non-file tabs (terminal, agent) continue to use the static `tab.label`
- Disambiguation still works when multiple tabs share the same base filename

These tests should fail initially because `TabInfo::from_tab` currently clones `tab.label`.

Location: `crates/editor/src/tab_bar.rs` (existing `#[cfg(test)]` module)

### Step 2: Modify `TabInfo::from_tab` to derive label for file tabs

Change the function signature to:
```rust
pub fn from_tab(tab: &Tab, index: usize, is_active: bool) -> Self
```

The implementation should:
1. For `TabKind::File` with `associated_file: Some(path)`:
   - Extract `path.file_name()` as the label
   - Fall back to `"Untitled"` if `file_name()` returns `None` (e.g., path ends in `..`)
2. For `TabKind::File` with `associated_file: None`:
   - Use `"Untitled"` as the label
3. For all other `TabKind` values (Terminal, AgentOutput, Diff):
   - Use `tab.label.clone()` (existing behavior)

After this change, the tests from Step 1 should pass.

Location: `crates/editor/src/tab_bar.rs#TabInfo::from_tab`

### Step 3: Update `disambiguate_labels` to use `associated_file` directly

The current `disambiguate_labels` function accesses `workspace.tabs[idx].associated_file` to add parent directory info. This approach is correct but depends on the initial label being the base filename. Verify that this still works correctly with the new derived labels.

If any adjustment is needed (e.g., the function now sees derived labels that might already be "Untitled"), update accordingly.

Location: `crates/editor/src/tab_bar.rs#disambiguate_labels`

### Step 4: Write failing tests for left-truncation

Add tests that verify:
- When a label exceeds `max_chars`, the label is left-truncated with leading `…`
- Example: `"very_long_module_name.rs"` with max_chars=10 → `"…e_name.rs"`
- Disambiguated labels (e.g., `"src/main.rs"`) are also left-truncated correctly
- Short labels that fit within max_chars are unchanged
- Single-character truncation produces just `"…"` (edge case)

These tests should fail because the current truncation is right-truncation.

Location: `crates/editor/src/tab_bar.rs` (new test functions)

### Step 5: Implement left-truncation in `TabBarGlyphBuffer::update`

Change the truncation logic in Phase 6 (Tab Labels) from:
```rust
let label: String = if tab_info.label.chars().count() > max_chars && max_chars > 3 {
    let truncated: String = tab_info.label.chars().take(max_chars - 1).collect();
    format!("{}…", truncated)
} else {
    tab_info.label.chars().take(max_chars).collect()
};
```

To left-truncation:
```rust
let label: String = {
    let char_count = tab_info.label.chars().count();
    if char_count > max_chars && max_chars > 1 {
        // Left-truncate: skip (char_count - max_chars + 1) chars, prepend ellipsis
        let skip = char_count - max_chars + 1;
        let truncated: String = tab_info.label.chars().skip(skip).collect();
        format!("…{}", truncated)
    } else {
        tab_info.label.chars().take(max_chars).collect()
    }
};
```

After this change, the tests from Step 4 should pass.

Location: `crates/editor/src/tab_bar.rs#TabBarGlyphBuffer::update` (Phase 6)

### Step 6: Update existing truncation tests

The existing test `test_tab_labels_truncated_to_max_width` verifies that tab width is clamped to `TAB_MAX_WIDTH`. This test focuses on tab geometry, not label content, so it should still pass.

Review any other truncation-related tests and update their assertions if they expected right-truncation output.

Location: `crates/editor/src/tab_bar.rs` (existing tests)

### Step 7: Write tests for click-to-switch behavior

Add integration-style tests in `editor_state.rs` that verify:
- Clicking a tab body switches to that tab
- Clicking the active tab is a no-op (no state change, no crash)
- Tab index returned by geometry matches the workspace tab indices

These tests use the existing `EditorState` and `MouseEvent` infrastructure without requiring a GPU.

Location: `crates/editor/src/editor_state.rs` (existing `#[cfg(test)]` module)

### Step 8: Verify coordinate transformation is correct

The `handle_tab_bar_click` function receives screen coordinates and must correctly transform them to tab bar local coordinates. The `tab_bar_layout_fixes` chunk should have corrected any coordinate issues, but we should verify this works correctly.

Write a test that constructs a known geometry, simulates a click at a specific position, and verifies the correct tab is selected.

Location: `crates/editor/src/editor_state.rs` (test function)

### Step 9: Update GOAL.md code_references

After implementation, update the chunk's `code_references` field with symbolic references to the modified symbols:
- `crates/editor/src/tab_bar.rs#TabInfo::from_tab` – Derives label from associated_file
- `crates/editor/src/tab_bar.rs#TabBarGlyphBuffer::update` – Left-truncation logic

## Dependencies

- `content_tab_bar` (ACTIVE) – Provides the tab bar structure, `TabInfo`, `TabBarGlyphBuffer`
- `tab_bar_layout_fixes` (HISTORICAL) – Corrected coordinate accounting for click handling

Both dependencies are already complete.

## Risks and Open Questions

1. **Disambiguation edge cases**: When two tabs have the same filename AND the same parent directory name (e.g., two `lib.rs` in different `src/` directories from different workspaces), the current single-level disambiguation may produce identical labels. This is an existing limitation; this chunk doesn't address it.

2. **Non-UTF8 paths**: `file_name().to_str()` returns `None` for non-UTF8 paths. The implementation falls back to `"Untitled"` in this case, which may be confusing but is safe.

3. **Performance**: Deriving labels at render time instead of caching them adds string allocation per render. With typical tab counts (1-20), this is negligible. If performance becomes an issue, labels could be cached on `Tab` and invalidated when `associated_file` changes.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
