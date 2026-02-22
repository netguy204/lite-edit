# Implementation Plan

## Approach

This chunk wires terminal tabs into the existing `Viewport` scroll infrastructure, enabling users to scroll through terminal scrollback history with trackpad/mouse wheel. The implementation follows the existing architecture where each `Tab` owns a `Viewport` and the renderer already uses `viewport.visible_range()` + `BufferView::styled_line()` generically.

**Architecture:**

The key insight is that the terminal already implements `BufferView`, and its `line_count()` returns the total of cold + hot scrollback + viewport lines. The `Viewport` scroll machinery doesn't care whether the underlying buffer is a `TextBuffer` or a `TerminalBuffer` — it just needs a line count for clamping.

The behavioral distinction between primary screen and alternate screen modes is critical:

1. **Primary screen** (shell, build output): Scroll events adjust the tab's `Viewport` scroll offset. The total scrollable content is `TerminalBuffer::line_count()`. New output auto-follows when at the bottom; otherwise, the user's position is preserved. Keypresses snap to bottom.

2. **Alternate screen** (vim, htop, less): Scroll events are encoded as mouse wheel escape sequences and sent to the PTY. The `Viewport` stays at offset 0 since the application owns scrolling.

**Strategy:**

1. **Add scroll encoding to `InputEncoder`**: Add `encode_scroll()` method that encodes scroll wheel events as mouse button 64/65 (scroll up/down) sequences.

2. **Implement `TerminalFocusTarget::handle_scroll()`**: Replace the no-op stub with logic that:
   - In alternate screen mode: Encode scroll as mouse wheel sequence via `InputEncoder` and write to PTY
   - In primary screen mode: Adjust the tab's `Viewport` scroll offset against `line_count()`

3. **Wire scroll through `EditorState`**: Update `EditorState::handle_scroll()` to properly route scroll events to terminal tabs via `TerminalFocusTarget`.

4. **Implement auto-follow behavior**: When in primary screen at the bottom, new output should advance the viewport. When scrolled up, new output should not change the scroll position.

5. **Implement snap-to-bottom on keypress**: Any keypress that sends data to the PTY should first snap the viewport to the bottom.

6. **Handle mode transitions**: When transitioning from alternate to primary screen, snap the viewport to bottom.

**Testing approach per TESTING_PHILOSOPHY.md:**

Since `TerminalBuffer` implements `BufferView` and `Viewport` is already well-tested, we can test the scroll behavior by:
- Unit tests for `InputEncoder::encode_scroll()` encoding
- Unit tests for `TerminalFocusTarget::handle_scroll()` behavior in both modes
- Integration tests for auto-follow and snap-to-bottom behavior

Visual verification will confirm the behavior with actual terminal applications.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport_scroll subsystem. The `Viewport` type and `RowScroller` primitives provide all the scroll arithmetic we need. The subsystem's scope explicitly lists "Terminal scrollback" as out of scope because `TerminalBuffer` has its own scrollback management — but that's fine because we're using `Viewport` as the view layer over the terminal's scrollback, not replacing how scrollback is stored.

## Sequence

### Step 1: Add scroll wheel encoding to InputEncoder

Add `encode_scroll(delta: ScrollDelta, col: usize, row: usize, modes: TermMode) -> Vec<u8>` to `InputEncoder` that encodes scroll events as mouse button 64 (up) or 65 (down).

The encoding follows the same pattern as `encode_mouse()`:
- Button 64 = scroll up
- Button 65 = scroll down
- Use SGR encoding if available, otherwise X10/normal encoding

**Files:** `crates/terminal/src/input_encoder.rs`

**Tests:** Unit tests verifying scroll encoding produces correct escape sequences for both SGR and legacy modes.

### Step 2: Implement TerminalFocusTarget::handle_scroll for alternate screen

Implement the alternate screen path in `handle_scroll()`:
- Check if `terminal.borrow().is_alt_screen()` is true
- If so, encode scroll as mouse wheel sequence using `InputEncoder::encode_scroll()`
- Write to PTY via `terminal.borrow_mut().write_input()`

This allows applications like vim, htop, and less to receive scroll events.

**Files:** `crates/terminal/src/terminal_target.rs`

**Tests:** Unit test verifying scroll events are encoded and written when in alternate screen mode.

### Step 3: Add viewport scroll support to TerminalFocusTarget

Modify `TerminalFocusTarget` to hold a reference to the tab's `Viewport` (or receive it as a parameter). This is needed for primary screen scrolling.

**Design decision:** Rather than storing the viewport in `TerminalFocusTarget`, we'll pass viewport parameters through the scroll handler. This matches how `EditorState::handle_scroll()` already creates an `EditorContext` for file tabs.

**Files:** `crates/terminal/src/terminal_target.rs`

### Step 4: Implement primary screen scrolling in handle_scroll

