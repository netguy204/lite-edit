# Implementation Plan

## Approach

Extend the existing focus-aware cursor blink mechanism (from `cursor_blink_focus` chunk) to handle multi-pane tiling layouts. The strategy is straightforward:

1. **Focused pane**: Cursor blinks normally, following `self.cursor_visible` which toggles on/off via timer
2. **Unfocused panes**: Cursor is always visible (static, non-blinking)

This builds on the existing infrastructure:
- `Renderer::cursor_visible` tracks the blink state (toggled by timer)
- `Workspace::active_pane_id` tracks which pane has focus
- `render_pane()` already iterates over all panes and knows which one is focused

The implementation is a simple one-line conditional in the pane rendering code:
```rust
let pane_cursor_visible = if is_focused { self.cursor_visible } else { true };
```

This approach:
- Requires no new state tracking
- Reuses existing focus tracking (`active_pane_id`)
- Reuses existing blink mechanism (`cursor_visible`)
- Has O(1) overhead per pane (just a boolean check)

## Subsystem Considerations

No subsystems are directly relevant to this change. The cursor blink mechanism is part of the rendering pipeline but not documented as a formal subsystem.

## Sequence

### Step 1: Modify cursor visibility logic in render_pane

In `crates/editor/src/renderer.rs`, update the `render_pane` method to calculate pane-specific cursor visibility:

```rust
// Focused pane: cursor blinks (follows self.cursor_visible)
// Unfocused pane: static cursor (always visible)
let pane_cursor_visible = if is_focused { self.cursor_visible } else { true };
```

The `is_focused` check uses `pane_rect.pane_id == workspace.active_pane_id`.

Location: `crates/editor/src/renderer.rs` in the `render_pane` method

### Step 2: Verify existing infrastructure

Verify that:
- `update_glyph_buffer_with_cursor_visible` correctly renders cursor based on the boolean
- Focus transitions between panes properly update `active_pane_id`
- The blink timer continues to toggle `self.cursor_visible` regardless of focus

These were already implemented in prior chunks (`cursor_blink_focus`, `tiling_focus_keybindings`).

### Step 3: Run tests

Run the editor tests to ensure no regressions in cursor blink or pane focus behavior.

---

**Note on backreferences**: The comment `// Chunk: docs/chunks/cursor_blink_pane_focus` was already present in the code from initial scaffolding. The implementation fills in the correct logic for that comment.

## Dependencies

The following chunks must be complete before this chunk:
- `tiling_focus_keybindings` - Provides pane focus switching and `active_pane_id` tracking
- `tiling_multi_pane_render` - Provides multi-pane rendering loop that iterates over panes
- `cursor_blink_focus` - Provides the foundational cursor blink mechanism and `update_glyph_buffer_with_cursor_visible`

## Risks and Open Questions

- **Visual glitches**: Need to ensure there's no frame where both cursors blink or neither blinks during focus transitions. The implementation handles this by always showing static cursor for unfocused panes, so focus transitions should be smooth.
- **Terminal panes**: Terminal tabs have their own cursor rendering. Need to verify the same logic applies consistently. The implementation passes `pane_cursor_visible` to terminal rendering as well.

## Deviations

- **Existing scaffolding**: The code already had `// Chunk: docs/chunks/cursor_blink_pane_focus` comments and the initial implementation that set `cursor_visible: false` for unfocused panes. The fix changed this to show a **static** cursor (always `true`) for unfocused panes, matching the GOAL.md specification.