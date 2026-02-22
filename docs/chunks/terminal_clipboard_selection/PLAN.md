# Implementation Plan

## Approach

This chunk brings selection and clipboard operations to `TerminalBuffer`/`TerminalFocusTarget`, mirroring the text editor's existing selection model but adapted for the terminal's read-only grid. The approach follows the established patterns from the dependent chunks:

1. **Selection state in TerminalBuffer**: Add `selection_anchor` and `selection_head` fields plus selection methods to `TerminalBuffer`, similar to `TextBuffer`. The selection is purely visual — it doesn't modify terminal content. Override `BufferView::selection_range()` to expose selection to the renderer.

2. **Mouse handling in TerminalFocusTarget**: Extend `handle_mouse()` to handle selection when no mouse mode is active. On click-and-drag, set anchor and extend selection. On double-click, select the word at the click position.

3. **Clipboard integration**: Use the existing `crates/editor/src/clipboard.rs` module for clipboard operations. `Cmd+C` copies selected text, `Cmd+V` pastes to PTY via `write_paste()`.

4. **Selection rendering**: The renderer already supports `selection_range()` from `BufferView` — we just need to return the correct range from `TerminalBuffer`. Selection highlighting will "just work" through the existing `glyph_buffer.rs` rendering pipeline.

5. **Selection clearing on output**: Hook into `poll_events()` to clear selection when new PTY output arrives.

