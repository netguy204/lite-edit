---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/renderer.rs
code_references:
  - ref: crates/editor/src/renderer.rs#Renderer::render_pane
    implements: "Pane-aware cursor visibility logic - focused pane gets blinking cursor, unfocused panes get static cursor"
  - ref: crates/editor/src/renderer.rs#Renderer::update_glyph_buffer_with_cursor_visible
    implements: "Helper method accepting explicit cursor_visible parameter for multi-pane rendering"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- tiling_focus_keybindings
- tiling_multi_pane_render
- startup_workspace_dialog
---

# Chunk Goal

## Minor Goal

In a multi-pane tiling layout, the blinking cursor should only appear in the pane that currently has focus. Unfocused panes should display a static (non-blinking) cursor or no cursor at all, providing clear visual feedback about which pane is receiving input.

The existing `cursor_blink_focus` chunk solved this for overlay mini-buffers vs the main edit buffer. This chunk extends that concept to the tiling/multi-pane workspace: when multiple panes are visible, only the focused pane's cursor blinks. This supports the project's goal of low-latency, responsive native editing by giving immediate visual feedback about input focus across split views.

## Success Criteria

- When multiple panes are visible, only the focused pane has a blinking cursor
- Unfocused panes display a static cursor (visible but not blinking)
- Switching focus between panes (via keybinding) immediately starts the cursor blinking in the newly focused pane and stops it in the previously focused one
- No visual glitches during focus transitions (no frame where both cursors blink or neither blinks)
- Overlay cursor blink behavior (from `cursor_blink_focus`) continues to work correctly within the focused pane