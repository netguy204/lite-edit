<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk implements session persistence by serializing workspace state to JSON on clean exit and restoring it on startup. The approach follows the existing architecture patterns:

1. **Serialization model**: Define separate `serde`-serializable structs that mirror the relevant subset of the workspace model (`Editor` → `Workspace` → `Pane` → `Tab`). Only file-backed tab state is captured; terminals are skipped as per the goal.

2. **Storage location**: Use the macOS application support directory (`~/Library/Application Support/lite-edit/session.json`) following platform conventions. Fall back gracefully if the directory doesn't exist or can't be created.

3. **Save trigger**: Hook into the `applicationWillTerminate:` delegate method in `main.rs` to save session state before exit.

4. **Restore trigger**: Modify the startup flow in `main.rs` to check for a session file before showing the directory picker. If a valid session exists, restore workspaces from it instead of prompting for a directory.

5. **Testing strategy**: Following TESTING_PHILOSOPHY.md, the core logic (serialization, deserialization, path validation) is pure Rust and fully testable without platform dependencies. The save/restore integration points are thin wrappers around this testable core.

The implementation adds the `serde` and `serde_json` dependencies, which are standard Rust crates with no runtime overhead beyond what's needed for JSON parsing.

## Subsystem Considerations

No existing subsystems are directly affected by this chunk. The renderer subsystem (DOCUMENTED) is not touched—session persistence operates entirely on the data model layer. The workspace model (`workspace.rs`) is extended with serialization support but no rendering logic changes.

## Sequence

### Step 1: Add serde dependencies

Add `serde` (with `derive` feature) and `serde_json` to `crates/editor/Cargo.toml`.

Location: `crates/editor/Cargo.toml`

### Step 2: Create the session module with serializable types

Create a new `session.rs` module with:
- `SessionData` struct (root): `schema_version`, `active_workspace`, `workspaces`
- `WorkspaceData` struct: `root_path`, `label`, `active_pane_id`, `pane_root` (pane tree)
- `PaneLayoutData` enum: mirrors `PaneLayoutNode` (Leaf/Split variants)
- `PaneData` struct: `id`, `tabs`, `active_tab`
- `TabData` struct: `associated_file` (absolute PathBuf)

Each struct derives `Serialize`, `Deserialize`, `Debug`, `Clone`.

The session schema version starts at 1. Future changes that break backward compatibility will bump this version.

Location: `crates/editor/src/session.rs`

### Step 3: Implement SessionData::from_editor

Add a method `SessionData::from_editor(&Editor) -> Self` that extracts the serializable state from the live editor model:
- Iterates through all workspaces
- For each workspace, captures root_path, label, active_pane_id
- Traverses the pane tree, converting each pane to PaneData
- For each pane, filters to file tabs only and extracts their `associated_file` paths
- Skips tabs where `associated_file` is `None` (new unsaved files)
- Records which workspace was active

Location: `crates/editor/src/session.rs`

### Step 4: Implement session file path resolution

Add a function `session_file_path() -> Option<PathBuf>` that returns:
- macOS: `~/Library/Application Support/lite-edit/session.json`

Use `dirs` crate (or raw `NSSearchPathForDirectoriesInDomains` via objc2) to get the application support directory. Create the `lite-edit` subdirectory if it doesn't exist.

Location: `crates/editor/src/session.rs`

### Step 5: Implement save_session

Add a function `save_session(editor: &Editor) -> Result<(), std::io::Error>` that:
1. Calls `SessionData::from_editor(editor)` to extract state
2. Calls `session_file_path()` to get the target path
3. Serializes to JSON with `serde_json::to_string_pretty`
4. Writes atomically: write to a temp file, then rename (atomic on APFS/HFS+)

Location: `crates/editor/src/session.rs`

### Step 6: Implement load_session

Add a function `load_session() -> Option<SessionData>` that:
1. Calls `session_file_path()` to find the session file
2. Returns `None` if the file doesn't exist
3. Reads and deserializes the JSON
4. Validates schema_version matches (return `None` if mismatch)
5. Returns `Some(SessionData)` on success, `None` on any error (graceful degradation)

