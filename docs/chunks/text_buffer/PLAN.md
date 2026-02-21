<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Implement a gap buffer-backed text buffer with cursor position tracking. Per the GOAL.md and investigation findings, a gap buffer is simpler than a rope and sufficient for the initial editable buffer milestone.

The key design insight from the investigation is that the buffer API must return dirty line information from mutations so that downstream rendering (viewport_rendering chunk) can minimize redraws via `DirtyRegion` tracking.

**Strategy:**

1. **Gap buffer as backing store**: A gap buffer is a character array with a movable gap at the cursor position. Insertions and deletions at the cursor are O(1); moving the cursor is O(gap_distance) but amortizes well for typical editing patterns.

2. **Line-aware indexing**: Maintain a separate line index (array of line start offsets) for O(1) line count and O(1) line content access. This avoids O(n) scans on every render.

3. **Dirty tracking via return values**: Each mutating operation returns a `DirtyLines` struct indicating which line(s) changed. This is simpler than callback-based notification and matches the drain-all-then-render pattern where dirty regions accumulate then render once.

4. **Cursor as (line, column)**: Store cursor as a logical position, not a byte offset. This simplifies line-based operations and matches what the viewport needs.

5. **API designed for swappability**: The public API hides the gap buffer implementation. When rope-backed buffers are needed for large files, only the internals change.

**Testing**: Unit tests will cover all success criteria: insert/delete at buffer boundaries, line join/split, cursor movement at boundaries, dirty line correctness. A simple benchmark will verify the 100K character insertion sanity check.

## Sequence

### Step 1: Project initialization

Create the Rust project structure with Cargo.toml. The buffer will be a library crate (`lite-edit-buffer` or similar) with no macOS or rendering dependencies. Only standard library dependencies to start.

Location: `Cargo.toml`, `src/lib.rs`

### Step 2: Define core types

Define the public types that form the buffer's API:

```rust
/// Position in the buffer as (line, column) where both are 0-indexed.
pub struct Position {
    pub line: usize,
    pub col: usize,
}

/// Information about which lines were dirtied by a mutation.
/// Used by the render loop to compute DirtyRegion.
pub enum DirtyLines {
    /// No lines changed (e.g., cursor-only movement).
    None,
    /// A single line changed (most insertions, deletions within a line).
    Single(usize),
    /// A range of lines changed [from, to). Used when lines are joined/split.
    Range { from: usize, to: usize },
    /// Everything from a line to the end of the buffer changed.
    /// Used when a line split pushes all subsequent lines down.
    FromLineToEnd(usize),
}
```

Location: `src/lib.rs` or `src/types.rs`

### Step 3: Implement the gap buffer

Implement the low-level gap buffer data structure:

```rust
struct GapBuffer {
    data: Vec<char>,  // Using char for simplicity; could use u8 + UTF-8 later
    gap_start: usize,
    gap_end: usize,
}
```

Operations:
- `insert(ch: char)` — insert at gap, grow gap if needed
- `delete_backward()` — expand gap leftward (delete char before cursor)
- `delete_forward()` — expand gap rightward (delete char after cursor)
- `move_gap_to(pos: usize)` — relocate the gap to a new position
- `content() -> &[char]` — return contiguous content (moves gap to end)
- `len()` — logical length (excluding gap)

This is an internal implementation detail; not exposed publicly.

Location: `src/gap_buffer.rs`

### Step 4: Implement line index

Build a line index that tracks line boundaries:

```rust
struct LineIndex {
    /// Byte offsets where each line starts. lines[0] = 0 always.
    line_starts: Vec<usize>,
}
```

Operations:
- `rebuild(content: &[char])` — full rebuild after bulk operations
- `line_count() -> usize`
- `line_start(line: usize) -> usize`
- `line_end(line: usize) -> usize` — character before the newline (or buffer end)
- `line_at_offset(offset: usize) -> usize` — which line contains this offset (binary search)
- `insert_newline_at(line: usize, col: usize)` — incremental update for newline insertion
- `remove_newline_at(line: usize)` — incremental update for line join

The incremental updates are key for performance: inserting a newline should be O(lines_below) to shift line starts, not O(total_chars) to rebuild.

Location: `src/line_index.rs`

### Step 5: Implement TextBuffer public API

Create the main `TextBuffer` type that composes the gap buffer and line index:

```rust
pub struct TextBuffer {
    buffer: GapBuffer,
    line_index: LineIndex,
    cursor: Position,
}
```

Implement the required operations from GOAL.md:

**Mutations (return DirtyLines):**
- `insert_char(ch: char) -> DirtyLines` — insert at cursor, advance cursor
- `insert_newline() -> DirtyLines` — split line, cursor to start of new line
- `delete_backward() -> DirtyLines` — delete char before cursor (Backspace)
- `delete_forward() -> DirtyLines` — delete char after cursor (Delete key)

