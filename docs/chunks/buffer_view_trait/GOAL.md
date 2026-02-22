---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/buffer/src/buffer_view.rs
  - crates/buffer/src/lib.rs
  - crates/buffer/src/text_buffer.rs
  - crates/editor/src/glyph_buffer.rs
  - crates/editor/src/renderer.rs
  - crates/editor/src/main.rs
  - crates/editor/src/context.rs
code_references:
  - ref: crates/buffer/src/buffer_view.rs#BufferView
    implements: "Core BufferView trait - unified rendering interface for file and terminal buffers"
  - ref: crates/buffer/src/buffer_view.rs#Color
    implements: "Terminal color representation (default, named ANSI, indexed 256, RGB)"
  - ref: crates/buffer/src/buffer_view.rs#NamedColor
    implements: "The 16 standard ANSI colors enum"
  - ref: crates/buffer/src/buffer_view.rs#UnderlineStyle
    implements: "5-variant underline styles (single, double, curly, dotted, dashed)"
  - ref: crates/buffer/src/buffer_view.rs#Style
    implements: "Terminal-grade text styling attributes (fg/bg, bold, italic, dim, underline variants, strikethrough, inverse, hidden)"
  - ref: crates/buffer/src/buffer_view.rs#Span
    implements: "Styled text run - a contiguous run of text with uniform Style"
  - ref: crates/buffer/src/buffer_view.rs#StyledLine
    implements: "Line as renderer sees it - sequence of Spans"
  - ref: crates/buffer/src/buffer_view.rs#CursorShape
    implements: "Cursor shape enum (Block, Beam, Underline, Hidden)"
  - ref: crates/buffer/src/buffer_view.rs#CursorInfo
    implements: "Cursor position, shape, and blink state for rendering"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer
    implements: "BufferView trait implementation for TextBuffer (returns single unstyled span per line)"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::accumulate_dirty
    implements: "Dirty line accumulation for BufferView::take_dirty()"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer
    implements: "Renderer integration accepting &dyn BufferView instead of TextBuffer"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_cursor
    implements: "Full BufferView-aware rendering with cursor, selection, and styled line extraction"
narrative: null
investigation: hierarchical_terminal_tabs
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
- word_forward_delete
- word_jump_navigation
---

# Chunk Goal

## Minor Goal

Introduce the `BufferView` trait — the foundational abstraction that unifies file editing buffers and terminal emulator buffers behind a common rendering interface. This is the keystone of the hierarchical terminal tabs architecture.

Define in the `buffer` crate:
- **`BufferView` trait**: `line_count()`, `styled_line(n)`, `take_dirty()`, `is_editable()`, `cursor_info()`
- **`StyledLine`**: A line as the renderer sees it — a sequence of `Span`s
- **`Span`**: A run of text with a `Style`
- **`Style`**: Terminal-grade attributes from day one — fg/bg color, bold, italic, dim, underline (5 variants: single, double, curly, dotted, dashed) + underline color, strikethrough, inverse, hidden
- **`CursorInfo`**: Position, shape (Block, Beam, Underline, Hidden), blinking state
- **`Color`**: Named (16 ANSI), indexed (256), and RGB

Implement `BufferView` for the existing `TextBuffer` (returns single unstyled span per line, default style).

Migrate the renderer from consuming `&[&str]` via `set_content()` to consuming `&dyn BufferView`. The renderer should iterate `styled_line()` calls for visible lines and use `take_dirty()` to minimize redraws.

This chunk enables all downstream work: terminal buffers, workspace tabs, and agent integration all depend on the BufferView contract.

## Success Criteria

- `BufferView` trait is defined and object-safe (usable as `Box<dyn BufferView>` and `&dyn BufferView>`)
- `Style` type supports all terminal-grade attributes listed above (not just fg color)
- `TextBuffer` implements `BufferView`, returning single-span unstyled lines
- The renderer in `crates/editor` consumes `&dyn BufferView` instead of `&[&str]`
- Existing rendering behavior is unchanged — the demo text still renders identically
- `DirtyLines` from `TextBuffer` flows through `BufferView::take_dirty()` correctly
- Unit tests verify `TextBuffer`'s `BufferView` implementation (line count, styled line content, cursor info, dirty tracking)