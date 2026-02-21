---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - Cargo.toml
  - src/lib.rs
  - src/gap_buffer.rs
  - src/line_index.rs
  - src/text_buffer.rs
  - src/types.rs
  - tests/editing_sequences.rs
  - tests/performance.rs
code_references:
  - ref: src/types.rs#Position
    implements: "Cursor position as (line, column) for line-based operations"
  - ref: src/types.rs#DirtyLines
    implements: "Dirty line information returned from mutations for render optimization"
  - ref: src/gap_buffer.rs#GapBuffer
    implements: "Low-level gap buffer data structure for O(1) insertions/deletions at cursor"
  - ref: src/line_index.rs#LineIndex
    implements: "Line boundary tracking with incremental updates for O(1) line access"
  - ref: src/text_buffer.rs#TextBuffer
    implements: "Main public API combining gap buffer, line index, and cursor tracking"
  - ref: src/text_buffer.rs#TextBuffer::insert_char
    implements: "Insert character at cursor with dirty line reporting"
  - ref: src/text_buffer.rs#TextBuffer::insert_newline
    implements: "Line split at cursor with dirty region tracking"
  - ref: src/text_buffer.rs#TextBuffer::delete_backward
    implements: "Backspace deletion with line join handling"
  - ref: src/text_buffer.rs#TextBuffer::delete_forward
    implements: "Delete key operation with line join handling"
  - ref: src/text_buffer.rs#TextBuffer::move_left
    implements: "Cursor movement left with line wrap"
  - ref: src/text_buffer.rs#TextBuffer::move_right
    implements: "Cursor movement right with line wrap"
  - ref: src/text_buffer.rs#TextBuffer::move_up
    implements: "Cursor movement up with column clamping"
  - ref: src/text_buffer.rs#TextBuffer::move_down
    implements: "Cursor movement down with column clamping"
  - ref: src/text_buffer.rs#TextBuffer::line_count
    implements: "O(1) line count access for rendering"
  - ref: src/text_buffer.rs#TextBuffer::line_content
    implements: "Line content retrieval for rendering"
  - ref: src/text_buffer.rs#TextBuffer::cursor_position
    implements: "Current cursor position for rendering"
  - ref: tests/editing_sequences.rs
    implements: "Integration tests for realistic editing patterns"
  - ref: tests/performance.rs
    implements: "Performance sanity checks including 100K character insertion"
narrative: null
investigation: editor_core_architecture
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after: []
---

# Text Buffer Data Structure

## Minor Goal

Implement the core text buffer that holds editor content and cursor state. Per GOAL.md, the text buffer is part of the "small core" — the data structure that the entire input→render critical path operates on. Per the investigation findings, focus targets mutate the buffer and the render loop reads from it, so its API must support both efficiently.

This chunk is purely a data structure with no rendering or macOS dependencies. It can be developed and tested independently of the Metal surface. It must return dirty line information on mutation so that downstream rendering (chunk 4) can use `DirtyRegion` tracking to minimize redraws.

A gap buffer is the right starting point: simpler than a rope, sufficient for the initial editable buffer milestone, and easily replaceable later when large-file support demands a rope. The API should be designed so that swapping the backing store doesn't change the interface.

## Success Criteria

- A `TextBuffer` type exists with the following operations, each returning dirty line information:
  - `insert_char(ch)` — insert a character at the cursor position
  - `insert_newline()` — split the current line at the cursor
  - `delete_backward()` — delete the character before the cursor (Backspace)
  - `delete_forward()` — delete the character after the cursor (Delete key)
- Cursor movement operations (no dirty lines, but cursor position changes):
  - `move_left()`, `move_right()`, `move_up()`, `move_down()`
  - `move_to_line_start()`, `move_to_line_end()`
  - `move_to_buffer_start()`, `move_to_buffer_end()`
- Line access for rendering:
  - `line_count() → usize`
  - `line_content(line_index) → &str`
  - `cursor_position() → (line, column)`
- Dirty information returned from mutations indicates which lines changed, sufficient to populate a `DirtyRegion::Lines { from, to }` or `DirtyRegion::FullViewport`.
- Unit tests covering:
  - Insert and delete at beginning, middle, and end of a line
  - Newline insertion and backspace across line boundaries (joining lines)
  - Cursor movement at buffer boundaries (start of buffer, end of buffer, empty lines)
  - Multi-character sequences (simulating typing a word, then deleting it)
  - Dirty line information is correct for each operation
- No macOS or rendering dependencies — the buffer compiles and tests on any platform.
- Performance: inserting 100K characters sequentially completes in under 100ms (sanity check, not the real benchmark).