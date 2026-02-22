<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The buffer is implemented as its own `lite-edit-buffer` crate (no macOS or rendering
dependencies — compiles and tests on any platform) composed of four modules:

1. **`types`** — `Position` and `DirtyLines`; the foundational types everything else uses
2. **`gap_buffer`** — character storage with a movable gap; O(1) amortized insert/delete at the gap position
3. **`line_index`** — a `Vec<usize>` of line-start offsets kept incrementally in sync with every mutation
4. **`text_buffer`** — the public API, combining the three above with cursor and selection state

This layering keeps each concern testable in isolation. `lib.rs` re-exports the three
public types (`TextBuffer`, `DirtyLines`, `Position`) as the crate surface.

The gap buffer was chosen over a rope as the backing store per the investigation
findings (`docs/investigations/editor_core_architecture`): simpler to implement
correctly, sufficient for the initial editable buffer milestone, and the interface
is designed so swapping to a rope later wouldn't change the public API.

## Sequence

### Step 1: Define `Position` and `DirtyLines` in `types.rs`

`Position` is `(line: usize, col: usize)`, 0-indexed. It derives `Ord` (line first,
then col) so selection range endpoints can be sorted without if/else at every call site.

`DirtyLines` is the return type of every `TextBuffer` mutation:

- `None` — no-op or cursor-only operation, nothing to redraw
- `Single(usize)` — exactly one line changed (most in-line edits)
- `Range { from, to }` — a contiguous range changed (e.g., multi-line delete)
- `FromLineToEnd(usize)` — all lines from a point to the end changed (line split or join
  pushes/pulls all subsequent lines)

`DirtyLines::merge` combines two dirty regions into the smallest covering region.
Used by the event loop to accumulate dirty info across multiple events in a single
frame before a single render pass.

Location: `crates/buffer/src/types.rs`

### Step 2: Implement `GapBuffer` in `gap_buffer.rs`

A `Vec<char>` with a movable gap representing the cursor:

```
data: [ pre-gap content | gap (\0 fill) | post-gap content ]
       [0 .. gap_start)   [gap_start..gap_end)   [gap_end..)
```

Key design points:

- `move_gap_to(pos)` shifts content to move the gap; O(distance), amortises well for
  the typical locality-of-edits pattern
- `insert(ch)` writes to `data[gap_start]` and increments `gap_start`; O(1) amortized
- `delete_backward()` / `delete_forward()` shrink/expand the gap by adjusting `gap_start`
  or `gap_end`; O(1)
- `ensure_gap(n)` grows the backing store in-place using `GAP_GROWTH_FACTOR = 2`,
  preserving the gap at its current position (critical: callers call `move_gap_to` then
  `insert` in sequence, so the gap must not move during growth)
- Initial gap size: 64 characters (`INITIAL_GAP_SIZE`)

Location: `crates/buffer/src/gap_buffer.rs`

### Step 3: Implement `LineIndex` in `line_index.rs`

A `Vec<usize>` where `line_starts[i]` is the character offset of the first character
on line `i`. Invariant: `line_starts[0] == 0` always; the vec always has at least one
entry.

Incremental update methods (called by `TextBuffer` on every mutation so the index never
needs a full rebuild during normal editing):

- `insert_newline(offset)` — inserts a new entry after the line containing `offset`,
  shifts all subsequent starts by +1
- `remove_newline(line)` — removes the entry for line `line+1` (joining it into `line`),
  shifts all subsequent starts by -1
- `insert_char(line)` / `remove_char(line)` — shifts all starts after `line` by ±1

Bulk variants for `insert_str` (avoids O(n·m) per-character shifting):

- `line_starts_after_mut(after_line)` — mutable slice of all entries strictly after
  `after_line`, for bulk offset adjustment
- `insert_line_starts_after(after_line, new_starts)` — splices multiple entries in one
  `copy_within` + slice-copy operation

`rebuild(content)` does a full O(n) pass from scratch, used only when loading a file.

Location: `crates/buffer/src/line_index.rs`

### Step 4: Implement `TextBuffer` in `text_buffer.rs`

Combines `GapBuffer` + `LineIndex` + `cursor: Position` + `selection_anchor: Option<Position>`.

