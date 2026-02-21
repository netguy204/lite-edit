---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/selector.rs
  - crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/selector.rs#SelectorOutcome
    implements: "Enum returned by event handlers: Pending, Confirmed(usize), or Cancelled"
  - ref: crates/editor/src/selector.rs#SelectorWidget
    implements: "Core struct managing query, items, and selected_index"
  - ref: crates/editor/src/selector.rs#SelectorWidget::handle_key
    implements: "Keyboard event handling: Up/Down navigation, Enter/Escape, query editing"
  - ref: crates/editor/src/selector.rs#SelectorWidget::handle_mouse
    implements: "Mouse event handling: click to select, click-release to confirm"
  - ref: crates/editor/src/selector.rs#SelectorWidget::set_items
    implements: "Item list replacement with selected_index clamping"
narrative: file_buffer_association
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- delete_to_line_start
- ibeam_cursor
---

# Selector Widget Model

## Minor Goal

Implement a reusable `SelectorWidget` interaction model — a self-contained struct that manages a filterable list of items, a text query the user types into, and a selected index. This is the shared UI primitive that will serve the file picker now and the command palette (and any other type-to-filter UI) later. It knows nothing about files, rendering, or macOS — only about query editing, item selection, and signalling outcomes.

## Success Criteria

- **`SelectorWidget` struct** in a new file (e.g., `crates/editor/src/selector.rs`) with fields:
  - `query: String` — the text the user has typed
  - `items: Vec<String>` — the current list of displayable strings (caller updates this when query changes)
  - `selected_index: usize` — index into `items` of the currently highlighted entry (clamped to `items.len().saturating_sub(1)`)

- **`SelectorOutcome` enum** returned by event handlers:
  ```rust
  pub enum SelectorOutcome {
      Pending,           // still open, no decision yet
      Confirmed(usize),  // user confirmed; value is index into items
      Cancelled,         // user dismissed without selecting
  }
  ```

- **`handle_key(event: &KeyEvent) -> SelectorOutcome`** behaviour:
  - `Up` arrow: decrement `selected_index` (floor at 0), return `Pending`.
  - `Down` arrow: increment `selected_index` (ceil at `items.len() - 1`), return `Pending`.
  - `Return`/`Enter`: return `Confirmed(selected_index)`. If `items` is empty, return `Confirmed(usize::MAX)` as a sentinel — callers treat this as "create with current query".
  - `Escape`: return `Cancelled`.
  - `Backspace` (no modifiers): remove the last character from `query`, return `Pending`.
  - Any printable `Key::Char(ch)` with no command/control modifiers: append `ch` to `query`, reset `selected_index` to 0, return `Pending`.
  - All other keys: return `Pending` (no-op, widget stays open).

- **`handle_mouse(position: (f64, f64), kind: MouseEventKind, item_height: f64, list_origin_y: f64) -> SelectorOutcome`** behaviour:
  - `Down` on a list row: compute `row = ((position.y - list_origin_y) / item_height) as usize`, clamp to valid range, set `selected_index = row`, return `Pending`.
  - `Up` on same row as `selected_index` (i.e., a click-and-release on the same item): return `Confirmed(selected_index)`.
  - `Up` on a different row: set `selected_index` to that row, return `Pending` (no immediate confirm — requires a second click).
  - Outside list bounds: return `Pending`.

- **`set_items(&mut self, items: Vec<String>)`**: replace the item list and clamp `selected_index`.

- **`query(&self) -> &str`** and **`selected_index(&self) -> usize`** accessors.

- **Unit tests** covering:
  - Up/Down navigation wraps at boundaries (no underflow/overflow).
  - Enter with items returns `Confirmed(selected_index)`.
  - Enter with empty items returns `Confirmed(usize::MAX)`.
  - Escape returns `Cancelled`.
  - Typing characters appends to query and resets selected index to 0.
  - Backspace removes last character; Backspace on empty query is a no-op returning `Pending`.
  - `set_items` with fewer items than `selected_index` clamps index.
  - Mouse click on row 2 sets `selected_index = 2` and returns `Pending`.
  - Mouse click-release on already-selected row returns `Confirmed`.