Location: `crates/editor/src/session.rs`

### Step 7: Implement SessionData::restore_into_editor

Add a method `SessionData::restore_into_editor(&self, line_height: f32) -> Result<Editor, RestoreError>` that:
1. Creates an `Editor` with `new_deferred`
2. For each WorkspaceData:
   - Checks if `root_path` exists (skip workspace if not)
   - Creates a `Workspace` with the root path
   - Reconstructs the pane tree from `PaneLayoutData`
   - For each PaneData, creates tabs by loading files from disk
   - Skips individual files that no longer exist (logs warning internally)
3. Sets the active workspace index
4. Returns the populated Editor

If all workspaces are skipped (none exist), returns an error so startup can fall back to the directory picker.

Location: `crates/editor/src/session.rs`

### Step 8: Hook save_session into applicationWillTerminate

Modify `AppDelegate` in `main.rs` to implement `applicationWillTerminate:`:
1. Access the `DRAIN_LOOP` global to get the `EditorState`
2. Call `save_session(&state.editor)`
3. Log any errors but don't block termination

This requires adding a new unsafe impl block for the termination delegate method.

Location: `crates/editor/src/main.rs`

### Step 9: Modify startup to check for existing session

Modify `resolve_startup_directory` (or add a new pre-startup step) to:
1. Call `load_session()` before showing any UI
2. If a valid session exists, use `SessionData::restore_into_editor`
3. If restoration succeeds, skip the directory picker and use the restored Editor
4. If restoration fails or no session exists, fall through to existing behavior

This integrates with the existing `setup_window` flow, replacing the `resolve_startup_directory` + `add_startup_workspace` sequence with either restored state or the picker.

Location: `crates/editor/src/main.rs`

### Step 10: Write unit tests for serialization round-trip

Add tests in `session.rs`:
- Test `SessionData::from_editor` produces expected structure
- Test JSON serialization/deserialization round-trip
- Test `restore_into_editor` with valid data
- Test handling of missing workspace directories (skipped)
- Test handling of missing tab files (skipped)
- Test schema_version mismatch returns None

Location: `crates/editor/src/session.rs` (tests module)

### Step 11: Write integration test for file persistence

Add a test that:
1. Creates an Editor with multiple workspaces and tabs
2. Calls `save_session` to write to a temp directory
3. Calls `load_session` to read it back
4. Verifies the restored structure matches the original

Uses `tempfile` crate (already a dev-dependency) for test isolation.

Location: `crates/editor/tests/session_persistence.rs` (new file)

## Dependencies

- **serde** (derive feature) - Standard Rust serialization framework
- **serde_json** - JSON serialization backend
- **dirs** crate (optional) - For cross-platform config directory resolution, or use macOS APIs directly via objc2

## Risks and Open Questions

1. **Global DRAIN_LOOP access for termination**: The current architecture uses a leaked `Box<EventDrainLoop>` pointer stored in a global static. Accessing this safely from `applicationWillTerminate:` requires care. The existing pattern works because it's single-threaded, but we need to ensure the drain loop hasn't been deallocated (it never is currently).

2. **Large session files**: Workspaces with many tabs could produce large JSON files. This is unlikely to be a problem in practice (even 1000 tabs would be ~100KB of JSON), but if it becomes an issue, we could switch to a binary format or compress.

3. **File system race conditions**: Between session save and restore, files could be deleted or moved. The implementation handles this gracefully by skipping missing files, but users may be surprised if their tabs don't fully restore.

4. **Dirty files at exit**: The goal specifies "clean exit" but doesn't define what happens if there are unsaved changes. Currently, the quit command shows a confirm dialog for dirty tabs. Session persistence should only save file paths, not unsaved content—restoring a previously-dirty file will show its saved (not dirty) content.

5. **Performance impact on quit**: JSON serialization should be fast (<1ms for typical sessions), but disk I/O could add latency. Using atomic write (write to temp, rename) avoids corruption but adds a syscall. This should be measured.

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