**Key architectural decisions:**
- Selection coordinates are in terminal grid positions (line, column), not buffer byte offsets
- Selection state lives on `TerminalBuffer` since it's the source of truth for grid content
- Word boundaries use simple alphanumeric/whitespace heuristics (not `TextBuffer`'s char-class model since we operate on rendered cell content)
- Follow TDD per `docs/trunk/TESTING_PHILOSOPHY.md` — write failing tests first, then implementation

## Sequence

### Step 1: Add selection state to TerminalBuffer

Add selection anchor and head fields plus basic selection methods to `TerminalBuffer`:

```rust
// In crates/terminal/src/terminal_buffer.rs
pub struct TerminalBuffer {
    // ... existing fields ...
    selection_anchor: Option<Position>,
    selection_head: Option<Position>,
}

impl TerminalBuffer {
    pub fn set_selection_anchor(&mut self, pos: Position);
    pub fn set_selection_head(&mut self, pos: Position);
    pub fn clear_selection(&mut self);
    pub fn selection_anchor(&self) -> Option<Position>;
    pub fn selection_head(&self) -> Option<Position>;
}
```

Location: `crates/terminal/src/terminal_buffer.rs`

**Tests (write first):**
- `test_selection_anchor_initially_none`
- `test_set_selection_anchor`
- `test_clear_selection`

### Step 2: Implement selection_range() for TerminalBuffer

Override `BufferView::selection_range()` to return the selection range when both anchor and head are set.

```rust
impl BufferView for TerminalBuffer {
    fn selection_range(&self) -> Option<(Position, Position)> {
        let anchor = self.selection_anchor?;
        let head = self.selection_head?;
        if anchor == head {
            return None;
        }
        // Return in document order (start, end)
        if anchor < head {
            Some((anchor, head))
        } else {
            Some((head, anchor))
        }
    }
}
```

Location: `crates/terminal/src/terminal_buffer.rs`

**Tests (write first):**
- `test_selection_range_none_when_no_anchor`
- `test_selection_range_none_when_anchor_equals_head`
- `test_selection_range_forward`
- `test_selection_range_backward_returns_ordered`

### Step 3: Implement selected_text() for TerminalBuffer

Add method to extract selected text from the terminal grid. Join rows with newlines. Handle wide characters and trailing spaces appropriately.

```rust
impl TerminalBuffer {
    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        // Extract text from grid lines start.line..=end.line
        // Trim trailing spaces per line (standard terminal behavior)
    }
}
```

Location: `crates/terminal/src/terminal_buffer.rs`

**Tests (write first):**
- `test_selected_text_single_line`
- `test_selected_text_multiline`
- `test_selected_text_trims_trailing_spaces`
- `test_selected_text_none_when_no_selection`

### Step 4: Add selection clearing on PTY output

Clear selection when new output arrives from the PTY to prevent stale/misaligned highlights.

```rust
// In poll_events(), after processing PtyOutput:
if processed_any {
    self.clear_selection();
    // ... existing damage tracking ...
}
```

Location: `crates/terminal/src/terminal_buffer.rs`

**Tests (write first):**
- `test_selection_cleared_on_pty_output`

### Step 5: Implement click-and-drag selection in TerminalFocusTarget

Extend `handle_mouse()` to handle selection when no mouse mode is active:

```rust
pub fn handle_mouse(&mut self, event: MouseEvent, view_origin: (f32, f32)) -> bool {
    let modes = self.terminal.borrow().term_mode();

    // If mouse mode is active, forward to PTY (existing behavior)
    if modes.intersects(TermMode::MOUSE_REPORT_CLICK | ...) {
        // existing encoding logic
    }

    // Otherwise, handle selection
    let (col, row) = self.pixel_to_cell(event.position, view_origin);
    let pos = Position::new(row, col);

    match event.kind {
        MouseEventKind::Down => {
            if event.click_count == 2 {
                self.select_word_at(pos);
            } else {
                self.terminal.borrow_mut().set_selection_anchor(pos);
                self.terminal.borrow_mut().set_selection_head(pos);
            }
            true
        }
        MouseEventKind::Moved => {
            // Only extend selection if we have an anchor
            if self.terminal.borrow().selection_anchor().is_some() {
                self.terminal.borrow_mut().set_selection_head(pos);
                true
            } else {
                false
            }
        }
        MouseEventKind::Up => {
            // Finalize selection - if anchor == head, clear selection
            let terminal = self.terminal.borrow();
            if terminal.selection_anchor() == terminal.selection_head() {
                drop(terminal);
                self.terminal.borrow_mut().clear_selection();
            }
            true
        }
    }
}
```

Location: `crates/terminal/src/terminal_target.rs`

**Tests (write first):**
- `test_click_sets_anchor`
- `test_drag_extends_selection`
- `test_click_without_drag_clears_selection`
- `test_mouse_events_ignored_when_mouse_mode_active`

### Step 6: Implement double-click word selection

Add word selection logic using simple word boundary detection on terminal cell content.

```rust
impl TerminalFocusTarget {
    fn select_word_at(&mut self, pos: Position) {
        let terminal = self.terminal.borrow();
        let line_content = self.get_line_chars(pos.line);
        // Find word boundaries using alphanumeric/whitespace classification
        // Set selection anchor at word start, head at word end
    }
}
```

Location: `crates/terminal/src/terminal_target.rs`

**Tests (write first):**
- `test_double_click_selects_word`
- `test_double_click_on_whitespace_selects_whitespace`
- `test_double_click_on_empty_line_no_selection`

### Step 7: Implement Cmd+C copy

Handle `Cmd+C` in `handle_key()` — if selection exists, copy to clipboard; otherwise no-op.

```rust
// In handle_key():
if event.modifiers.command {
    match event.key {
        Key::Char('c') | Key::Char('C') => {
            if let Some(text) = self.terminal.borrow().selected_text() {
                clipboard::copy_to_clipboard(&text);
                // Optionally clear selection after copy
                self.terminal.borrow_mut().clear_selection();
            }
            return true;
        }
        // ... existing Cmd+V handling ...
    }
}
```

Location: `crates/terminal/src/terminal_target.rs`

Note: Need to add dependency on `crates/editor` for clipboard module, or move clipboard to a shared crate.

**Tests (write first):**
- `test_cmd_c_with_selection_copies_to_clipboard`
- `test_cmd_c_without_selection_is_noop`

### Step 8: Implement Cmd+V paste

The existing `write_paste()` method already handles bracketed paste. Just need to wire up Cmd+V to read from clipboard and call it.

```rust
// In handle_key():
Key::Char('v') | Key::Char('V') => {
    if let Some(text) = clipboard::paste_from_clipboard() {
        self.write_paste(&text);
    }
    return true;
}
```

Location: `crates/terminal/src/terminal_target.rs`

**Tests (write first):**
- `test_cmd_v_pastes_clipboard_to_pty`
- `test_cmd_v_with_empty_clipboard_is_noop`

### Step 9: Wire up selection to dirty region tracking

Mark selection-affected lines as dirty when selection changes.

```rust
impl TerminalBuffer {
    pub fn set_selection_head(&mut self, pos: Position) {
        let old_head = self.selection_head;
        self.selection_head = Some(pos);

        // Mark dirty: old selection range + new selection range
        if let Some(old) = old_head {
            self.dirty.merge(DirtyLines::line_range(
                old.line.min(pos.line),
                old.line.max(pos.line) + 1,
            ));
        }
        if let Some(anchor) = self.selection_anchor {
            self.dirty.merge(DirtyLines::line_range(
                anchor.line.min(pos.line),
                anchor.line.max(pos.line) + 1,
            ));
        }
    }
}
```

Location: `crates/terminal/src/terminal_buffer.rs`

**Tests (write first):**
- `test_selection_change_marks_lines_dirty`

### Step 10: Integration tests

Write integration tests that exercise the full flow:

```rust
#[test]
fn test_terminal_selection_copy_paste_roundtrip() {
    // Create terminal, simulate output "hello world"
    // Simulate click-drag to select "world"
    // Verify selection_range() returns correct range
    // Simulate Cmd+C
    // Verify clipboard contains "world"
}

#[test]
fn test_terminal_paste_sends_to_pty() {
    // Create terminal with PTY
    // Set clipboard to "test input"
    // Simulate Cmd+V
    // Verify PTY received "test input" (possibly with bracketed paste sequences)
}
```

Location: `crates/terminal/tests/selection_integration.rs`

## Dependencies

**Chunk dependencies** (all ACTIVE, already implemented):
- `terminal_emulator` — provides `TerminalBuffer`, `BufferView` impl, PTY management
- `clipboard_operations` — provides `clipboard::copy_to_clipboard()`, `clipboard::paste_from_clipboard()`
- `mouse_drag_selection` — establishes the pattern for `MouseEventKind::Down/Moved/Up` handling
- `word_double_click_select` — establishes the pattern for `click_count` handling

**Crate dependencies:**
- Need to expose clipboard functions to `crates/terminal`. Options:
  1. Move clipboard module to `crates/editor/src/clipboard.rs` → `crates/shared/src/clipboard.rs`
  2. Make `crates/terminal` depend on `crates/editor` (not ideal, creates cycle risk)
  3. Re-export clipboard functions via `lite_edit_input` or similar shared crate

  Recommendation: Option 1 — create a small `crates/clipboard` crate that both editor and terminal can depend on.

## Risks and Open Questions

1. **Clipboard crate location**: The clipboard module is currently in `crates/editor`. Moving it to a shared location requires minor refactoring. Low risk — the module is self-contained.

2. **Word boundary detection**: The text editor uses a sophisticated char-class model. For terminal, we'll use simpler alphanumeric/whitespace classification since we're operating on rendered cell content, not source code. This may behave slightly differently than the editor for edge cases.

3. **Wide character handling in selection**: CJK characters and emoji occupy 2 cells. Selection coordinates need to account for this when extracting text. `alacritty_terminal` provides `Cell::flags().contains(Flags::WIDE_CHAR)` for detection.

4. **Selection rendering with scrollback**: When selection spans both cold scrollback and hot scrollback, the coordinates must be correctly mapped. The existing `styled_line()` logic handles this mapping, so selection rendering should work correctly.

5. **Selection vs mouse mode conflict**: TUI apps that enable mouse mode should receive mouse events instead of selection. The existing `handle_mouse()` check for `TermMode::MOUSE_*` flags handles this correctly.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->
