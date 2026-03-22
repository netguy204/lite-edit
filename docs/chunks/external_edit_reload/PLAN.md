


<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a semantic bug fix with a defense-in-depth strategy. The existing file
watcher infrastructure (chunks `file_change_events`, `base_snapshot_reload`,
`buffer_file_watching`) provides a complete event-to-reload pipeline, but
external edits to files in unfocused panes are not picked up. The fix has two
layers:

**Layer 1 — Diagnose and fix the watcher pipeline gap.** The drain loop's
`handle_file_changed` searches all workspaces and has no focus-based filtering,
so the event *should* work regardless of pane focus. The most likely root causes
to investigate:

- **FSEvents event kind mismatch**: External tools that use atomic writes
  (write-temp + rename) may produce `Create`/`Rename` events rather than
  `Modify(Data(Content))`. The `handle_fs_event` function in `file_index.rs`
  only registers `Modify(Data(_))` and `Modify(Any)` with the debouncer
  (line 1017–1023), so an atomic-write pattern would be silently dropped.
  The `buffer_file_watcher.rs` handler may have the same gap.
- **Path mismatch**: The `find_tab_by_path` comparison uses exact equality
  (`associated == path`). If `tab.associated_file` and the watcher event path
  differ (e.g., symlink resolution, trailing slashes), the lookup fails silently.
- **Debouncer edge case**: A timing issue where the debouncer's 100ms flush
  window interacts poorly with the watcher thread's 100ms poll interval, causing
  a race where events are registered but never flushed.

**Layer 2 — Mtime-based staleness checks as a safety net.** Even after fixing
the watcher pipeline, add mtime checks on pane focus change and workspace switch
so that missed events are caught when the user returns to a buffer. This follows
the same pattern as the existing `pause_file_watchers`/`resume_file_watchers`
mtime-comparison logic (chunk `app_nap_file_watcher_pause`).

The testing strategy follows docs/trunk/TESTING_PHILOSOPHY.md: the core logic
(mtime comparison, staleness detection, reload-vs-skip decisions) is pure state
manipulation testable without a window or GPU. The file watcher integration is
verified via integration tests using real temp files.

## Subsystem Considerations

No existing subsystems (renderer, spatial_layout, viewport_scroll) are directly
relevant. This chunk primarily touches the event channel and editor state layers.

## Sequence

### Step 1: Add diagnostic logging to the file change pipeline

Add temporary `eprintln!` traces at key points to confirm event flow during
manual testing. This will be removed before the final commit but is essential
for root-cause confirmation.

Locations:
- `crates/editor/src/file_index.rs` — in `handle_fs_event` for Modify events
  *and* for events that fall through to the `_ => {}` catch-all, log the
  `EventKind` and path so we can see what FSEvents actually delivers for
  atomic writes.
- `crates/editor/src/drain_loop.rs` — in `handle_file_changed`, log the path
  and whether reload/merge succeeded.
- `crates/editor/src/editor_state.rs` — in `reload_file_tab`, log when the
  tab lookup fails (no matching path found across workspaces).

### Step 2: Handle atomic-write file changes (Create-after-Remove pattern)

External tools (git, vim, Claude Code) often write files atomically: write to
a temp file, then rename over the target. On macOS FSEvents, this can produce
`Remove` + `Create` events or `Rename` events rather than `Modify` events.

In `crates/editor/src/file_index.rs`, inside `handle_fs_event`:
- For `EventKind::Create(_)`: after updating the cache, also register the path
  with the debouncer if the path matches an existing cache entry (i.e., the file
  was already known — this is a recreate, not a brand-new file). This ensures
  the on_change callback fires for atomic writes.
- For `EventKind::Remove(_)` followed by `EventKind::Create(_)`: the Create
  handler above covers this. No special pairing needed since the debouncer
  coalesces within its 100ms window.
- For `Rename` events that resolve to an existing watched path (the "to" path
  matches a cached file): register the "to" path with the debouncer.

In `crates/editor/src/buffer_file_watcher.rs`, apply the same pattern in the
watcher callback: treat `Create` and `Rename(to)` events for tracked files
as content changes.

Location: `crates/editor/src/file_index.rs` lines 924–1029,
`crates/editor/src/buffer_file_watcher.rs` watcher callback (~line 420–450).

### Step 3: Add per-tab mtime tracking

Add a `last_known_mtime: Option<SystemTime>` field to the `Tab` struct in
`crates/editor/src/workspace.rs`.

Populate it:
- In `associate_file()` — after loading file contents, store the file's mtime.
- In `reload_file_tab()` — after reading the file, update the mtime.
- In `merge_file_tab()` — after reading the file, update the mtime.
- In `save_file()` — after writing, update the mtime.

This field enables the staleness checks in Steps 4 and 5.

Location: `crates/editor/src/workspace.rs` (Tab struct definition),
`crates/editor/src/editor_state.rs` (associate_file, reload_file_tab,
merge_file_tab, save_file methods).

### Step 4: Add staleness check on pane focus change

When the user clicks into or navigates to a different pane, check whether that
pane's active tab has a stale file.

