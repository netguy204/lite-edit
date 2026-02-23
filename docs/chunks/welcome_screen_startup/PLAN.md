<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a straightforward change that leverages existing welcome_screen logic:

1. **Identify the initialization code** – The `setup_window()` function in `main.rs` creates the initial `EditorState` with a buffer populated by `generate_demo_content()`.

2. **Replace demo buffer with empty buffer** – Create an empty `TextBuffer::new()` instead of `TextBuffer::from_str(&demo_content())`.

3. **Remove unused demo content function** – Delete `generate_demo_content()` since it's no longer used anywhere.

The welcome_screen chunk already implements `Editor::should_show_welcome_screen()` which checks if the active file tab has an empty buffer and returns true. The renderer uses this to render the welcome screen when appropriate. By creating an empty buffer on startup, the existing welcome_screen logic will automatically display it.

No new tests are needed—this is a simple data flow change that activates existing, tested behavior.

## Subsystem Considerations

This chunk does not touch any subsystems. It's a simple change to initialization logic that activates existing welcome_screen functionality.

## Sequence

### Step 1: Replace demo buffer with empty buffer

In `setup_window()` function in `main.rs`, find the code that creates the initial buffer:

```rust
// Create a TextBuffer with demo content
let demo_content = generate_demo_content();
let buffer = TextBuffer::from_str(&demo_content);
```

Replace it with:

```rust
// Chunk: docs/chunks/welcome_screen_startup - Initialize with empty buffer
// Create an empty buffer to show welcome screen on startup
let buffer = TextBuffer::new();
```

Location: `crates/editor/src/main.rs` around line 720-730

### Step 2: Remove unused generate_demo_content function

Delete the entire `generate_demo_content()` function and its associated comment block from `main.rs`. This function is defined around line 650 and is no longer needed since we're initializing with an empty buffer.

Location: `crates/editor/src/main.rs`

### Step 3: Verify behavior manually

Build and run the editor:
- The welcome screen should be displayed on startup (feather logo, hotkeys, etc.)
- Typing should dismiss the welcome screen and show a normal empty buffer
- Creating a new tab (Cmd+T) should still show the welcome screen (existing behavior preserved)

## Dependencies

- **welcome_screen** chunk must be complete – This chunk leverages the welcome screen rendering logic. The chunk is already ACTIVE and the logic is in place.

No new external libraries or infrastructure needed.

## Risks and Open Questions

**Risks:**
- None identified. This is a straightforward code path change with well-defined behavior.

**Open Questions:**
- None. The implementation is clear-cut and leverages tested functionality.

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