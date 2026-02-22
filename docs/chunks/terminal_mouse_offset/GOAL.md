---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse_buffer
    implements: "Fixed terminal mouse Y coordinate calculation to use content_height flip and scroll_fraction_px compensation"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- terminal_pty_wakeup
- terminal_alt_backspace
---

# Chunk Goal

## Minor Goal

Fix the ~3-line vertical offset when clicking inside a terminal tab running a program that accepts mouse input (e.g., vim, htop). The mouse position reported to the PTY is approximately 3 rows above where the user actually clicked.

The bug is in `EditorState::handle_mouse_buffer` (crates/editor/src/editor_state.rs, around line 1340). The terminal mouse coordinate calculation does:

```rust
let adjusted_y = self.view_height as f64 - TAB_BAR_HEIGHT as f64 - y;
let row = (adjusted_y / cell_height as f64) as usize;
```

This Y-flip formula is likely wrong. In NSView coordinates, y=0 is at the bottom of the view. The formula `view_height - TAB_BAR_HEIGHT - y` produces a value that's 0 at the *top* of the content area (where the tab bar ends) and increases downward — which seems correct for mapping to terminal row 0 at top. However, the ~3-row offset suggests something else is off: possibly the rendering origin doesn't match the assumed coordinate space (e.g., the terminal content is rendered with additional padding or offset that isn't accounted for here), or there's an off-by-one in how view_height relates to the actual NSView frame.

Note that TAB_BAR_HEIGHT is 32.0 and cell_height is ~16px, so the tab bar accounts for only ~2 rows, not the observed ~3. The extra offset likely comes from another source (e.g., rendering y-offset, scale factor, or the content scissor rect).

The fix should investigate the actual rendering origin for terminal content and align the mouse coordinate calculation accordingly. Alternatively, use `TerminalFocusTarget::handle_mouse` + `pixel_to_cell` which already exists but is bypassed in the current inline calculation.

## Success Criteria

- Clicking inside a terminal tab running vim/htop positions the cursor at the exact row the user clicked, not ~3 rows above.
- The mouse coordinate transformation for terminal tabs correctly accounts for RAIL_WIDTH, TAB_BAR_HEIGHT, and the NSView Y-flip.
- Existing terminal mouse encoding tests continue to pass.
- Manual verification: open vim in a terminal tab, click at various positions, confirm cursor lands where clicked.