For primary screen mode (not alternate screen):
1. Get the terminal's `line_count()` for scroll bounds
2. Convert pixel delta to scroll offset change
3. Call `viewport.set_scroll_offset_px(new_offset, line_count)`
4. Mark viewport dirty

The viewport clamping ensures we can't scroll past the oldest line or past the bottom.

**Files:** `crates/terminal/src/terminal_target.rs`, `crates/editor/src/editor_state.rs`

**Tests:** Unit tests for scroll delta conversion and viewport offset changes.

### Step 5: Wire terminal scroll through EditorState

Update `EditorState::handle_scroll()` to properly handle terminal tabs:
1. Get the terminal buffer and viewport from the active tab
2. Create a `TerminalFocusTarget` (or reuse one) with access to both
3. Call `handle_scroll()` on the target with the delta and line count
4. Mark the appropriate dirty region

The current implementation marks `FullViewport` dirty, which is correct since terminal content may need full re-render after scrolling.

**Files:** `crates/editor/src/editor_state.rs`

**Tests:** Integration test verifying scroll events are properly routed to terminal tabs.

### Step 6: Implement auto-follow behavior

Add logic to `TerminalBuffer` polling to track whether the viewport is "at bottom":
- Track `was_at_bottom` flag before processing PTY events
- After new output arrives, if `was_at_bottom`, advance the viewport to keep latest output visible
- If not at bottom, preserve the current scroll position

"At bottom" means the viewport's scroll offset positions the last line of content at or below the bottom of the visible area.

**Files:** `crates/editor/src/editor_state.rs`

**Tests:**
- Test that new output advances viewport when at bottom
- Test that new output does NOT change viewport when scrolled up into history

### Step 7: Implement snap-to-bottom on keypress

Modify `EditorState::handle_key()` terminal path to:
1. Check if the terminal tab's viewport is scrolled up from bottom
2. If so, snap to bottom before sending the keypress to the PTY
3. Mark viewport dirty

This ensures the user always sees their input and the terminal's response.

**Files:** `crates/editor/src/editor_state.rs`

**Tests:** Test that keypress snaps viewport to bottom when scrolled up.

### Step 8: Handle alternate-to-primary mode transition

When the terminal transitions from alternate screen back to primary screen, snap the viewport to bottom. This is detected by:
- Tracking `was_alt_screen` before processing PTY events
- If `is_alt_screen()` changed from true to false, set viewport to bottom

**Files:** `crates/editor/src/editor_state.rs` or `crates/terminal/src/terminal_buffer.rs`

**Tests:** Test that exiting vim/htop snaps viewport to bottom of primary scrollback.

### Step 9: Integration testing and visual verification

Create integration tests that verify the complete flow:
1. Terminal with scrollback → scroll up → can see history
2. Scroll up → new output arrives → viewport position unchanged
3. At bottom → new output arrives → viewport follows
4. Scrolled up → keypress → snaps to bottom
5. Alternate screen active → scroll event → sent to PTY
6. Exit alternate screen → viewport snaps to bottom

Visual verification:
1. Run shell, generate output, scroll through history with trackpad
2. Run vim, scroll using trackpad → vim receives scroll events
3. Exit vim → back to shell at bottom of output

**Files:** `crates/terminal/tests/scroll_integration.rs`

## Dependencies

**Chunks that must be complete:**
- `terminal_input_encoding` (ACTIVE) — Provides `InputEncoder` with mouse encoding
- `terminal_file_backed_scrollback` (ACTIVE) — Provides `TerminalBuffer::line_count()` including cold + hot + screen
- `viewport_scrolling` (ACTIVE) — Provides scroll event handling infrastructure
- `terminal_tab_spawn` (ACTIVE) — Provides terminal tab creation and PTY polling

**External dependencies:** None — all required infrastructure exists.

## Risks and Open Questions

1. **Scroll direction convention**: Need to verify that positive scroll delta in the terminal context maps correctly to "scroll down into newer content" vs "scroll up into history". The existing `BufferFocusTarget` convention is positive dy = content moves up = see older content.

2. **Scroll granularity for alternate screen**: Terminal mouse wheel encoding sends discrete events (button 64 or 65) per scroll "click". Need to determine how to convert continuous pixel delta to discrete scroll events — probably use a threshold (e.g., one event per 3 pixels).

3. **Auto-follow threshold**: The GOAL says "within one screen of the latest line". Need to define precisely what "at bottom" means — possibly `scroll_offset_px >= max_offset_px - line_height` to allow some tolerance.

4. **Mode transition detection**: Detecting alternate-to-primary transitions requires tracking mode state across PTY polls. Need to verify this can be done reliably without race conditions.

5. **Viewport sharing**: `TerminalFocusTarget` currently doesn't have access to the viewport. Need to either:
   - Pass viewport as parameter to `handle_scroll()`
   - Store viewport reference in the target
   - Handle scrolling entirely in `EditorState` without delegating to the target

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->