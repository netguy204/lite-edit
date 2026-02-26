<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

When the window loses key status (`windowDidResignKey:`), we need to pause all
file watching to eliminate wakeups that prevent App Nap. When the window becomes
key again (`windowDidBecomeKey:`), we resume watching and trigger a re-scan to
detect any changes that occurred while paused.

There are two file watching systems in lite-edit:

1. **FileIndex** (`crates/editor/src/file_index.rs`): Watches the workspace root
   recursively for the file picker and file change detection.
2. **BufferFileWatcher** (`crates/editor/src/buffer_file_watcher.rs`): Watches
   individual files opened from outside the workspace.

Both need pause/resume functionality. The approach is:

1. **Add pause/resume to BufferFileWatcher**: Stop all watcher threads and drop
   watchers on pause. On resume, re-register all tracked files and scan for
   changes by comparing timestamps.

2. **Add pause/resume to FileIndex**: Similar to BufferFileWatcher, but for the
   workspace watcher. Since FileIndex tracks state in an `Arc<Mutex<SharedState>>`,
   we can stop the watcher thread and restart it on resume.

3. **Wire into window key events**: Extend `windowDidResignKey:` and
   `windowDidBecomeKey:` in `main.rs` to call pause/resume on the watchers
   through the event channel (similar to how the blink timer is handled).

4. **Resume scan**: On resume, instead of relying on FSEvents to deliver
   coalesced events (which is OS-dependent), we stat all watched files and
   emit `FileChanged` events for any that changed while paused.

**Why pause instead of just ignoring events?**

The `notify` crate's watcher threads still wake the process to deliver events
even if we ignore them. Stopping the watchers entirely is required to let the
process enter App Nap.

**Testing strategy** (per TESTING_PHILOSOPHY.md):
- Watcher lifecycle is platform code (humble view), so we verify high-level
  behavior manually via Activity Monitor's "App Nap" column.
- Add unit tests for timestamp-based change detection logic.
- Existing file watcher tests remain unchanged.

## Subsystem Considerations

No subsystems are directly relevant to this chunk.

## Sequence

### Step 1: Add pause/resume to BufferFileWatcher

Add methods to `BufferFileWatcher`:

```rust
/// Pauses all watchers, storing modification times for later comparison.
/// Returns the current state for re-registration on resume.
pub fn pause(&mut self) -> PausedWatcherState

/// Resumes watching. Re-registers all previously tracked files and
/// emits FileChanged events for any files modified while paused.
pub fn resume(&mut self, state: PausedWatcherState)
```

The `PausedWatcherState` struct captures:
- Map of file paths to their last-known modification times (from stat)
- The on_change callback (needs to be preserved across pause)

On pause:
1. For each registered file, stat it to get the current mtime
2. Drop all watchers (this stops the threads)
3. Clear internal state but keep file_to_watch mapping

On resume:
1. Re-register all files from the saved mapping
2. For each file, stat again and compare mtimes
3. If mtime changed, emit FileChanged through the callback

**Location**: `crates/editor/src/buffer_file_watcher.rs`

### Step 2: Add pause/resume to FileIndex

The FileIndex is more complex because it's already started with threads. We need
a different approach: add a "paused" flag that the watcher thread checks.

Actually, simpler approach: FileIndex is per-workspace and already has reference
counting. We can add pause/resume by:

1. Add an `Arc<AtomicBool>` paused flag
2. The watcher thread checks this flag and sleeps when paused
3. On resume, trigger a full re-scan of watched files

Or even simpler: since FileIndex watchers can be resource-heavy, we could:
1. On pause: store the root path and drop the FileIndex
2. On resume: recreate the FileIndex (it will re-walk and re-watch)

This is heavyweight but simple and safe. The re-walk on resume ensures we
detect any changes. For a background operation, this is acceptable.

However, this approach loses the cache and recency list. Let's use a middle
ground:

Add a paused mode where:
1. The watcher is stopped (events no longer processed)
2. On resume, the watcher is restarted and we do a single-pass scan of
   recently-accessed files to emit change events

For now, we'll implement a simpler version: add pause/resume that:
- Pauses by stopping the watcher
- Resumes by recreating the watcher (FSEvents will coalesce any missed events)

**Location**: `crates/editor/src/file_index.rs`

### Step 3: Add pause/resume to EditorState

Add methods to EditorState that pause/resume both watchers:

```rust
/// Pauses file watching for App Nap eligibility.
pub fn pause_file_watchers(&mut self)

/// Resumes file watching after returning from background.
pub fn resume_file_watchers(&mut self)
```

These methods:
1. Pause/resume the BufferFileWatcher
2. Pause/resume each workspace's FileIndex

**Location**: `crates/editor/src/editor_state.rs`

### Step 4: Wire into window key events

Extend `windowDidResignKey:` and `windowDidBecomeKey:` in `main.rs`:

```rust
// In window_did_resign_key, after stopping blink timer:
// Send a PauseFileWatchers event through the channel

// In window_did_become_key, before starting blink timer:
// Send a ResumeFileWatchers event through the channel
```

Add two new event types to `EditorEvent`:
- `PauseFileWatchers`
- `ResumeFileWatchers`

Handle these in the drain loop by calling the EditorState methods.

**Location**: `crates/editor/src/main.rs`, `crates/editor/src/editor_event.rs`,
`crates/editor/src/drain_loop.rs`

### Step 5: Run tests and verify

Run existing tests:
```bash
cargo test --package editor
```

Verify no regressions in file watching tests.

### Step 6: Manual App Nap verification

1. Build and run lite-edit: `cargo run --release -p editor`
2. Open a file from outside the workspace (Cmd+O, navigate to /tmp or similar)
3. Open Activity Monitor, enable the "App Nap" column
4. Focus another application (lite-edit loses key window status)
5. Wait ~30 seconds for App Nap to engage
6. Verify "App Nap" shows "Yes" for lite-edit
7. Modify the external file with another editor
8. Click lite-edit to regain focus
9. Verify the file shows as modified (or reloads)
10. Verify normal file watching continues to work

## Dependencies

- **app_nap_blink_timer** (ACTIVE): This chunk adds file watcher pause/resume
  to the existing window key event handlers created by app_nap_blink_timer.
- **buffer_file_watching** (ACTIVE): This chunk extends BufferFileWatcher with
  pause/resume capability.

## Risks and Open Questions

- **FSEvents coalescing reliability**: macOS FSEvents is supposed to coalesce
  events and deliver them on re-subscribe, but this is not guaranteed. The
  timestamp-based scan on resume is a fallback to ensure we detect changes.

- **FileIndex recreation overhead**: Recreating the FileIndex on resume means
  re-walking the directory tree. For large workspaces this could cause a brief
  delay. The walk happens on a background thread so UI won't freeze, but there
  may be a window where the file picker shows stale results. This is acceptable
  for a backgrounded app returning to foreground.

- **Simultaneous pause/resume**: If the user rapidly switches between apps,
  pause and resume events could interleave. The implementation should be
  idempotent (pausing twice is safe, resuming twice is safe).

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