Add a method `EditorState::check_active_tab_staleness()`:
1. Get the active tab's `associated_file` and `last_known_mtime`.
2. If both are `Some`, stat the file and compare mtimes.
3. If the disk mtime is newer and the tab is clean → call `reload_file_tab`.
4. If the disk mtime is newer and the tab is dirty → call `merge_file_tab`.
5. If the file no longer exists, skip (deletion handling is separate).

Call this method from the pane focus change code path in
`crates/editor/src/editor_state.rs` — wherever `ws.active_pane_id` is updated
(mouse click on pane, keyboard pane navigation). The call should happen *after*
the pane ID is updated so the correct tab is checked.

Location: `crates/editor/src/editor_state.rs` (new method + call sites in
mouse handling and pane navigation).

### Step 5: Add staleness check on workspace switch

When switching workspaces, check ALL tabs in ALL panes of the target workspace
for staleness — not just the active tab.

Add a method `EditorState::check_workspace_staleness(ws_idx: usize)`:
1. Iterate all panes in the workspace at `ws_idx`.
2. For each pane, iterate all tabs.
3. For each tab with an `associated_file` and `last_known_mtime`, stat the file.
4. If disk mtime is newer and tab is clean → reload via the same logic as
   `reload_file_tab` but operating on the specific workspace/tab by index.
5. If disk mtime is newer and tab is dirty → merge.

Call this from `EditorState::switch_workspace()` after updating the active
workspace index.

Location: `crates/editor/src/editor_state.rs` (new method + call from
switch_workspace).

### Step 6: Write unit tests for staleness detection

Following the testing philosophy (TDD for behavioral logic), write tests first
for the mtime comparison logic:

1. **test_staleness_check_detects_modified_file**: Create a temp file, associate
   it with a tab, modify it on disk, call the staleness check, verify the buffer
   content was updated.
2. **test_staleness_check_skips_clean_unchanged_file**: Associate a file, don't
   modify it, verify no reload occurs (buffer unchanged).
3. **test_staleness_check_skips_dirty_buffer_without_merge**: Associate a file,
   make local edits (dirty buffer), modify file on disk, verify merge_file_tab
   is called (not reload).
4. **test_staleness_check_handles_missing_file**: Associate a file, delete it,
   verify no crash and no reload.
5. **test_staleness_check_handles_no_associated_file**: Tab with no file (e.g.,
   untitled buffer) — verify no-op.

Location: `crates/editor/src/editor_state.rs` `#[cfg(test)]` module.

### Step 7: Write integration tests for atomic-write detection

Test that the file watcher pipeline correctly handles atomic writes:

1. **test_create_event_triggers_reload_for_known_file**: Set up a FileIndex,
   create a file, register it, then simulate a remove+create (atomic write) by
   deleting and recreating the file. Verify the on_change callback fires.
2. **test_debouncer_coalesces_remove_create_pair**: Register a Remove then
   Create for the same path within the 100ms window. Verify exactly one callback
   invocation.

Location: `crates/editor/src/file_index.rs` `#[cfg(test)]` module.

### Step 8: Remove diagnostic logging

Remove the `eprintln!` traces added in Step 1. The mtime-based safety net
provides ongoing protection without runtime logging overhead.

### Step 9: Manual verification

Manually verify all success criteria:
1. Open a file in a non-focused pane. Edit it externally (e.g., `echo "new" > file`).
   Verify the buffer updates within a few seconds without interaction.
2. Open a file, switch to another pane. Edit externally. Click back to the
   original pane. Verify reload occurs immediately.
3. Open a file in workspace 1. Switch to workspace 2. Edit the file externally.
   Switch back to workspace 1. Verify the buffer is updated.
4. Open a file, make local unsaved edits, then modify externally. Verify the
   buffer enters merge/conflict mode rather than silently overwriting.
5. Verify focused-pane reload behavior is unaffected (existing behavior).

---

**BACKREFERENCE COMMENTS**

When implementing code, add backreference comments:
- `// Chunk: docs/chunks/external_edit_reload` on new methods and significant
  modifications.

## Risks and Open Questions

- **FSEvents event kinds are platform-specific and under-documented.** The
  actual event kinds delivered for atomic writes (write-temp + rename) on macOS
  need empirical confirmation via the Step 1 logging. The fix in Step 2 covers
  the most common patterns but may need adjustment based on what FSEvents
  actually reports.
- **mtime granularity.** On APFS, mtime has nanosecond resolution, so false
  negatives from equal-mtime races are unlikely. On HFS+, resolution is 1
  second, which could cause a miss if the external write happens within the same
  second as the last known mtime. This is acceptable since the watcher pipeline
  (Layer 1) is the primary detection mechanism and mtime is a safety net.
- **Performance of stat() on workspace switch.** Checking all tabs in a
  workspace on switch involves one `stat()` syscall per tab with an associated
  file. For typical workspace sizes (10-50 tabs), this is negligible (<1ms).
  If workspaces grow much larger, consider limiting to recently-active tabs.
- **Rename event variants.** The `notify` crate on macOS can produce rename
  events in three modes (Both, From, To, Any). Step 2 needs to handle all of
  these gracefully, particularly the non-Both modes where we only see one half
  of the rename.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
