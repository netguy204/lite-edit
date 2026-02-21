---
status: FUTURE
ticket: null
parent_chunk: null
code_paths: []
code_references: []
narrative: editor_qol_interactions
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
  - text_selection_model
created_after: ["editable_buffer", "glyph_rendering", "metal_surface", "viewport_rendering"]
---

# Selection Rendering

## Minor Goal

Render selected text with a visible background highlight so the user can see what is selected. Currently the renderer draws glyphs on a solid background with no concept of selection. This chunk adds highlight quads behind selected character cells, drawn before the glyph pass so text remains readable on top of the highlight.

This depends on the text selection model chunk which provides `selection_range()` on `TextBuffer`.

## Success Criteria

- **Selection highlight color**: Use a semi-transparent or distinct background color for selected text. A reasonable default is the Catppuccin Mocha "surface2" color (`#585b70`) or similar — distinct enough to be visible against the `#1e1e2e` background but not so bright it overwhelms the text.

- **Renderer queries selection state**: During rendering, the renderer checks `buffer.selection_range()`. If a selection exists, it determines which visible lines (within the viewport) intersect the selection range.

- **Highlight quads are drawn for selected regions**: For each visible line that intersects the selection:
  - Calculate the start and end columns of the selection on that line
  - Emit a colored quad covering `(start_col * char_width, line_y)` to `(end_col * char_width, line_y + line_height)`
  - For lines fully within the selection, the quad spans the entire line width (or from column 0 to line length + 1 to include the newline visual space)
  - For partially selected lines, the quad covers only the selected columns

- **Selection highlight is drawn before glyphs**: The highlight quads must be rendered before the glyph quads so text is drawn on top. This may use:
  - A separate render pass / draw call for solid-color quads before the textured glyph draw call
  - Or the same pipeline with the glyph atlas texture disabled (using a solid-color fragment shader variant)
  - The simplest approach is likely a solid-color pipeline state that draws untextured quads.

- **Selection state synced to renderer**: The renderer's copy of `TextBuffer` (synced in `EditorController::sync_renderer_buffer`) must include the selection anchor so `selection_range()` returns the correct value during rendering. Ensure `TextBuffer::from_str` + `set_cursor` reconstruction preserves or re-establishes the selection state.

- **Dirty region tracking**: When selection changes (anchor set, cursor moves during drag, selection cleared), the affected lines must be marked dirty so the renderer redraws them with or without the highlight.

- **Visual test**: Selecting text via mouse drag (or programmatically in tests) shows a visible highlight behind the selected characters. Deselecting (clicking elsewhere) removes the highlight.
