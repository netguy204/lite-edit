# Implementation Plan

## Approach

The current architecture has all the pieces for incremental parsing, but they aren't wired together. The `SyntaxHighlighter::edit()` method exists and accepts an `EditEvent`, but it's never called. Instead, all buffer mutations flow through `sync_active_tab_highlighter()` which calls `SyntaxHighlighter::update_source()` (full reparse, passing `None` as the old tree).

The strategy is:

1. **Add byte-offset return values to `TextBuffer` mutations**: Currently, mutation methods like `insert_char`, `insert_str`, `delete_backward`, etc. return `DirtyLines` (which lines changed for rendering). We need them to also return the byte-offset information required to construct an `EditEvent`. This will be done by introducing a new return type `MutationResult` that bundles `DirtyLines` with optional `EditEvent` data.

2. **Wire mutation sites in `editor_state.rs` to call `Tab::notify_edit()`**: The investigation identified five mutation sites:
   - `handle_key_buffer()` → routes through `BufferFocusTarget.handle_key()`
   - `handle_insert_text()` → direct `buffer.insert_str()`
   - `handle_set_marked_text()` → `buffer.set_marked_text()`
   - `handle_unmark_text()` → `buffer.cancel_marked_text()`
   - File-drop text insertion (same path as `handle_insert_text`)