**Core helper — `sync_gap_to_cursor()`:** Converts `(line, col)` to a logical character
offset using the line index and calls `buffer.move_gap_to(offset)`. Called at the top
of every mutation to ensure the gap is in the right place before touching the buffer.

**Mutation operations** (all return `DirtyLines`):

- `insert_char(ch)` — inserts at cursor, updates line index via `insert_char` or
  `insert_newline`, advances cursor; returns `Single(line)` for normal chars,
  `FromLineToEnd(line)` for `\n`
- `insert_newline()` — splits the current line at the cursor; returns `FromLineToEnd(line)`
- `delete_backward()` — deletes the character before the cursor; if that character is
  `\n` the lines join (`remove_newline`) and returns `FromLineToEnd`; no-op at buffer
  start
- `delete_forward()` — deletes the character at the cursor; same line-join logic;
  no-op at buffer end
- `insert_str(s)` — bulk insert, uses `line_starts_after_mut` + `insert_line_starts_after`
  for efficient line-index update
- `delete_selection()` — deletes the text between anchor and cursor, places cursor at
  the selection start, clears the anchor

**Cursor movement** (return `()`):

- `move_left` / `move_right` with wrap at line boundaries
- `move_up` / `move_down` with column clamping at shorter lines
- `move_to_line_start` / `move_to_line_end`
- `move_to_buffer_start` / `move_to_buffer_end`
- `set_cursor(pos)` — direct placement with bounds clamping
- `move_word_left` / `move_word_right` — character-class run scanning using
  `word_boundary_left` / `word_boundary_right` (private helpers, also used by the
  word-deletion operations added in later chunks)

**Selection:**

- Anchor-cursor model: `selection_anchor: Option<Position>` stores one end; the cursor
  is the other
- `set_selection_anchor(pos)` / `clear_selection()` / `has_selection()`
- `selection_range()` — returns `(min, max)` regardless of which end is anchor vs cursor
- `select_all()`, `select_word_at(col)` — convenience setters

**Debug consistency check:** `assert_line_index_consistent()` — sampled every 64
mutations in debug builds; does a full rebuild into a scratch index and panics if it
disagrees with the live index. Catches any off-by-one in the incremental update code.

Location: `crates/buffer/src/text_buffer.rs`

### Step 5: Wire up `lib.rs`

Re-export `TextBuffer`, `DirtyLines`, and `Position` as the crate's public surface.
Add module declarations for the four internal modules. Include a doc-comment crate
overview with a usage example.

Location: `crates/buffer/src/lib.rs`

### Step 6: Write unit tests

Tests are co-located in each module's `#[cfg(test)]` block:

- **`gap_buffer.rs`** — new/empty, `from_str`, insert, insert at middle, delete backward/forward
  at boundaries, `move_gap`, `char_at` with gap in middle, `slice`, `insert_str`, large insert
- **`line_index.rs`** — rebuild (empty, single-line, multi-line), `line_end`, `line_len`,
  `line_at_offset`, `insert_newline`, `remove_newline`, `insert_char`, `remove_char`
- **`types.rs`** — `DirtyLines::merge` exhaustively: identity (None), same/adjacent/distant
  singles, overlapping/disjoint/nested ranges, single+range, `FromLineToEnd` absorption,
  multi-event sequences
- **`text_buffer.rs`** — 169 tests covering insert/delete at beginning/middle/end,
  newline insertion, backspace across line boundaries, cursor movement at all boundaries,
  multi-character sequences, dirty line correctness, selection operations, word movement,
  word deletion, kill-line, paste, and the debug line-index consistency assertion

Integration tests in `crates/buffer/tests/` cover multi-operation sequences and
performance (100K character insert in < 100ms release, < 200ms with newlines).

## Deviations

None — implementation matched the design described above. The `word_boundary_left` and
`word_boundary_right` helpers, and the selection/word-operation methods, were added in
later chunks (`word_boundary_primitives`, `text_selection_model`, `delete_backward_word`,
`word_forward_delete`) but live in this file because `TextBuffer` is the natural home
for buffer-level operations. Those additions are governed by their respective chunks,
not this one.
