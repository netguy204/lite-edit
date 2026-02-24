<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds Emacs-style navigation keybindings and page up/down support to the buffer focus target. The implementation follows the existing patterns established by the `viewport_scrolling` chunk and the command resolution system in `buffer_target.rs`.

**Key design decisions:**

1. **Stateless command resolution**: All new keybindings map to `Command` enum variants via the pure `resolve_command` function. No state machine changes are needed since all new bindings are single-step modifier+key combinations.

2. **Page scrolling uses viewport.visible_lines()**: Per the `viewport_scroll` subsystem (docs/subsystems/viewport_scroll/OVERVIEW.md), `Viewport::visible_lines()` returns the number of lines that fit in the viewport. Page Up/Down will scroll by this amount.

3. **Cursor moves with viewport on page scroll**: Unlike trackpad scrolling (which only moves the viewport and leaves the cursor in place), Page Up/Down moves both the viewport and the cursor by the same amount. This matches Emacs `scroll-up-command`/`scroll-down-command` behavior.

4. **Ctrl+V aliases Page Down**: Emacs's `scroll-up-command` is bound to Ctrl+V. The naming is confusing (content scrolls up, view moves down), but the behavior is: viewport advances toward the end of the buffer.

5. **Ctrl+F and Ctrl+B are character movement**: These Emacs bindings map directly to `MoveRight` and `MoveLeft` commands already implemented.

**Testing strategy per TESTING_PHILOSOPHY.md:**
- Write unit tests in `buffer_target.rs` following the existing test patterns
- Tests exercise `resolve_command` for keybinding correctness
- Tests exercise `handle_key` through `EditorContext` for behavior correctness
- No GPU/platform code involved — all testable via the pure state manipulation pattern

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport_scroll subsystem for page scrolling. Specifically, it uses `Viewport::visible_lines()` to determine page size and `Viewport::set_scroll_offset_px_wrapped()` (via `handle_scroll`) for scroll clamping. No subsystem deviations introduced.

## Sequence

### Step 1: Add PageUp and PageDown Command variants

Add two new variants to the `Command` enum in `buffer_target.rs`:
- `PageUp` — move viewport and cursor up by one page
- `PageDown` — move viewport and cursor down by one page

Location: `crates/editor/src/buffer_target.rs#Command`

### Step 2: Add keybinding resolution for Page Up/Down and Emacs bindings

Extend `resolve_command` to handle:
- `Key::PageUp` → `Command::PageUp`
- `Key::PageDown` → `Command::PageDown`
- `Key::Char('v')` with `control && !command` → `Command::PageDown` (Emacs scroll-up)
- `Key::Char('f')` with `control && !command` → `Command::MoveRight` (forward-char)
- `Key::Char('b')` with `control && !command` → `Command::MoveLeft` (backward-char)

Note: Ctrl+F and Ctrl+B reuse existing `MoveLeft`/`MoveRight` commands.

Location: `crates/editor/src/buffer_target.rs#resolve_command`

### Step 3: Implement PageUp/PageDown command execution

In `execute_command`, add cases for `PageUp` and `PageDown`:

1. **PageUp**:
   - Get `page_size = ctx.viewport.visible_lines()`
   - Move cursor up by `page_size` lines (using `buffer.move_up()` in a loop, or a new dedicated method if available)
   - Scroll viewport up by `page_size * line_height` pixels
   - Clamp cursor to buffer bounds (line 0 minimum)
   - Mark full viewport dirty
   - Ensure cursor visible

2. **PageDown**:
   - Get `page_size = ctx.viewport.visible_lines()`
   - Move cursor down by `page_size` lines
   - Scroll viewport down by `page_size * line_height` pixels
   - Clamp cursor to buffer bounds (last line maximum)
   - Mark full viewport dirty
   - Ensure cursor visible

For the scroll portion, we'll adjust `scroll_offset_px` directly rather than using `handle_scroll` (which is for trackpad input). We use `set_scroll_offset_px_wrapped` to respect wrap-aware bounds.

Location: `crates/editor/src/buffer_target.rs#BufferFocusTarget::execute_command`

### Step 4: Write unit tests for keybinding resolution

Add tests to `buffer_target.rs` to verify:
- `resolve_command` maps `Key::PageUp` → `Command::PageUp`
- `resolve_command` maps `Key::PageDown` → `Command::PageDown`
- `resolve_command` maps Ctrl+V → `Command::PageDown`
- `resolve_command` maps Ctrl+F → `Command::MoveRight`
- `resolve_command` maps Ctrl+B → `Command::MoveLeft`

Follow the existing test pattern (e.g., `test_ctrl_a_select_all`).

Location: `crates/editor/src/buffer_target.rs` (test module)

### Step 5: Write integration tests for Page Up/Down behavior

Add tests to `buffer_target.rs` to verify:
- Page Down moves cursor down by viewport height and scrolls viewport
- Page Up moves cursor up by viewport height and scrolls viewport
- Page Down at buffer bottom clamps cursor to last line
- Page Up at buffer top clamps cursor to line 0
- Both commands work correctly with line wrapping enabled
- Ctrl+V behaves identically to Page Down

These tests will use `EditorContext` with a mock viewport and buffer, similar to existing tests like `test_move_down`.

Location: `crates/editor/src/buffer_target.rs` (test module)

### Step 6: Update chunk GOAL.md with code_paths

Add the files touched by this implementation to the `code_paths` field in the chunk's GOAL.md frontmatter:
- `crates/editor/src/buffer_target.rs`

Location: `docs/chunks/viewport_emacs_navigation/GOAL.md`

## Dependencies

- **viewport_scrolling chunk (ACTIVE)**: Provides `Viewport::visible_lines()` and scroll offset manipulation. Already complete.
- **line_wrap_rendering chunk (ACTIVE)**: Provides wrap-aware viewport calculations. Already complete.
- **input crate**: Already defines `Key::PageUp` and `Key::PageDown` variants.

## Risks and Open Questions

1. **Cursor movement granularity**: The buffer API provides `move_up()` and `move_down()` for single-line movement. For page scrolling, we need to move by `visible_lines` lines. Options:
   - Call `move_up()`/`move_down()` in a loop (simple but O(n) calls)
   - Add a new `move_up_by(n)` / `move_down_by(n)` method to TextBuffer
   - Directly set cursor position with `set_cursor(Position::new(new_line, col))`

   **Decision**: Use `set_cursor` directly with clamped line calculation. This is simpler and follows the pattern used by `MoveToBufferStart`/`MoveToBufferEnd`.

2. **Column preservation across page jumps**: When moving down a full page, should the cursor column be preserved (sticky column behavior)? The existing `move_up`/`move_down` methods handle sticky columns. Using `set_cursor` directly may not preserve this.

   **Decision**: Preserve the current column by reading it before the jump and restoring it (clamped to line length) after. This matches expected Emacs behavior.

3. **Wrap-aware page size**: With line wrapping, a "page" could mean either:
   - Number of buffer lines visible (ignoring that some wrap to multiple screen rows)
   - Number of screen rows visible (more accurate for visual paging)

   **Decision**: Use `visible_lines()` which returns screen rows. This gives a consistent visual page size regardless of wrapping. The cursor moves by the same visual distance the viewport scrolls.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->