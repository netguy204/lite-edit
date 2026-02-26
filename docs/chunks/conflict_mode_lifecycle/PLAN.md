<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk implements the conflict mode lifecycle by extending the existing three-way merge infrastructure. When `merge_file_tab()` returns a `Conflict` result, the tab enters conflict mode which suppresses further auto-merge until the user saves.

**Strategy:**
1. Add a `conflict_mode: bool` field to `Tab` (default false)
2. Set `conflict_mode = true` when `three_way_merge` returns `MergeResult::Conflict`
3. Add a check in `handle_file_changed` that ignores `FileChanged` events when `conflict_mode == true`
4. In `save_file()`, clear conflict mode and trigger a re-check for pending disk changes
5. Add a distinct visual indicator (colored icon/marker) in the tab bar for conflict mode tabs

The approach builds on:
- `Tab` struct in `workspace.rs` (add `conflict_mode` field)
- `merge_file_tab()` in `editor_state.rs` (set conflict_mode on conflict result)
- `handle_file_changed()` in `drain_loop.rs` (check conflict_mode before processing)
- `save_file()` in `editor_state.rs` (clear conflict_mode, check for disk changes)
- `tab_bar.rs` (render conflict indicator with distinct color)
- `TabInfo` in `tab_bar.rs` (add `is_conflict` field)

## Subsystem Considerations

No subsystems are directly relevant to this chunk. The conflict mode lifecycle is a new feature that extends the existing file change handling infrastructure without touching any documented subsystems.

## Sequence

### Step 1: Add `conflict_mode` field to Tab

Add a `conflict_mode: bool` field to the `Tab` struct with a default value of `false`. Initialize it in all `Tab` constructors (`new_file`, `empty_file`, `new_agent`, `new_terminal`).

**Location:** `crates/editor/src/workspace.rs`

**Verification:** Compile check - the code should build without errors.

---

### Step 2: Add `is_conflict` field to TabInfo

Extend the `TabInfo` struct with an `is_conflict: bool` field. Update `TabInfo::from_tab()` to populate it from `tab.conflict_mode`.

**Location:** `crates/editor/src/tab_bar.rs`

**Verification:** Compile check.

---

### Step 3: Add conflict indicator color constant

Add a `CONFLICT_INDICATOR_COLOR` constant in the tab bar colors section. Use a distinctive color (e.g., Catppuccin red/pink `#f38ba8` or similar) that stands out from the yellow dirty indicator and blue unread indicator.

**Location:** `crates/editor/src/tab_bar.rs`

**Verification:** Compile check.

---

### Step 4: Render conflict indicator in tab bar

Update the Phase 4 (Dirty/Unread Indicators) section of `TabBarBuffer::update()` to prioritize conflict mode:
- If `is_conflict && is_dirty`: show conflict indicator (new color)
- Else if `is_dirty`: show dirty indicator (yellow)
- Else if `is_unread`: show unread indicator (blue)

This ensures conflict mode has the highest visual priority while still showing that the tab is dirty.

**Location:** `crates/editor/src/tab_bar.rs`

**Verification:** Compile check.

---

### Step 5: Set conflict_mode when merge produces conflicts

Modify `merge_file_tab()` to set `tab.conflict_mode = true` when the merge result is `MergeResult::Conflict`. The function already returns `Option<MergeResult>`, so access the result to check if it's a conflict.

**Location:** `crates/editor/src/editor_state.rs`

**Verification:** Write a unit test that creates a conflict scenario and verifies `conflict_mode` is set to `true`.

---

### Step 6: Suppress FileChanged events for tabs in conflict mode

Modify `handle_file_changed()` in `drain_loop.rs` to check if the target tab is in conflict mode before processing. If `conflict_mode == true`, ignore the event (return early).

This requires adding a helper method to check conflict mode for a path, since `handle_file_changed` only has the path.

**Location:** `crates/editor/src/drain_loop.rs` and `crates/editor/src/editor_state.rs`

Add helper method `is_tab_in_conflict_mode(path: &Path) -> bool` to `EditorState`:
- Search all workspaces for a tab with the given associated_file
- Return `tab.conflict_mode` if found, `false` otherwise

**Verification:** Write a unit test that:
1. Creates a tab with conflict_mode = true
2. Simulates a FileChanged event for that path
3. Verifies the buffer content is unchanged (event was ignored)

