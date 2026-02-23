---
decision: FEEDBACK
summary: Core multi-pane cursor visibility works, but unfocused panes hide cursor completely rather than showing a static cursor as specified in GOAL.md
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: When multiple panes are visible, only the focused pane has a blinking cursor

- **Status**: satisfied
- **Evidence**:
  - `renderer.rs:1602` computes `let pane_cursor_visible = is_focused && self.cursor_visible;`
  - Only when `is_focused` is true AND the blink timer says `cursor_visible` is true does the cursor appear
  - Unfocused panes always pass `false` for cursor visibility

### Criterion 2: Unfocused panes display a static cursor (visible but not blinking)

- **Status**: gap
- **Evidence**:
  - GOAL.md specifies: "Unfocused panes display a static cursor (visible but not blinking)"
  - Implementation at `renderer.rs:1600-1602` states: "Unfocused pane: no cursor (always false)"
  - The code passes `pane_cursor_visible = false` for unfocused panes, which means NO cursor is rendered
  - A static cursor would require passing `true` for unfocused panes (so cursor is always visible)
  - The current behavior is: unfocused panes have NO visible cursor, not a static visible one

### Criterion 3: Switching focus between panes (via keybinding) immediately starts the cursor blinking in the newly focused pane and stops it in the previously focused one

- **Status**: satisfied
- **Evidence**:
  - `workspace.rs:578-592` `switch_focus()` changes `active_pane_id` immediately
  - `editor_state.rs:636-638` triggers `dirty_region.merge(DirtyRegion::FullViewport)` on focus change
  - The next render pass will use the new `active_pane_id` to determine which pane is focused
  - Note: Unlike click-to-focus (which updates `last_keystroke`), keybinding focus switch does not reset the cursor blink phase - this means if the cursor was in the "off" phase of blinking, it may remain briefly off in the newly focused pane until the next blink toggle

### Criterion 4: No visual glitches during focus transitions (no frame where both cursors blink or neither blinks)

- **Status**: satisfied
- **Evidence**:
  - Single atomic `active_pane_id` assignment ensures exactly one pane is focused per frame
  - The rendering logic checks `is_focused` for each pane independently
  - Only the focused pane's cursor follows `self.cursor_visible`; unfocused panes always pass `false`
  - No race conditions possible within a single render pass

### Criterion 5: Overlay cursor blink behavior (from `cursor_blink_focus`) continues to work correctly within the focused pane

- **Status**: satisfied
- **Evidence**:
  - Overlay rendering (`draw_selector_overlay` at line 1214-1215) happens after all pane rendering
  - Overlay cursor uses separate `overlay_cursor_visible` state and `last_overlay_keystroke` timestamp
  - The pane cursor visibility changes do not affect the overlay cursor state
  - `main.rs:311-350` correctly routes cursor visibility based on `EditorFocus` enum

## Feedback Items

- **id**: issue-unfocused-cursor-hidden
  **location**: crates/editor/src/renderer.rs:1600-1602
  **concern**: The implementation hides the cursor entirely in unfocused panes, but GOAL.md specifies unfocused panes should "display a static cursor (visible but not blinking)"
  **suggestion**: Change the logic to pass `true` (always visible) for unfocused panes instead of `false`. For example:
  ```rust
  // Focused pane: cursor blinks (shows/hides based on self.cursor_visible)
  // Unfocused pane: static cursor (always visible)
  let pane_cursor_visible = if is_focused { self.cursor_visible } else { true };
  ```
  Alternatively, if hiding the cursor in unfocused panes is the intended design, update GOAL.md to reflect this behavior.
  **severity**: functional
  **confidence**: high
