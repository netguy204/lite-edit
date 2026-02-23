# Implementation Plan

## Approach

Add a precomputed line offset index to `SyntaxHighlighter` to make line lookups O(1) instead of O(n). The index is a `Vec<usize>` storing the byte offset where each line starts:
- `line_offsets[0]` = 0 (first line starts at byte 0)
- `line_offsets[n]` = byte offset immediately after the `\n` ending line `n-1`

This is the standard rope/text-editor index structure. Building the index costs ~94µs for a 6K-line file (one-time per parse), and lookups are simple index operations (~0.3ns each).

**Key implementation decisions:**
1. Build the index during `new()` from a single pass over source bytes
2. Rebuild completely in `update_source()` (non-incremental full reparse)
3. Update incrementally in `edit()` — adjust offsets after the edit point by the delta
4. `line_count()` becomes `self.line_offsets.len()`
5. `line_byte_range()` becomes two index lookups with bounds handling

**Testing strategy (per TESTING_PHILOSOPHY.md):**
Following TDD, existing tests (`test_line_byte_range_*`, `test_line_count_*`) should continue to pass. The profiling test (`profile_scroll.rs`) will validate the performance improvement. No new behavioral tests are needed — this is a pure performance optimization that preserves existing behavior.

## Sequence

### Step 1: Add `line_offsets` field to `SyntaxHighlighter`

Add a `line_offsets: Vec<usize>` field to the struct. Each element stores the byte offset where a line starts.

Location: `crates/syntax/src/highlighter.rs`, struct `SyntaxHighlighter`

**Invariants:**
- `line_offsets.len()` == number of lines in source
- `line_offsets[0]` == 0 (always)
- For i > 0: `line_offsets[i]` == byte index immediately after the `\n` that ended line i-1
- `line_offsets` is always in sorted order (strictly increasing)

### Step 2: Implement `build_line_offsets()` helper

Create a private function that builds the offset index from a source string:

```rust
fn build_line_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for (i, b) in source.as_bytes().iter().enumerate() {
        if *b == b'\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}
```

This iterates over bytes (not chars) since byte position is what we need for slicing. O(n) but only runs once per parse.

Location: `crates/syntax/src/highlighter.rs`, module-level private function

### Step 3: Build index in `new()`

Call `build_line_offsets()` during highlighter construction and store the result.

```rust
pub fn new(config: &LanguageConfig, source: &str, theme: SyntaxTheme) -> Option<Self> {
    // ... existing parser setup ...
    let line_offsets = build_line_offsets(source);
    Some(Self {
        // ... existing fields ...
        line_offsets,
    })
}
```

Location: `crates/syntax/src/highlighter.rs`, `SyntaxHighlighter::new()`

### Step 4: Rebuild index in `update_source()`

Since `update_source()` does a full reparse with no edit position information, simply rebuild the entire index:

```rust
pub fn update_source(&mut self, new_source: &str) {
    // ... existing reparse logic ...
    self.line_offsets = build_line_offsets(new_source);
}
```

Location: `crates/syntax/src/highlighter.rs`, `SyntaxHighlighter::update_source()`

### Step 5: Update index incrementally in `edit()`

For incremental edits, update the index efficiently:

1. Find the line containing the edit start byte
2. Calculate the byte delta (new_len - old_len of the edited range)
3. Remove lines that fell within the old range
4. Insert new line offsets for any `\n` in the inserted text
5. Shift all subsequent line offsets by the delta

```rust
pub fn edit(&mut self, event: EditEvent, new_source: &str) {
    // ... existing tree edit and reparse ...

    // Update line offsets incrementally
    let old_start = event.start_byte;
    let old_end = event.old_end_byte;
    let new_end = event.new_end_byte;
    let delta = (new_end as isize) - (old_end as isize);

    // Find first affected line
    let affected_line = self.line_offsets.partition_point(|&off| off <= old_start);

    // Remove lines whose start fell within the deleted range
    self.line_offsets.retain(|&off| off <= old_start || off >= old_end);

    // Insert new line offsets for newlines in the inserted text
    let inserted_text = &new_source[old_start..new_end];
    let new_offsets: Vec<usize> = inserted_text
        .as_bytes()
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b == b'\n' { Some(old_start + i + 1) } else { None })
        .collect();

    // Find insertion point and insert new offsets
    let insert_idx = self.line_offsets.partition_point(|&off| off < old_start + 1);
    for (i, off) in new_offsets.into_iter().enumerate() {
        self.line_offsets.insert(insert_idx + i, off);
    }

    // Shift all offsets after the edit by the delta
    for off in &mut self.line_offsets {
        if *off > old_start {
            *off = ((*off as isize) + delta) as usize;
        }
    }

    self.source = new_source.to_string();
    self.generation = self.generation.wrapping_add(1);
}
```

