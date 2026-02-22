---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/renderer.rs
- crates/editor/src/main.rs
- crates/editor/src/workspace.rs
code_references:
  - ref: crates/editor/src/workspace.rs#Editor::active_buffer_view
    implements: "Helper method to get active tab's BufferView, handling AgentTerminal placeholder"
  - ref: crates/editor/src/renderer.rs#Renderer::update_glyph_buffer
    implements: "Refactored to accept &dyn BufferView parameter for polymorphic rendering"
  - ref: crates/editor/src/renderer.rs#Renderer::render_with_editor
    implements: "Main render entry point - fetches BufferView from Editor and passes through"
  - ref: crates/editor/src/renderer.rs#Renderer::render_with_find_strip
    implements: "Find-in-file render path - also uses polymorphic BufferView access"
  - ref: crates/editor/src/renderer.rs#Renderer::apply_mutation
    implements: "Now accepts line_count parameter instead of reading from self.buffer"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- terminal_active_tab_safety
created_after:
- terminal_active_tab_safety
- cursor_pointer_ui_hints
- scroll_wrap_deadzone_v2
---

# Chunk Goal

## Minor Goal

The renderer currently owns a private `TextBuffer` copy (`self.buffer: Option<TextBuffer>`) and syncs it from the editor's active tab every frame via `EditorController::sync_renderer_buffer`. This method calls `self.state.buffer()` unconditionally, which panics when the active tab is a terminal tab (`"active tab is not a file tab"` at `editor_state.rs:128`).

The deeper issue: this copy-and-sync pattern exists even though `GlyphBuffer::update_from_buffer_with_wrap` already accepts `&dyn BufferView`, and `TerminalBuffer` already implements `BufferView`. The renderer should read directly from the editor's active tab via the `BufferView` trait instead of maintaining its own `TextBuffer` copy.

This chunk:

1. **Removes `self.buffer: Option<TextBuffer>` from the renderer** — the renderer no longer owns a buffer copy.
2. **Eliminates `sync_renderer_buffer`** — no more per-frame content copying.
3. **Threads `&dyn BufferView` from the active tab through the render path** — `render_with_editor` gets the active tab's `BufferView` from the `Editor` and passes it to `update_glyph_buffer` / `GlyphBuffer::update_from_buffer_with_wrap`.
4. **Enables terminal tab rendering** — since `TerminalBuffer` implements `BufferView`, terminal tabs render through the same code path as file tabs with no special casing.

## Success Criteria

- The renderer no longer owns a `TextBuffer` (field removed)
- `sync_renderer_buffer` in `main.rs` is deleted
- `render_with_editor` reads from the editor's active tab `BufferView` directly
- `Cmd+Shift+T` to spawn a terminal tab does not crash
- Key presses while a terminal tab is active do not crash in the render path
- Switching between file and terminal tabs renders each correctly
- All existing tests continue to pass