---

### Step 7: Clear conflict_mode on save and re-check disk

Modify `save_file()` to:
1. Clear `conflict_mode = false` after successful save
2. After clearing conflict mode and updating `base_content`, check if disk content differs from saved content
3. If disk differs (external edit arrived during conflict resolution), trigger a new merge cycle

**Location:** `crates/editor/src/editor_state.rs`

**Implementation detail:** After save completes successfully:
```rust
// Clear conflict mode
tab.conflict_mode = false;

// Re-check disk for changes that arrived during conflict resolution
if let Ok(disk_bytes) = std::fs::read(&path) {
    let disk_content = String::from_utf8_lossy(&disk_bytes).to_string();
    if disk_content != content {
        // Disk has changed again - need to merge
        // base_content is already updated to saved content
        // Call merge_file_tab to handle this
    }
}
```

Note: The re-merge after save requires careful sequencing since `base_content` was just updated to the saved content. If disk differs, we need to merge with the new base.

**Verification:** Write a unit test that:
1. Creates a tab in conflict mode
2. Calls save
3. Verifies conflict_mode is false after save
4. Verifies base_content equals saved content

---

### Step 8: Add accessor for Tab conflict mode by path

Add a method to allow checking and modifying conflict mode by path:
- `find_tab_by_path()` already exists for finding tabs
- Add `find_tab_mut_by_path()` if not present, or use existing one

**Location:** `crates/editor/src/workspace.rs`

**Verification:** Compile check (may already exist based on merge_file_tab usage).

---

### Step 9: Write integration test for full conflict mode lifecycle

Write a comprehensive test that exercises the full lifecycle:
1. Create a file tab with some content, set base_content
2. Modify buffer (make it dirty)
3. Simulate external change that conflicts
4. Verify conflict markers appear and conflict_mode is true
5. Simulate another FileChanged event
6. Verify it's ignored (buffer unchanged)
7. Call save
8. Verify conflict_mode is false, base_content updated, dirty is false

**Location:** `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

---

### Step 10: Visual verification

Manually verify the visual indicator:
1. Open a file in lite-edit
2. Make an edit (buffer becomes dirty)
3. Use an external tool to modify the same line(s)
4. Observe: conflict markers should appear, tab should show conflict indicator
5. Make another external edit
6. Observe: no change (events ignored while in conflict mode)
7. Save (Cmd+S)
8. Observe: conflict mode clears, tab indicator returns to normal dirty state

---

**BACKREFERENCE COMMENTS**

Add the following backreference at the top of relevant sections:

```rust
// Chunk: docs/chunks/conflict_mode_lifecycle - Conflict mode lifecycle management
```

Place at:
- The `conflict_mode` field in Tab struct
- The conflict_mode setter in merge_file_tab
- The conflict_mode check in handle_file_changed
- The conflict_mode clearing in save_file
- The CONFLICT_INDICATOR_COLOR constant

## Dependencies

- **three_way_merge chunk (ACTIVE)**: Provides `MergeResult::Conflict` variant and `three_way_merge()` function
- **base_snapshot_reload chunk (ACTIVE)**: Provides `base_content` field on Tab
- **file_change_events chunk (ACTIVE)**: Provides `FileChanged` event routing and self-write suppression

## Risks and Open Questions

1. **Re-merge timing after save**: When the user saves while in conflict mode, we need to check if the disk changed again during conflict resolution. The sequencing is delicate:
   - Save writes buffer content to disk
   - base_content updates to saved content
   - We check if disk differs from what we just wrote
   - If it differs, another process wrote between our save and read
   - This is a race window, but acceptable for the use case

2. **Finding tabs by path across workspaces**: The helper to check conflict mode needs to search all workspaces. This mirrors the existing `reload_file_tab` and `merge_file_tab` pattern.

3. **Conflict indicator visibility**: The conflict indicator color must be distinct enough from dirty (yellow) and unread (blue) to be immediately recognizable. Using red/pink (Catppuccin's #f38ba8) provides good contrast.

4. **Dirty flag interaction**: The success criteria state "dirty flag remains true throughout the conflict lifecycle" - need to ensure we never clear dirty when setting conflict mode. The existing code already keeps dirty=true after merge, so this should be fine.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->