3. **Preserve `sync_highlighter()` for initial file load**: The full-reparse path should remain for initial file open (where there's no previous tree) and potentially for file reload scenarios.

This approach minimizes the surface area of changes by:
- Keeping the `DirtyLines` flow for rendering unchanged
- Adding `EditEvent` data as a companion to existing return values
- Changing only the wiring in `editor_state.rs`, not the buffer mutation logic itself

## Subsystem Considerations

This chunk does not directly implement any existing subsystems. It touches the syntax highlighting infrastructure but does not change the rendering subsystem's patterns.

## Sequence

### Step 1: Define `MutationResult` type in `crates/buffer`

Create a new type that bundles `DirtyLines` with the byte-offset information needed for incremental parsing:

```rust
// Chunk: docs/chunks/incremental_parse - Mutation result with edit event data
/// Result of a buffer mutation, containing both rendering and parsing info.
pub struct MutationResult {
    /// Which lines need re-rendering
    pub dirty_lines: DirtyLines,
    /// Edit event for incremental parsing (None if no text was actually changed)
    pub edit_info: Option<EditInfo>,
}

/// Byte-offset information for a buffer edit.
///
/// This provides everything needed to construct a tree-sitter `InputEdit`:
/// - Byte offsets: where the edit happened in the byte stream
/// - Row/col positions: where the edit happened in line/column coordinates
pub struct EditInfo {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start_row: usize,
    pub start_col: usize,
    pub old_end_row: usize,
    pub old_end_col: usize,
    pub new_end_row: usize,
    pub new_end_col: usize,
}
```

**Location**: `crates/buffer/src/types.rs`

**Tests**: Add unit tests verifying `MutationResult` construction and field access.

### Step 2: Add `position_to_byte_offset` as a public method

Currently `TextBuffer::position_to_offset` is private. We need byte offset calculation to be accessible for constructing `EditInfo`. Rather than making the private method public, add a dedicated public API:

```rust
/// Returns the byte offset for a (line, col) position.
///
/// This is useful for incremental parsing where tree-sitter needs byte offsets.
/// Returns the buffer length if the position is past the end.
pub fn byte_offset_at(&self, line: usize, col: usize) -> usize {
    self.position_to_offset(Position::new(line, col))
}
```

**Location**: `crates/buffer/src/text_buffer.rs`

**Tests**: Add tests for byte offset calculation with ASCII, multi-byte UTF-8, and edge cases.

### Step 3: Add `byte_len` method to `TextBuffer`

Add a method to get the total byte length of the buffer:

```rust
/// Returns the total byte length of the buffer content.
pub fn byte_len(&self) -> usize {
    self.buffer.byte_len()  // GapBuffer needs this method
}
```

This requires also adding `byte_len()` to `GapBuffer`.

**Location**: `crates/buffer/src/text_buffer.rs`, `crates/buffer/src/gap_buffer.rs`

**Tests**: Add tests verifying byte length matches `content().len()`.

### Step 4: Create helper to convert `DirtyLines` + position info to `MutationResult`

Add a helper method to `TextBuffer` that captures pre-mutation state and constructs a `MutationResult`:

```rust
/// Captures the current cursor position's byte offset for edit tracking.
///
/// Call this before a mutation to get the start position for EditInfo.
fn capture_edit_start(&self) -> (usize, usize, usize) {
    let pos = self.cursor;
    let byte = self.position_to_offset(pos);
    (byte, pos.line, pos.col)
}

/// Creates an EditInfo for an insertion at the given start position.
fn make_insert_info(
    &self,
    start_byte: usize,
    start_row: usize,
    start_col: usize,
    inserted_bytes: usize,
    final_row: usize,
    final_col: usize,
) -> EditInfo {
    EditInfo {
        start_byte,
        old_end_byte: start_byte,
        new_end_byte: start_byte + inserted_bytes,
        start_row,
        start_col,
        old_end_row: start_row,
        old_end_col: start_col,
        new_end_row: final_row,
        new_end_col: final_col,
    }
}

/// Creates an EditInfo for a deletion ending at the current position.
fn make_delete_info(
    &self,
    start_byte: usize,
    start_row: usize,
    start_col: usize,
    deleted_bytes: usize,
) -> EditInfo {
    EditInfo {
        start_byte,
        old_end_byte: start_byte + deleted_bytes,
        new_end_byte: start_byte,
        start_row,
        start_col,
        old_end_row: /* computed from deleted content */,
        old_end_col: /* computed from deleted content */,
        new_end_row: start_row,
        new_end_col: start_col,
    }
}
```

**Location**: `crates/buffer/src/text_buffer.rs`

### Step 5: Add `MutationResult`-returning variants to key mutation methods

For each of the primary mutation methods, add a version that returns `MutationResult`:

**insert_char → insert_char_tracked**:
```rust
/// Inserts a character at the cursor position and returns edit info for incremental parsing.
pub fn insert_char_tracked(&mut self, ch: char) -> MutationResult {
    let (start_byte, start_row, start_col) = self.capture_edit_start();
    let dirty = self.insert_char(ch);
    let inserted_bytes = ch.len_utf8();
    let (end_row, end_col) = (self.cursor.line, self.cursor.col);
    MutationResult {
        dirty_lines: dirty,
        edit_info: Some(self.make_insert_info(
            start_byte, start_row, start_col,
            inserted_bytes, end_row, end_col
        )),
    }
}
```

Similarly for:
- `insert_str_tracked`
- `delete_backward_tracked`
- `delete_forward_tracked`
- `delete_selection_tracked`
- `delete_backward_word_tracked`
- `delete_forward_word_tracked`

**Alternative approach**: Instead of `_tracked` variants, modify the existing methods to return `MutationResult` directly. This is a larger change but cleaner. The `DirtyLines` can be extracted from the result for existing callers.

**Decision**: Use the `_tracked` variant approach initially for lower risk. This keeps backward compatibility and allows incremental rollout.

**Location**: `crates/buffer/src/text_buffer.rs`

**Tests**: Add tests verifying that `EditInfo` has correct byte offsets for:
- Single ASCII character insertion
- Multi-byte UTF-8 character insertion
- Newline insertion (line split)
- Backspace at column 0 (line join)
- Selection delete (multi-character, multi-line)
- String paste with newlines

### Step 6: Add conversion from `EditInfo` to `EditEvent`

In `crates/syntax/src/edit.rs`, add a `From` impl or constructor:

```rust
impl From<lite_edit_buffer::EditInfo> for EditEvent {
    fn from(info: lite_edit_buffer::EditInfo) -> Self {
        EditEvent {
            start_byte: info.start_byte,
            old_end_byte: info.old_end_byte,
            new_end_byte: info.new_end_byte,
            start_row: info.start_row,
            start_col: info.start_col,
            old_end_row: info.old_end_row,
            old_end_col: info.old_end_col,
            new_end_row: info.new_end_row,
            new_end_col: info.new_end_col,
        }
    }
}
```

**Location**: `crates/syntax/src/edit.rs`

**Tests**: Verify round-trip conversion produces correct `tree_sitter::InputEdit`.

### Step 7: Modify `BufferFocusTarget` to capture edit info

The `handle_key` method in `buffer_target.rs` needs to capture `EditInfo` from mutations and expose it via `EditorContext`:

Add a field to `EditorContext`:
```rust
pub struct EditorContext<'a> {
    // existing fields...
    /// Edit info for incremental syntax parsing (set by mutation commands)
    pub edit_info: Option<lite_edit_buffer::EditInfo>,
}
```

Update mutation commands in `BufferFocusTarget` to use `_tracked` variants and set `ctx.edit_info`.

**Location**: `crates/editor/src/context.rs`, `crates/editor/src/buffer_target.rs`

**Tests**: Add tests that mutation commands through EditorContext produce correct edit_info.

### Step 8: Wire `handle_key_buffer` to use incremental path

In `editor_state.rs`, modify `handle_key_buffer` to call `Tab::notify_edit()` when `ctx.edit_info` is present:

```rust
// After handling the key event through focus target...
if needs_highlighter_sync {
    if let Some(edit_info) = ctx.edit_info.take() {
        // Use incremental path
        if let Some(ws) = self.editor.active_workspace_mut() {
            if let Some(tab) = ws.active_tab_mut() {
                let event = lite_edit_syntax::EditEvent::from(edit_info);
                tab.notify_edit(event);
            }
        }
    } else {
        // Fallback to full reparse (e.g., no mutation happened)
        self.sync_active_tab_highlighter();
    }
}
```

**Location**: `crates/editor/src/editor_state.rs`

**Tests**: Integration test that types characters and verifies highlighting still works.

### Step 9: Wire `handle_insert_text` to use incremental path

Modify `handle_insert_text` to use `insert_str_tracked` and call `notify_edit`:

```rust
// In the Buffer focus branch...
if let Some((buffer, viewport)) = tab.buffer_and_viewport_mut() {
    buffer.clear_marked_text();

    let result = buffer.insert_str_tracked(text);
    self.dirty_lines.merge(result.dirty_lines.clone());
    let dirty = viewport.dirty_lines_to_region(&result.dirty_lines, buffer.line_count());
    self.invalidation.merge(InvalidationKind::Content(dirty));

    // ... cursor visibility handling ...

    tab.dirty = true;

    // Incremental highlighting
    if let Some(edit_info) = result.edit_info {
        let event = lite_edit_syntax::EditEvent::from(edit_info);
        tab.notify_edit(event);
    }
}
```

Remove the subsequent `sync_active_tab_highlighter()` call.

**Location**: `crates/editor/src/editor_state.rs`

### Step 10: Wire `handle_set_marked_text` and `handle_unmark_text`

These are trickier because marked text is overlay-rendered, not actually inserted into the buffer until commit. The current `set_marked_text` doesn't modify buffer content, so there's no edit to report to tree-sitter.

For marked text, the highlighter should ignore the marked portion (it's not committed). The current `sync_highlighter()` call is unnecessary during composition. Only when marked text is **committed** (via `handle_insert_text`) do we need to update the tree.

**Change**: Remove the `sync_active_tab_highlighter()` calls from `handle_set_marked_text` and `handle_unmark_text`. The syntax tree doesn't need to reflect uncommitted IME composition.

**Location**: `crates/editor/src/editor_state.rs`

### Step 11: Verify and remove full-reparse from mutation paths

Search for remaining calls to `sync_active_tab_highlighter()` in mutation contexts and ensure they're all converted to the incremental path or are appropriate (initial file load).

Audit:
- [x] `handle_key_buffer` → converted to incremental
- [x] `handle_insert_text` → converted to incremental
- [x] `handle_set_marked_text` → removed (no buffer change)
- [x] `handle_unmark_text` → removed (no buffer change)
- [ ] File open/reload → keep full reparse (via `sync_highlighter()`)
- [ ] File-drop paste → same path as `handle_insert_text`

**Location**: `crates/editor/src/editor_state.rs`

### Step 12: Add performance verification

Add a benchmark or test that measures parse time on a large file (~5000 lines) to verify incremental parsing is indeed faster:

```rust
#[test]
fn incremental_parse_faster_than_full_reparse() {
    // Setup: Load a large Rust file
    // Measure: Time for full reparse via update_source()
    // Measure: Time for incremental via edit()
    // Assert: Incremental is significantly faster (10x or more)
}
```

**Location**: `crates/syntax/tests/` or as a benchmark in `crates/syntax/benches/`

## Dependencies

- **No chunk dependencies**: This chunk has no dependencies on other chunks (explicitly declared in GOAL.md: `depends_on: []`).
- **No external library additions needed**: tree-sitter already supports incremental parsing; we're just wiring it up.

## Risks and Open Questions

1. **Byte offset accuracy for complex edits**: Selection deletion spanning multiple lines requires computing the byte length of the deleted content. The current `delete_selection` implementation deletes character-by-character, which loses byte-length information. May need to capture the selected byte range before deletion.

2. **UTF-8 byte vs character indexing**: Tree-sitter uses byte offsets, but `TextBuffer` uses character columns. The existing `position_to_byte_offset` helper in `crates/syntax/src/edit.rs` handles this, but we need to verify it handles all edge cases (grapheme clusters, combining characters).

3. **Selection-replacement edits**: When inserting with an active selection (common for paste-over-selection), the edit is conceptually delete-then-insert. Tree-sitter expects a single `InputEdit` for this. Need to verify the byte offsets account for this correctly.

4. **IME marked text edge cases**: The marked text overlay approach means tree-sitter doesn't see IME composition. This should be fine (the tree reflects committed content only), but needs verification with actual IME usage.

## Deviations

<!-- Populated during implementation -->