**Note:** The actual implementation may need adjustment based on `EditEvent` field names (referencing `crate::edit::EditEvent`). The edit.rs module likely has `start_byte`, `old_end_byte`, `new_end_byte` or similar.

Location: `crates/syntax/src/highlighter.rs`, `SyntaxHighlighter::edit()`

### Step 6: Rewrite `line_byte_range()` to use index

Replace the O(n) char scanning with O(1) index lookup:

```rust
fn line_byte_range(&self, line_idx: usize) -> Option<(usize, usize)> {
    if line_idx >= self.line_offsets.len() {
        return None;
    }

    let start = self.line_offsets[line_idx];
    let end = if line_idx + 1 < self.line_offsets.len() {
        // End is one before the start of next line (the \n position)
        self.line_offsets[line_idx + 1] - 1
    } else {
        // Last line extends to end of source
        self.source.len()
    };

    Some((start, end))
}
```

**Edge cases:**
- Empty last line after trailing `\n`: `line_offsets` has an entry for it, end == source.len()
- Single line with no `\n`: `line_offsets` = [0], end == source.len()
- Empty source: `line_offsets` = [0], line 0 returns (0, 0)

Location: `crates/syntax/src/highlighter.rs`, `SyntaxHighlighter::line_byte_range()`

### Step 7: Rewrite `line_count()` to use index length

Replace O(n) char filtering with O(1) length check:

```rust
pub fn line_count(&self) -> usize {
    self.line_offsets.len()
}
```

Location: `crates/syntax/src/highlighter.rs`, `SyntaxHighlighter::line_count()`

### Step 8: Run existing tests

Run the existing test suite to verify correctness:

```bash
cargo test -p lite-edit-syntax
```

Expected: All tests pass, including:
- `test_line_byte_range_first_line`
- `test_line_byte_range_second_line`
- `test_line_byte_range_out_of_bounds`
- `test_line_count_single_line`
- `test_line_count_multiple_lines`
- `test_incremental_edit`

### Step 9: Run profiling test to verify performance

Run the profiling test from the investigation:

```bash
cargo test -p lite-edit-syntax --release --test profile_scroll -- --nocapture
```

Expected: `highlight_viewport` at line 4000 should now take ~1,500µs (down from 6,032µs), with no position-dependent scaling.

### Step 10: Add backreference comment

Add chunk backreference to the line offset index code:

```rust
// Chunk: docs/chunks/highlight_line_offset_index - O(1) line offset index
fn build_line_offsets(source: &str) -> Vec<usize> {
    // ...
}
```

Location: `crates/syntax/src/highlighter.rs`, above `build_line_offsets()`

## Risks and Open Questions

1. **Edit event field names**: The exact field names in `EditEvent` need to be verified. The plan assumes `start_byte`, `old_end_byte`, `new_end_byte` but the actual names may differ.

2. **Trailing newline semantics**: Need to verify the existing tests' expectations for lines ending with `\n`. The current `line_byte_range()` excludes the `\n` from the range (returns up to but not including it). The new implementation must preserve this behavior.

3. **Empty file edge case**: An empty string should return `line_count() == 1` (one empty line) or `0`? Current code returns `1` (`chars().filter() + 1`), so the new code must match.

4. **Incremental edit complexity**: Step 5 is the most complex. If it proves too error-prone, a simpler fallback is to just rebuild the entire index in `edit()` (still O(n) but only during edits, not lookups). This would still achieve the goal since the hot path is `line_byte_range()` during rendering, not `edit()`.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->