**Cursor movement (no dirty, but updates cursor position):**
- `move_left()`
- `move_right()`
- `move_up()`
- `move_down()`
- `move_to_line_start()`
- `move_to_line_end()`
- `move_to_buffer_start()`
- `move_to_buffer_end()`

**Line access for rendering:**
- `line_count() -> usize`
- `line_content(line_index: usize) -> &str` or `-> String`
- `cursor_position() -> Position`

Location: `src/text_buffer.rs` or directly in `src/lib.rs`

### Step 6: Implement dirty tracking logic

Each mutation method computes the correct `DirtyLines`:

| Operation | DirtyLines |
|-----------|-----------|
| `insert_char` (non-newline) | `Single(cursor.line)` |
| `insert_newline` | `FromLineToEnd(cursor.line)` — current line truncated, all below shift down |
| `delete_backward` (not at line start) | `Single(cursor.line)` |
| `delete_backward` (at line start, joins lines) | `FromLineToEnd(cursor.line - 1)` — previous line extended, all below shift up |
| `delete_forward` (not at line end) | `Single(cursor.line)` |
| `delete_forward` (at line end, joins lines) | `FromLineToEnd(cursor.line)` — current line extended, all below shift up |

This matches the investigation's H3 analysis: most operations dirty 1 line, line split/join dirty from mutation point to bottom.

Location: integrated into `TextBuffer` methods from Step 5

### Step 7: Unit tests for basic operations

Test each operation in isolation:

1. **Insert tests:**
   - Insert at empty buffer
   - Insert at beginning of line
   - Insert at middle of line
   - Insert at end of line
   - Insert multiple characters (typing simulation)

2. **Delete backward tests:**
   - Delete at middle of line
   - Delete at end of line
   - Delete at beginning of line (no-op)
   - Delete joining lines (backspace at line start)

3. **Delete forward tests:**
   - Delete at middle of line
   - Delete at beginning of line
   - Delete at end of line (no-op if buffer end, joins if newline)
   - Delete joining lines

4. **Cursor movement tests:**
   - Move left at buffer start (no-op)
   - Move right at buffer end (no-op)
   - Move up at top line (no-op)
   - Move down at bottom line (no-op)
   - Move up/down with shorter lines (column clamping)
   - Line start/end at empty line
   - Buffer start/end

Location: `src/lib.rs` (in `#[cfg(test)]` module) or `tests/` directory

### Step 8: Unit tests for dirty tracking

For each mutation operation, verify the returned `DirtyLines` is correct:

- `insert_char` returns `Single(current_line)`
- `insert_newline` returns `FromLineToEnd(split_line)`
- `delete_backward` returns correct dirty based on whether lines joined
- `delete_forward` returns correct dirty based on whether lines joined

Location: same test module as Step 7

### Step 9: Integration tests for editing sequences

Test realistic editing patterns:

1. Type a word, then delete it entirely — buffer returns to original state
2. Type multiple lines, navigate between them, edit in the middle
3. Split and rejoin lines — content is preserved
4. Rapid insert/delete cycles (simulating corrections while typing)

These tests verify that the gap buffer and line index stay in sync.

Location: `tests/editing_sequences.rs`

### Step 10: Performance sanity check

Add a benchmark or test that verifies:
- Inserting 100K characters sequentially completes in under 100ms

This isn't a formal benchmark but guards against obvious performance regressions.

```rust
#[test]
fn insert_100k_chars_under_100ms() {
    let mut buffer = TextBuffer::new();
    let start = std::time::Instant::now();
    for _ in 0..100_000 {
        buffer.insert_char('x');
    }
    assert!(start.elapsed() < std::time::Duration::from_millis(100));
}
```

Location: `tests/performance.rs` or in the main test module

## Dependencies

**No chunk dependencies**: Per the investigation, this chunk has no dependencies on other chunks (`depends_on: []` in GOAL.md). It is a standalone data structure with no macOS or rendering dependencies.

**External crates**: None required. Standard library only.

**Infrastructure**: Rust toolchain (stable). The project needs a `Cargo.toml` but no special configuration.

## Risks and Open Questions

1. **char vs u8 for storage**: Using `Vec<char>` is simpler for implementation but uses 4 bytes per character. For a code editor, this may matter for large files. However, the investigation explicitly deferred large-file optimization to a later rope implementation. `Vec<char>` is acceptable for v1.

2. **Line index incremental updates**: The line index must update efficiently when lines are inserted/deleted. If this proves complex, falling back to full rebuild is acceptable for v1 — the buffer is small enough that O(n) rebuilds are still fast.

3. **Unicode grapheme clusters**: A "column" in the cursor position is a `char` index, not a grapheme cluster. This means multi-codepoint characters (emoji, combining marks) may behave unexpectedly. This is a known limitation for v1; proper grapheme handling is a future enhancement.

4. **Line content return type**: Returning `&str` from `line_content` requires the gap buffer to provide contiguous storage for that line. This may require moving the gap or building a temporary string. The implementation should prefer efficiency but may return `String` if simpler.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->