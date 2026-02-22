# Implementation Plan

## Approach

The bug is that `EditorState::update_viewport_size` and `update_viewport_dimensions`
pass the **full** window height to `viewport.update_size()`, but the actual text
content area is smaller by `TAB_BAR_HEIGHT` pixels (the tab bar occupies space at
the top of the window).

This causes `visible_lines` to be overcounted by approximately one line, which in
turn makes `max_offset_px` too small. The user cannot scroll far enough to fully
reveal the last line of the buffer.

The fix is straightforward:
1. Compute `content_height = window_height - TAB_BAR_HEIGHT` before calling
   `viewport.update_size()`.
2. Pass `content_height` instead of `window_height` to the viewport.

This ensures the viewport calculates `visible_lines` based on the actual pixel
area available for text, not the full window.

**Critical constraint from GOAL.md**: The `view_height` field stored on
`EditorState` must remain the **full** window height. It is used for:
- Mouse-coordinate flipping (NSView uses bottom-left origin)
- Tab bar hit-testing
- Selector overlay geometry
- Rail hit-testing

Only the value forwarded to `viewport.update_size()` changes.

Following docs/trunk/TESTING_PHILOSOPHY.md, we write the regression test first
(red phase), then implement the fix.

## Sequence

### Step 1: Write failing regression test

Create a test in `editor_state.rs` that verifies `visible_lines` is computed from
the content area height, not the full window height.

**Test logic:**
- Create an `EditorState` with a known line height (e.g., 16.0px)
- Set window height to 192px (TAB_BAR_HEIGHT=32, so content_height=160)
- Call `update_viewport_dimensions(800.0, 192.0)`
- Assert `visible_lines == 10` (160 / 16), **not** 12 (192 / 16)

The test will fail initially because the current code passes 192 to the viewport.

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 2: Implement the fix in update_viewport_size

Modify `EditorState::update_viewport_size` to compute the content height before
passing to the viewport:

```rust
pub fn update_viewport_size(&mut self, window_height: f32) {
    let line_count = self.buffer().line_count();
    let content_height = window_height - TAB_BAR_HEIGHT;
    self.viewport_mut().update_size(content_height, line_count);
    self.view_height = window_height;  // Keep full height for coordinate flipping
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 3: Implement the fix in update_viewport_dimensions

Apply the same fix to `update_viewport_dimensions`:

```rust
pub fn update_viewport_dimensions(&mut self, window_width: f32, window_height: f32) {
    let line_count = self.buffer().line_count();
    let content_height = window_height - TAB_BAR_HEIGHT;
    self.viewport_mut().update_size(content_height, line_count);
    self.view_height = window_height;  // Keep full height for coordinate flipping
    self.view_width = window_width;
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 4: Add backreference comment

Add a chunk backreference to both methods indicating this fix:

```rust
// Chunk: docs/chunks/scroll_max_last_line - Pass content_height to viewport
```

### Step 5: Verify the regression test now passes

Run the new test to confirm it passes with the fix in place.

### Step 6: Run existing tests

Run the full test suite to verify no regressions in:
- Click-to-cursor alignment (from `resize_click_alignment` chunk)
- Viewport scroll clamping
- Selector overlay geometry
- Tab bar/rail hit-testing

The existing tests may need adjustment if they relied on the incorrect behavior.
Any test that calls `update_viewport_size(160.0)` expecting 10 visible lines will
still work correctly because 160 - 32 = 128, and 128 / 16 = 8 visible lines. We
may need to adjust expected values or use larger window heights to maintain the
same effective visible line counts.

### Step 7: Verify no changes to coordinate flipping behavior

Manually verify (or add a test assertion) that `view_height` is still the full
window height after the fix. The mouse coordinate flipping logic uses
`view_height` to transform NSView coordinates:

```rust
let content_y = (self.view_height as f64 - mouse_y) - TAB_BAR_HEIGHT as f64;
```

This must continue to work correctly. The fix changes what goes to the viewport,
not what is stored in `view_height`.

## Dependencies

This chunk depends on the completed work from `resize_click_alignment`, which
established the pattern of passing `line_count` to `viewport.update_size()` for
scroll clamping. That pattern is already in place.

## Risks and Open Questions

1. **Test adjustments**: Existing tests that call `update_viewport_size(160.0)`
   expecting 10 visible lines will now get 8 visible lines (because content_height
   = 160 - 32 = 128). These tests may need to:
   - Use window heights that account for TAB_BAR_HEIGHT (e.g., 192.0 for 10 lines)
   - Have their expected values adjusted

   This is acceptable because the tests were implicitly testing incorrect behavior.

2. **Wrapped line scroll handling**: `ensure_visible_wrapped` computes its own
   max_offset_px. Verify this code path is unaffected (it uses visible_lines
   which will now be correct, so it should benefit from the fix).

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->