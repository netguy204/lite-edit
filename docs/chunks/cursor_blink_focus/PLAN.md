<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The core insight is that cursor blinking is currently **global** — a single `cursor_visible` boolean in `EditorState` controls both the main buffer cursor and overlay cursors (selector query, find strip). When focus is on an overlay, the main buffer cursor should be static (either hidden or always-visible), while the overlay cursor should blink.

**Strategy: Focus-Aware Cursor Visibility**

We will:
1. Track **separate cursor visibility states** for the main buffer and overlay mini buffers
2. Make the blink timer **focus-aware** — it toggles visibility only for the currently focused target
3. Update rendering to use the **appropriate visibility flag** for each cursor

**Key design decisions:**
- The main buffer cursor should render as **static visible** (not hidden) when an overlay has focus — this provides visual feedback about where editing will resume
- Overlay cursors inherit the existing blink behavior (toggle every 500ms, solid after keystroke)
- When focus returns to the buffer, its cursor immediately starts blinking from the visible state

**Building on existing code:**
- `EditorFocus` enum already cleanly separates `Buffer`, `Selector`, and `FindInFile` modes
- `last_keystroke` timestamp tracking already exists for blink suppression
- `MiniBuffer` has its own internal state but doesn't currently track blink visibility
- Rendering already passes `cursor_visible` to the render functions, just needs separate values per focus target

**Testing approach (per TESTING_PHILOSOPHY.md):**
- Cursor blinking involves timer callbacks and visual output — the timer/rendering is platform code (humble view)
- We can test the **pure logic**: given a focus state and time-since-keystroke, which cursor visibility flags should be set?
- Unit test the `toggle_cursor_blink` logic changes to ensure correct focus-aware behavior

## Subsystem Considerations

No subsystems documented in `docs/subsystems/` — section not applicable.

## Sequence

### Step 1: Add overlay cursor visibility state to EditorState

Add a new field `overlay_cursor_visible: bool` to `EditorState` in `editor_state.rs`.

This field tracks whether the overlay cursor (selector or find strip) should be visible. It is:
- Toggled by the blink timer when focus is on an overlay
- Reset to `true` on overlay keystroke
- Independent from the main buffer's `cursor_visible`

Initialize it to `true` in the constructor (cursors start visible).

Location: `crates/editor/src/editor_state.rs`

### Step 2: Track last keystroke time for overlay input

Add `last_overlay_keystroke: Instant` field to `EditorState`.

Update `handle_key_selector()` and `handle_key_find()` methods to record `self.last_overlay_keystroke = Instant::now()` when processing overlay keystrokes (mirroring what `handle_key_buffer()` does for `last_keystroke`).

This enables the blink timer to keep the overlay cursor solid while the user is actively typing in the overlay.

Location: `crates/editor/src/editor_state.rs`

### Step 3: Make toggle_cursor_blink focus-aware

Refactor the `toggle_cursor_blink()` method to be focus-aware:

1. Check `self.focus` to determine which cursor is active
2. If `EditorFocus::Buffer`: toggle `self.cursor_visible` using `last_keystroke` (existing behavior)
3. If `EditorFocus::Selector` or `EditorFocus::FindInFile`: toggle `self.overlay_cursor_visible` using `last_overlay_keystroke`
4. Return the appropriate dirty region (cursor line for buffer, full viewport or specific overlay region for overlays)

The key change: when focus is NOT on the buffer, the buffer's `cursor_visible` is not toggled — it remains in whatever state it was (we'll set it to `true` in the next step).

Location: `crates/editor/src/editor_state.rs`

### Step 4: Reset cursor visibility on focus transitions

When focus changes, reset cursor states appropriately:

1. **Entering an overlay** (buffer → selector/find):
   - Set `cursor_visible = true` (main buffer cursor stays solid/visible)
   - Set `overlay_cursor_visible = true` (overlay cursor starts visible)
   - Record `last_overlay_keystroke = Instant::now()` (prevent immediate blink-off)

2. **Leaving an overlay** (selector/find → buffer):
   - Set `cursor_visible = true` (resume visible, blinking will start on next timer tick)
   - Record `last_keystroke = Instant::now()` (keep solid briefly before blinking)

Modify the existing focus-change paths:
- `handle_cmd_p()` (opens selector)
- `handle_cmd_f()` (opens find)
- `close_selector()` / `confirm_selector()` (closes selector)
- `handle_key_find()` with Escape (closes find)

Location: `crates/editor/src/editor_state.rs`

### Step 5: Update rendering to use separate visibility flags

Update the `render_if_dirty()` method in `EditorController` (main.rs) to pass the correct visibility flag for each render path:

1. When `focus == Selector`: pass `overlay_cursor_visible` to `render_with_editor()` for the selector cursor
2. When `focus == FindInFile`: pass `overlay_cursor_visible` to `render_with_find_strip()` for the find cursor
3. When `focus == Buffer`: pass `cursor_visible` to `render_with_editor()` (unchanged)

Note: The existing code already passes `self.state.cursor_visible` to the renderer. We just need to substitute `self.state.overlay_cursor_visible` for the overlay cases.

Location: `crates/editor/src/main.rs`

### Step 6: Add unit tests for focus-aware blink logic

Add tests that verify:

1. **Buffer focus blink**: When focus is Buffer, `toggle_cursor_blink()` toggles `cursor_visible` (existing behavior)
2. **Overlay focus blink**: When focus is Selector/FindInFile, `toggle_cursor_blink()` toggles `overlay_cursor_visible`, not `cursor_visible`
3. **Keystroke suppression**: Recent keystroke prevents cursor from toggling off (for both buffer and overlay)
4. **Focus transition reset**: Opening an overlay sets `cursor_visible = true` and `overlay_cursor_visible = true`

The tests exercise pure state logic without platform dependencies (per TESTING_PHILOSOPHY.md).

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 7: Manual verification

Verify visually:
- Open file picker (Cmd+P): main buffer cursor is visible but not blinking; picker cursor blinks
- Type in picker: picker cursor stays solid during typing, resumes blinking after pause
- Close picker (Escape or select): main buffer cursor resumes blinking
- Open find strip (Cmd+F): same behavior — main buffer cursor static, find cursor blinks
- Close find strip (Escape): main buffer cursor resumes blinking
- No visual glitches during focus transitions (no double-blink, no missing cursor frame)

## Dependencies

None. This chunk builds on existing infrastructure:
- `EditorFocus` enum (docs/chunks/file_picker)
- `MiniBuffer` (docs/chunks/mini_buffer_model)
- Cursor blink timer (established in docs/chunks/editable_buffer)
- Find strip (docs/chunks/find_in_file)

## Risks and Open Questions

1. **Dirty region calculation for overlays**: Currently `cursor_dirty_region()` returns the line containing the buffer cursor. For overlay cursors, we may need to return `FullViewport` or calculate the overlay's specific dirty region. Starting with `FullViewport` for overlay blink is safe but slightly less efficient.

2. **Edge case: rapid focus switching**: If the user rapidly toggles between buffer and overlay (e.g., Cmd+P then Escape repeatedly), the cursor visibility states should remain consistent. The reset-on-transition logic should handle this, but worth verifying.

3. **Renderer sync**: The renderer's `set_cursor_visible()` is called once per frame. We need to ensure it receives the correct visibility flag for the main buffer cursor even when focus is on an overlay (since the buffer is still rendered in the background).

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