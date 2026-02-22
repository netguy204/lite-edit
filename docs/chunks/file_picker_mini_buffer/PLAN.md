# Implementation Plan

## Approach

Replace the `SelectorWidget`'s bespoke query-editing implementation (a raw
`String` with manual `push()`/`pop()` handling) with a `MiniBuffer` instance.
This gives the file picker query field the full editing affordance set for free:
word-jump, kill-line, shift-selection, clipboard, Emacs-style Ctrl bindings.

**Strategy:**

1. **Replace `query: String` with `mini_buffer: MiniBuffer`** in `SelectorWidget`.
   The `MiniBuffer` constructor requires `FontMetrics`, but since the widget's
   zero-argument `new()` signature must be preserved, we'll construct `MiniBuffer`
   with default metrics (the metrics only affect internal viewport bookkeeping
   which is irrelevant for a single-line query field — rendering pulls content
   via `content()`, not viewport coordinates).

2. **Change `query()` accessor** to delegate to `mini_buffer.content()`. The
   return type becomes `String` (owned) since `MiniBuffer::content()` returns
   `String`, not `&str`. Callers in `editor_state.rs` and `selector_overlay.rs`
   already call `.to_string()` or `.chars()` on the query, so this change has
   minimal ripple effects.

3. **Refactor `handle_key()`** to delegate all query-editing keys to `MiniBuffer`
   while preserving the selector's navigation logic (Up/Down/Return/Escape):
   - Up/Down: handled by `SelectorWidget` directly (list navigation)
   - Return/Escape: handled by `SelectorWidget` directly (outcome signaling)
   - All other keys: delegate to `mini_buffer.handle_key()`. Compare
     `content()` before and after; if changed, reset `selected_index` to 0.

4. **Tests remain unchanged.** The observable behavior (query string changes on
   Backspace and printable chars, Up/Down navigation, Enter/Escape outcomes)
   is identical; only the internal mechanism differs.

**Testing approach:** Per TESTING_PHILOSOPHY.md, the existing unit tests verify
semantic behavior (query content changes, selection index changes, outcome
signaling). No new tests are required because the behavior is unchanged — we're
only swapping the implementation. A manual smoke test validates the new
affordances (Option+Backspace, Ctrl+A, Cmd+V).

## Sequence

### Step 1: Add MiniBuffer field to SelectorWidget

**Location:** `crates/editor/src/selector.rs`

Replace the `query: String` field with `mini_buffer: MiniBuffer`.

Since `SelectorWidget::new()` must remain zero-argument:
- Construct `MiniBuffer` with default `FontMetrics` values. These metrics
  affect internal viewport calculations in `MiniBuffer`, but since
  `SelectorWidget` only reads `content()`, `cursor_col()`, and
  `selection_range()`, the exact metric values don't affect behavior.

Add the import:
```rust
use crate::mini_buffer::MiniBuffer;
use crate::font::FontMetrics;
```

Change the struct definition:
```rust
pub struct SelectorWidget {
    mini_buffer: MiniBuffer,
    items: Vec<String>,
    selected_index: usize,
    view_offset: usize,
    visible_items: usize,
}
```

Update `new()`:
```rust
pub fn new() -> Self {
    // Default metrics for MiniBuffer (values don't affect query behavior,
    // only internal viewport calculations which aren't used by selector)
    let metrics = FontMetrics {
        advance_width: 8.0,
        line_height: 16.0,
        ascent: 12.0,
        descent: 4.0,
        leading: 0.0,
        point_size: 14.0,
    };
    Self {
        mini_buffer: MiniBuffer::new(metrics),
        items: Vec::new(),
        selected_index: 0,
        view_offset: 0,
        visible_items: 0,
    }
}
```

### Step 2: Update query() accessor

**Location:** `crates/editor/src/selector.rs`

Change the return type from `&str` to `String` to match `MiniBuffer::content()`:

```rust
/// Returns the current query string.
pub fn query(&self) -> String {
    self.mini_buffer.content()
}
```

### Step 3: Update callers of query() for String return type

**Locations:**
- `crates/editor/src/editor_state.rs` — calls `selector.query()` and immediately
  converts to owned (`selector.query().to_string()`)
- `crates/editor/src/selector_overlay.rs` — calls `widget.query().chars()`

Review each call site:
- `editor_state.rs` line ~392: `let prev_query = selector.query().to_string();`
  → Now `selector.query()` returns `String`, so `.to_string()` is redundant but
  harmless. We can simplify to `let prev_query = selector.query();`.
