---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/tab_width.rs
  - crates/editor/src/lib.rs
  - crates/editor/src/glyph_buffer.rs
  - crates/editor/src/wrap_layout.rs
  - crates/editor/src/buffer_target.rs
  - crates/editor/tests/tab_rendering_test.rs
code_references:
  - ref: crates/editor/src/tab_width.rs
    implements: "Tab-stop arithmetic and visual width calculation helpers"
  - ref: crates/editor/src/tab_width.rs#TAB_WIDTH
    implements: "Compile-time tab width constant (4 columns)"
  - ref: crates/editor/src/tab_width.rs#next_tab_stop
    implements: "Next tab stop column calculation"
  - ref: crates/editor/src/tab_width.rs#char_visual_width
    implements: "Per-character visual width (tabs expand based on position)"
  - ref: crates/editor/src/tab_width.rs#line_visual_width
    implements: "Total visual width of a line with tabs and wide chars"
  - ref: crates/editor/src/tab_width.rs#char_col_to_visual_col
    implements: "Buffer column (char index) to visual column conversion"
  - ref: crates/editor/src/tab_width.rs#visual_col_to_char_col
    implements: "Visual column to buffer column (char index) conversion"
  - ref: crates/editor/src/wrap_layout.rs#WrapLayout::screen_rows_for_line_content
    implements: "Tab-aware screen row count for wrapped lines"
  - ref: crates/editor/src/wrap_layout.rs#WrapLayout::char_col_to_screen_pos
    implements: "Tab-aware buffer column to screen position mapping"
  - ref: crates/editor/src/wrap_layout.rs#WrapLayout::screen_pos_to_char_col
    implements: "Tab-aware screen position to buffer column (hit-testing)"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_wrap
    implements: "Tab-aware glyph rendering, selection, cursor, and underline quads"
  - ref: crates/editor/src/buffer_target.rs#pixel_to_buffer_position_wrapped
    implements: "Tab-aware mouse hit-testing (visual column to char index)"
narrative: null
investigation: treesitter_editing
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- pty_wakeup_reliability
---

# Chunk Goal

## Minor Goal

Add proper tab character rendering and tab-aware coordinate arithmetic throughout the editor. Currently, tab characters (`'\t'`) are stored as literal single characters in the gap buffer, but the rendering pipeline has no tab-expansion logic — tabs render as replacement glyphs (U+FFFD or solid blocks) occupying a single column width. Every layer that converts between character indices and visual columns treats a tab identically to any other character, making files with tab indentation unusable.

This chunk introduces a `TAB_WIDTH` constant (hardcoded to 4 — no configuration system exists yet), then threads tab-stop-aware column arithmetic through every layer that maps between buffer positions and visual positions:

- **Rendering**: The glyph buffer rendering loops must expand `'\t'` to whitespace spanning from the current visual column to the next tab stop (`next_tab_stop = ((visual_col / tab_width) + 1) * tab_width`). No glyph is emitted — the tab just advances the column counter like spaces do.
- **Cursor positioning**: Cursor pixel position (`col * glyph_width`) must use the visual column, not the character index. A tab at character column 0 with tab_width=4 means the cursor at character column 1 renders at visual column 4.
- **Mouse hit-testing**: `pixel_to_buffer_position` must reverse the tab-expanded visual column back to a character index, accounting for tab stops on the target line.
- **Wrap layout**: `WrapLayout` must use visual column widths (accounting for tab expansion) when computing wrap boundaries, not raw character counts.
- **Line width calculations**: Any code that computes a line's visual width (for horizontal scrolling, viewport sizing, etc.) must account for tab expansion.

This is a prerequisite for intelligent indentation (`treesitter_indent`) — files using tab indentation must render correctly before smart indent can be useful. It also directly supports GOAL.md's requirement that "the editor can open, edit, and save source files" — files with tabs are currently broken.

## Success Criteria

- Tab characters render as whitespace spanning to the next tab stop, not as replacement glyphs
- A `TAB_WIDTH` constant (value 4) controls tab stop positions. This is a compile-time constant for now — a runtime configuration system does not exist yet and is out of scope for this chunk
- Cursor positioning is visually correct when the cursor is on or after tab characters — the cursor appears at the correct visual column, not at `char_index * glyph_width`
- Mouse clicking on a line containing tabs places the cursor at the correct character position, accounting for the visual width of preceding tabs
- Text selection across tab characters highlights the correct visual region
- Word wrap accounts for tab expansion — a line with tabs wraps at the correct visual column boundary, not at a raw character count
- Horizontal scrolling (if applicable) accounts for tab-expanded line widths
- Opening a file that uses tab indentation (e.g., a Go source file, a Makefile) looks correct — indented code is visually aligned at tab stops
- Syntax highlighting spans are visually correct on lines containing tabs (span start/end positions align with the expanded tab columns)
- The Tab key still inserts a literal `'\t'` character (no expand-tab-to-spaces behavior in this chunk — that can be added later as a separate feature)

## Rejected Ideas

### Expand tabs to spaces on file load

Convert all `'\t'` characters to spaces when reading a file, so the rendering pipeline doesn't need tab-awareness.

Rejected because: This silently modifies file content. Files would be saved with spaces even if the original used tabs, breaking formatting for projects that require tab indentation (Go, Makefiles, .editorconfig-governed projects). The buffer must faithfully store what the file contains.

### Add expand-tab-to-spaces on insertion (Tab key inserts spaces)

Make the Tab key insert N space characters instead of `'\t'`.

Rejected because: This is a separate, orthogonal feature (often called "expandtab" or "soft tabs"). It should be a user preference, not bundled with rendering fixes. This chunk focuses on making literal tab characters render correctly. Expand-on-insert can be added later, potentially informed by `.editorconfig` or per-language defaults.