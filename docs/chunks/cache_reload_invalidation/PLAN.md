<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a small semantic bug fix that uses the existing cache invalidation
infrastructure established by the `styled_line_cache` chunk. The fix requires
adding a single assignment (`self.clear_styled_line_cache = true`) in two
locations where buffer content is wholesale-replaced:

1. **`reload_file_tab()`** — Called when a `FileChanged` event arrives for a
   clean tab. After the `TextBuffer::from_str()` call replaces the buffer.

2. **`associate_file()`** — Called when the file picker confirms a selection
   or Cmd+O opens a file. After the buffer is replaced with loaded content.

Both methods already call `self.invalidation.merge(InvalidationKind::Layout)`,
which triggers a full viewport render. The missing piece is that the styled
line cache is not cleared, so the renderer serves stale cached lines instead
of re-computing styled content from the new buffer.

**Testing strategy**: Per TESTING_PHILOSOPHY.md, this is a testable behavior
at the state layer. The `clear_styled_line_cache` flag is observable via
`take_clear_styled_line_cache()`. We can write unit tests that:
- Call `associate_file()` with a real file
- Assert that `take_clear_styled_line_cache()` returns `true`
- Do the same for `reload_file_tab()`

This verifies the semantic guarantee without requiring a Metal renderer.

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk USES the renderer
  subsystem's styled line cache invalidation mechanism. The fix sets the
  `clear_styled_line_cache` flag that the renderer consumes via
  `take_clear_styled_line_cache()` during the render pass.

The renderer subsystem (status: DOCUMENTED) defines the styled line cache as
part of its scope. This chunk does not modify the cache mechanism itself—it
only ensures the existing invalidation flag is set in two previously-missed
code paths.

## Sequence

### Step 1: Write failing tests for cache invalidation on associate_file

Add a test in `crates/editor/src/editor_state.rs` that:
1. Creates an `EditorState` with a file tab
2. Creates a temporary file with content
3. Calls `associate_file()` with that file path
4. Asserts that `take_clear_styled_line_cache()` returns `true`

This test will fail initially because `associate_file()` does not set the flag.

**Location**: `crates/editor/src/editor_state.rs` (in the `#[cfg(test)]` module,
near the existing `test_associate_file_*` tests around line 6317)

### Step 2: Write failing tests for cache invalidation on reload_file_tab

Add a test that:
1. Creates an `EditorState` with a file tab associated to a temporary file
2. Modifies the file on disk
3. Calls `reload_file_tab()` with that path
4. Asserts that `take_clear_styled_line_cache()` returns `true`

This test will also fail initially.

**Location**: `crates/editor/src/editor_state.rs` (in the `#[cfg(test)]` module,
near the existing `reload_file_tab` tests)

### Step 3: Fix associate_file to set clear_styled_line_cache

In `associate_file()` (line ~4053), add:
```rust
self.clear_styled_line_cache = true;
```

This should be placed after the buffer replacement inside the `Ok(bytes)` branch
(around line 4065-4076), and also unconditionally at the end of the method since
even loading a non-existent file changes the tab's identity.

**Location**: `crates/editor/src/editor_state.rs` function `associate_file()`

Add a chunk backreference comment:
```rust
// Chunk: docs/chunks/cache_reload_invalidation - Clear cache on buffer replace
```

### Step 4: Fix reload_file_tab to set clear_styled_line_cache

In `reload_file_tab()` (line ~4382), add:
```rust
self.clear_styled_line_cache = true;
```

This should be placed after the `*buffer = TextBuffer::from_str(&new_content);`
line (around line 4429), alongside the existing `self.invalidation.merge(...)`.

**Location**: `crates/editor/src/editor_state.rs` function `reload_file_tab()`

Add a chunk backreference comment:
```rust
// Chunk: docs/chunks/cache_reload_invalidation - Clear cache on buffer replace
```

### Step 5: Verify tests pass

Run the tests:
```bash
cargo test -p lite-edit-editor --lib -- associate_file
cargo test -p lite-edit-editor --lib -- reload_file_tab
```

Both tests written in Steps 1-2 should now pass.

### Step 6: Run full test suite

Ensure no regressions:
```bash
cargo test --workspace
```

---

**BACKREFERENCE COMMENTS**

This chunk adds backreferences at the method level in `reload_file_tab()` and
`associate_file()` to explain why `clear_styled_line_cache = true` is set.

## Dependencies

- **styled_line_cache** (ACTIVE): This chunk depends on the `clear_styled_line_cache`
  flag and `take_clear_styled_line_cache()` method introduced by that chunk.
- **base_snapshot_reload** (ACTIVE): The `reload_file_tab()` method was introduced
  by this chunk.

Both dependencies are already merged (ACTIVE status).

## Risks and Open Questions

**Low risk**: This is a minimal fix that adds a single flag assignment in two
well-understood code paths. The flag mechanism is already proven by tab-switch
cache clearing.

**Potential edge case**: If `associate_file()` is called but the file read fails
(the `Err(_)` branch at line 4078), should the cache still be cleared? Currently
the buffer is left as-is, so arguably the cache is still valid. However, the
method still changes `associated_file` and calls `setup_active_tab_highlighting()`,
which could change the highlighter. For safety, we clear the cache unconditionally
in `associate_file()` after the file read attempt—the cost is one extra full
cache invalidation, which is negligible compared to the cost of stale rendering.

**No performance concern**: Cache clearing only occurs on explicit buffer
replacement operations (file reload, file open), not on every frame or keystroke.

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