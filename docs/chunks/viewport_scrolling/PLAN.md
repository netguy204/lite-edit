# Implementation Plan

## Approach

This chunk implements scroll event handling and the "ensure cursor visible" snap-back behavior. The architecture follows the existing Humble View pattern from TESTING_PHILOSOPHY.md: scroll events mutate `Viewport.scroll_offset` through the focus target, cursor position is unchanged, and when a keystroke is received while the cursor is off-screen, the viewport snaps back to make the cursor visible before the edit is processed.

The key insight from the existing codebase is that `ensure_cursor_visible()` in `EditorContext` already handles the snap-back logic — we just need to ensure it's called at the right time (before keystroke processing when cursor is off-screen) and that scroll events are properly wired through the macOS event system.

**Strategy:**

1. **Wire scroll events through macOS**: Add `scrollWheel:` handler to `MetalView` that converts `NSEvent` scroll deltas to our `ScrollDelta` type and invokes a callback.

2. **Extend input handling**: Add a scroll handler to `EditorController` that forwards to `EditorState`, which delegates to the focus target's `handle_scroll` method.

3. **Ensure cursor visibility on keystroke**: Modify `EditorState.handle_key()` to call `ensure_cursor_visible()` before delegating to the focus target when the cursor is off-screen.

4. **Cursor rendering**: The existing code in `glyph_buffer.rs` already checks `viewport.buffer_line_to_screen_line()` before rendering the cursor — if the cursor is off-screen, `buffer_line_to_screen_line()` returns `None` and no cursor quad is generated.

**Testing approach per TESTING_PHILOSOPHY.md:**

Since scroll input and viewport mutation are pure state operations, tests can exercise the full event-to-state pipeline without a GPU:

- Scroll events mutate `scroll_offset` correctly
- Cursor position is unchanged by scroll
- Cursor off-screen after scroll produces no cursor quad in glyph buffer
- Keystroke when cursor is off-screen triggers snap-back before mutation

Visual verification will confirm macOS scroll event integration.

## Sequence

### Step 1: Add scroll handler callback infrastructure to MetalView

Extend `MetalViewIvars` with a `scroll_handler: RefCell<Option<ScrollHandler>>` and add a `scrollWheel:` method override that converts the NSEvent scroll delta to our `ScrollDelta` type and invokes the callback.

The macOS scroll delta is in pixels; we'll convert to line-based scrolling in the focus target (which has access to line_height via the viewport).

**Files:** `crates/editor/src/metal_view.rs`

**Tests:** Manual verification (scroll event handling is platform code)

### Step 2: Wire scroll handler through EditorController and EditorState

Add `set_scroll_handler()` to `MetalView` and wire it in `AppDelegate.setup_window()` to call `EditorController.handle_scroll()`.

Add `handle_scroll(delta: ScrollDelta)` to `EditorState` that forwards to the focus target's `handle_scroll()` method. This follows the same pattern as `handle_key()` and `handle_mouse()`.

**Files:** `crates/editor/src/metal_view.rs`, `crates/editor/src/main.rs`, `crates/editor/src/editor_state.rs`

**Tests:** Unit tests for `EditorState.handle_scroll()` verifying viewport mutation and dirty marking.

### Step 3: Implement scroll handling in BufferFocusTarget

The `FocusTarget` trait already declares `handle_scroll(&mut self, delta: ScrollDelta, ctx: &mut EditorContext)` and `BufferFocusTarget` already implements it — the logic converts pixel delta to lines, adjusts `scroll_offset`, and marks `FullViewport` dirty.

Verify this implementation is correct:
- Positive dy = scroll down = content moves up = `scroll_offset` increases
- Negative dy = scroll up = content moves down = `scroll_offset` decreases
- Clamping is handled by `Viewport.scroll_to()`

**Files:** `crates/editor/src/buffer_target.rs` (verify existing implementation)

**Tests:** Unit tests for scroll delta → viewport offset conversion with various delta values.

### Step 4: Implement "ensure cursor visible" before keystroke processing

Modify `EditorState.handle_key()` to check if the cursor is currently off-screen and, if so, call `ensure_cursor_visible()` BEFORE delegating to the focus target.

The check is: `viewport.buffer_line_to_screen_line(buffer.cursor_position().line).is_none()`.

This ensures that any keystroke that would mutate the buffer (or move the cursor) first snaps the viewport back to make the cursor visible.

**Files:** `crates/editor/src/editor_state.rs`

**Tests:**
- Create a long buffer, scroll cursor off-screen, send keystroke → viewport should snap back
- Verify buffer mutation is applied after snap-back

### Step 5: Verify cursor is not rendered when off-screen

The existing code in `GlyphBuffer.update_from_buffer_with_cursor()` already checks `viewport.buffer_line_to_screen_line(cursor_pos.line)` before generating the cursor quad. If this returns `None`, no cursor is rendered.

Add a targeted unit test to confirm this behavior.

**Files:** (verification only — no code changes expected)

**Tests:** Unit test that creates a viewport+buffer state with cursor off-screen and verifies no cursor quad is generated.

### Step 6: Integration test and visual verification

Create an integration test that:
1. Sets up a buffer with many lines (50+)
2. Scrolls down programmatically (via scroll delta)
3. Verifies cursor position unchanged
4. Sends a keystroke
5. Verifies viewport snapped back and cursor is visible

Visual verification:
1. Run the editor with the demo buffer
2. Scroll using trackpad/mouse wheel
3. Verify viewport moves but cursor stays at its buffer position
4. Verify cursor disappears when scrolled off-screen
5. Type a character → viewport should snap back to cursor

**Files:** `crates/editor/tests/viewport_test.rs` or new `scroll_test.rs`

## Dependencies

**Chunks that must be complete:**
- `viewport_rendering` (ACTIVE) — Provides `Viewport` with `scroll_to()`, `ensure_visible()`, `buffer_line_to_screen_line()`
- `editable_buffer` (ACTIVE) — Provides `EditorState`, `FocusTarget` trait, event handling infrastructure

**External dependencies:** None — scroll event handling uses existing macOS/objc2-app-kit bindings.

## Risks and Open Questions

1. **Scroll delta direction convention**: macOS uses "natural scrolling" by default where positive delta = scroll down. The existing `BufferFocusTarget.handle_scroll()` implementation assumes positive dy = scroll down. Need to verify this matches macOS behavior.

2. **Scroll momentum/inertia**: macOS provides momentum scroll events after the user lifts their finger. These should work automatically since we handle each scroll event incrementally.

3. **Scroll sensitivity**: The conversion from pixel delta to lines uses `(delta.dy / line_height).round()`. This may feel too slow or too fast depending on user preferences. A sensitivity multiplier could be added later.

4. **Fractional scrolling**: Currently we scroll by whole lines. Smooth scrolling (fractional scroll_offset) is a future enhancement and out of scope.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->