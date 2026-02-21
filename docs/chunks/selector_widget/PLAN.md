<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Implement a self-contained `SelectorWidget` struct in a new `crates/editor/src/selector.rs` module. This widget is a **pure interaction model** with no filesystem, rendering, or macOS dependencies—only query state, item management, and selection logic. It follows the project's "Humble View Architecture" pattern: the widget is testable Rust state that downstream code (the renderer, the file picker) will consume.

The widget will:
1. Own a `query: String` that the user edits
2. Own a `Vec<String>` of display items (updated externally when the query changes)
3. Track a `selected_index: usize` clamped to valid bounds
4. Expose `handle_key()` and `handle_mouse()` methods returning a `SelectorOutcome` enum

Following the project's TDD approach per `docs/trunk/TESTING_PHILOSOPHY.md`, we'll write failing tests first for each behavior, then implement the minimum code to pass them.

The widget intentionally does **not** implement the `FocusTarget` trait—it returns an outcome enum rather than mutating `EditorContext`. The file_picker chunk (later) will wrap this in a focus target that interprets outcomes and mutates editor state.

## Subsystem Considerations

No existing subsystems are relevant to this chunk. This is a new, self-contained interaction model that may seed a future "selector" or "overlay" subsystem if the pattern proves reusable across command palette, buffer switcher, etc.

## Sequence

### Step 1: Create selector.rs with SelectorOutcome enum and basic tests

Create `crates/editor/src/selector.rs` with:

1. The `SelectorOutcome` enum:
   ```rust
   pub enum SelectorOutcome {
       Pending,           // still open, no decision yet
       Confirmed(usize),  // user confirmed; value is index into items
       Cancelled,         // user dismissed without selecting
   }
   ```

2. A minimal `SelectorWidget` struct scaffold with fields:
   - `query: String`
   - `items: Vec<String>`
   - `selected_index: usize`

3. TDD: Write failing tests for basic accessors:
   - `query()` returns the query string
   - `selected_index()` returns the current index
   - A new widget starts with empty query and index 0

Register the module in `main.rs`.

### Step 2: Implement set_items with clamping tests

TDD approach:
1. Write tests:
   - `set_items` with 5 items keeps `selected_index` if in range
   - `set_items` with fewer items than current `selected_index` clamps to `len - 1`
   - `set_items` with empty list clamps `selected_index` to 0

2. Implement `set_items(&mut self, items: Vec<String>)`:
   - Replace `self.items`
   - Clamp `selected_index` to `items.len().saturating_sub(1)`

### Step 3: Implement keyboard navigation (Up/Down)

TDD approach:
1. Write tests:
   - Down from index 0 with 5 items → index 1, returns `Pending`
   - Down from index 4 (last) with 5 items → stays at 4 (no wrap), returns `Pending`
   - Up from index 2 → index 1, returns `Pending`
   - Up from index 0 → stays at 0 (floor), returns `Pending`

2. Implement `handle_key(&mut self, event: &KeyEvent) -> SelectorOutcome`:
   - Match on `event.key`:
     - `Key::Up` → decrement `selected_index` (saturating), return `Pending`
     - `Key::Down` → increment `selected_index` (ceil at `items.len() - 1`), return `Pending`

### Step 4: Implement Enter/Escape handling

TDD approach:
1. Write tests:
   - Enter with items returns `Confirmed(selected_index)`
   - Enter with empty items returns `Confirmed(usize::MAX)` (sentinel)
   - Escape returns `Cancelled`

2. Extend `handle_key`:
   - `Key::Return` → return `Confirmed(selected_index)` if items non-empty, else `Confirmed(usize::MAX)`
   - `Key::Escape` → return `Cancelled`

### Step 5: Implement query editing (character input and backspace)

TDD approach:
1. Write tests:
   - Typing 'a' appends to query, resets selected_index to 0, returns `Pending`
   - Typing multiple characters builds the query
   - Backspace removes last character, returns `Pending`
   - Backspace on empty query is no-op, returns `Pending`
   - Typing with command/control modifiers is a no-op (returns `Pending` without modifying query)

2. Extend `handle_key`:
   - `Key::Backspace` (no command/control) → pop last char from query, return `Pending`
   - `Key::Char(ch)` with no command/control modifiers → append `ch` to query, reset `selected_index` to 0, return `Pending`
   - All other keys → return `Pending` (no-op)

### Step 6: Implement mouse handling (click to select, click-release to confirm)

TDD approach:
1. Write tests:
   - Mouse Down on row 2 (within bounds) sets `selected_index = 2`, returns `Pending`
   - Mouse Down outside list bounds returns `Pending` without changing selection
   - Mouse Up on same row as current `selected_index` returns `Confirmed(selected_index)`
   - Mouse Up on a different row updates `selected_index` to that row, returns `Pending`
   - Mouse Moved events return `Pending` (no-op)

2. Implement `handle_mouse(&mut self, position: (f64, f64), kind: MouseEventKind, item_height: f64, list_origin_y: f64) -> SelectorOutcome`:
   - Compute `row = ((position.y - list_origin_y) / item_height) as usize`
   - Clamp row to valid range (0..items.len())
   - If position.y < list_origin_y or row >= items.len(), return `Pending` (out of bounds)
   - Match on `kind`:
     - `MouseEventKind::Down` → set `selected_index = row`, return `Pending`
     - `MouseEventKind::Up` → if `row == selected_index`, return `Confirmed(selected_index)`; else set `selected_index = row`, return `Pending`
     - `MouseEventKind::Moved` → return `Pending`

### Step 7: Add Constructor and Default Implementation

1. Add `SelectorWidget::new()` constructor that initializes with empty query, empty items, and index 0
2. Implement `Default` for `SelectorWidget`
3. Add doc comments to all public items

---

**BACKREFERENCE COMMENTS**

Add at module level in `selector.rs`:
```rust
// Chunk: docs/chunks/selector_widget - Reusable selector interaction model
```

## Dependencies

- **crates/editor/src/input.rs**: Uses `KeyEvent`, `Key`, `Modifiers`, and `MouseEventKind` types. These already exist.
- No external crates required beyond what's already in the project.

## Risks and Open Questions

1. **Mouse click semantics**: The GOAL specifies "double-clicking (or a single click if it matches the current selection) confirms." We've interpreted this as: first click selects, second click on same row confirms. This matches the success criteria description more closely. If actual double-click detection is needed, it would require timing state, which we're avoiding for simplicity.

2. **Empty items edge case**: When `items` is empty, Enter returns `Confirmed(usize::MAX)` as a sentinel. Callers (file_picker) must handle this case to trigger "create new file" behavior.

3. **Character filtering**: The GOAL says "any printable `Key::Char(ch)` with no command/control modifiers." We'll use `char::is_control()` to filter out control characters, allowing Unicode printable chars.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->