- `editor_state.rs` line ~400: `let current_query = selector.query();` — already
  takes ownership, no change needed.
- `editor_state.rs` line ~435: `selector.query().to_string()` — simplify.
- `selector_overlay.rs` line ~328: `widget.query().chars().count()` — works
  identically with `String`.
- `selector_overlay.rs` line ~422: `for c in widget.query().chars()` — works
  identically.

All callers are compatible; the signature change is non-breaking.

### Step 4: Refactor handle_key() to use MiniBuffer

**Location:** `crates/editor/src/selector.rs`

Replace the `Backspace` and `Char` handling branches with delegation to
`MiniBuffer`. The catch-all arm:
1. Captures `prev_query = self.mini_buffer.content()` before delegating.
2. Calls `self.mini_buffer.handle_key(event.clone())`.
3. If `mini_buffer.content() != prev_query`, resets `selected_index` to 0.
4. Returns `SelectorOutcome::Pending`.

Up/Down/Return/Escape are still handled directly by `SelectorWidget` before the
catch-all.

Updated `handle_key()`:
```rust
pub fn handle_key(&mut self, event: &KeyEvent) -> SelectorOutcome {
    match &event.key {
        Key::Up => {
            self.selected_index = self.selected_index.saturating_sub(1);
            if self.selected_index < self.view_offset {
                self.view_offset = self.selected_index;
            }
            SelectorOutcome::Pending
        }
        Key::Down => {
            if !self.items.is_empty() {
                let max_index = self.items.len() - 1;
                if self.selected_index < max_index {
                    self.selected_index += 1;
                }
            }
            if self.visible_items > 0
                && self.selected_index >= self.view_offset + self.visible_items
            {
                self.view_offset = self.selected_index - self.visible_items + 1;
            }
            SelectorOutcome::Pending
        }
        Key::Return => {
            if self.items.is_empty() {
                SelectorOutcome::Confirmed(usize::MAX)
            } else {
                SelectorOutcome::Confirmed(self.selected_index)
            }
        }
        Key::Escape => SelectorOutcome::Cancelled,
        _ => {
            // Delegate all other keys to MiniBuffer
            let prev_query = self.mini_buffer.content();
            self.mini_buffer.handle_key(event.clone());
            if self.mini_buffer.content() != prev_query {
                self.selected_index = 0;
            }
            SelectorOutcome::Pending
        }
    }
}
```

Note: The original code checked `!has_command_or_control` before handling
`Backspace` and `Char`. With `MiniBuffer` delegation, Cmd+Backspace and similar
combinations are handled by `MiniBuffer` itself (e.g., Cmd+Backspace may delete
to start of line). This is correct and desirable — it gives the query field
more affordances.

### Step 5: Run existing tests

**Location:** `crates/editor/src/selector.rs`

Run `cargo test -p lite_edit` (or the appropriate test command) to verify all
existing `SelectorWidget` tests pass. The tests check observable behavior:
- `query()` returns expected content after character input
- `query()` changes on backspace
- `selected_index` resets on typing
- Up/Down/Return/Escape behavior

Since we haven't changed any observable behavior, all tests should pass.

### Step 6: Manual smoke test

**Verification steps:**
1. Build and run the editor.
2. Press Cmd+P to open the file picker.
3. Type a partial filename to verify basic query editing works.
4. Test new affordances:
   - Option+Backspace: delete word backward
   - Ctrl+A (or Cmd+Left): jump to start
   - Ctrl+E (or Cmd+Right): jump to end
   - Shift+Left/Right: extend selection
   - Cmd+A: select all
   - Cmd+V: paste from clipboard
   - Cmd+C: copy selection (if any)
5. Verify Up/Down navigation still works.
6. Verify Return confirms and Escape cancels.

## Dependencies

- **mini_buffer_model chunk (ACTIVE)**: Provides `MiniBuffer` struct with
  `new()`, `handle_key()`, `content()`, and other accessors. This chunk is
  already completed and merged.

## Risks and Open Questions

- **`handle_key` signature mismatch**: `MiniBuffer::handle_key` takes
  `KeyEvent` by value (`event: KeyEvent`), while `SelectorWidget::handle_key`
  takes `&KeyEvent`. We'll need to clone the event when delegating. This is
  cheap (small struct) and correct.

- **Default FontMetrics values**: The exact values don't matter for
  `SelectorWidget`'s use case (reading content/cursor/selection), but we should
  verify that `MiniBuffer::new()` doesn't panic or behave unexpectedly with
  any reasonable metric values.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->