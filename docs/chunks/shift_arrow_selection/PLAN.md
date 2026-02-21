# Implementation Plan

## Approach

This chunk implements Shift+Arrow key selection by building on the existing text selection model from the `text_selection_model` chunk. The approach follows the same stateless chord resolution pattern used for other key bindings in `buffer_target.rs`.

The key design decision is how to preserve the selection anchor when executing movement operations. The existing `move_*` methods on `TextBuffer` unconditionally clear the selection anchor. Rather than modifying the `TextBuffer` API, we use a save/restore pattern: save the anchor position before the move, execute the move (which clears the anchor), then restore the anchor.

The implementation adds:
1. New `Select*` command variants to the `Command` enum
2. Updated `resolve_command` to map Shift+Arrow combinations to Select* commands
3. A helper method `extend_selection_with_move` that implements the anchor preservation pattern
4. Execute handlers for each Select* command that use the helper

## Subsystem Considerations

No subsystems are relevant to this chunk. The implementation follows the existing command resolution pattern established in `buffer_target.rs`.

## Sequence

### Step 1: Add Select* command variants

Add selection-extending command variants to the `Command` enum in `buffer_target.rs`:
- `SelectLeft` — extend selection one character left
- `SelectRight` — extend selection one character right
- `SelectUp` — extend selection one line up
- `SelectDown` — extend selection one line down
- `SelectToLineStart` — extend selection to beginning of line
- `SelectToLineEnd` — extend selection to end of line
- `SelectToBufferStart` — extend selection to buffer start
- `SelectToBufferEnd` — extend selection to buffer end

Location: `crates/editor/src/buffer_target.rs`

### Step 2: Update resolve_command for Shift+Arrow bindings

Update the `resolve_command` function to recognize Shift modifier combinations:
- Shift+Left → `SelectLeft`
- Shift+Right → `SelectRight`
- Shift+Up → `SelectUp`
- Shift+Down → `SelectDown`
- Shift+Home → `SelectToLineStart`
- Shift+End → `SelectToLineEnd`
- Shift+Cmd+Left → `SelectToLineStart`
- Shift+Cmd+Right → `SelectToLineEnd`
- Shift+Cmd+Up → `SelectToBufferStart`
- Shift+Cmd+Down → `SelectToBufferEnd`
- Shift+Ctrl+A → `SelectToLineStart` (Emacs-style)
- Shift+Ctrl+E → `SelectToLineEnd` (Emacs-style)

Important: Selection bindings must be matched before movement bindings since Shift+Arrow should resolve to Select*, not Move*.

Location: `crates/editor/src/buffer_target.rs`

### Step 3: Implement extend_selection_with_move helper

Add a helper method to `BufferFocusTarget` that:
1. Determines the anchor position:
   - If selection exists, compute anchor from `selection_range()` and cursor
   - If no selection, anchor is current cursor position
2. Executes the movement operation (which clears selection)
3. Restores the anchor using `set_selection_anchor()`
4. Marks dirty and ensures cursor visible

This helper encapsulates the anchor preservation pattern so each Select* command can reuse it.

Location: `crates/editor/src/buffer_target.rs`

### Step 4: Implement execute_command handlers for Select* commands

Add match arms for each Select* command that call `extend_selection_with_move` with the appropriate movement closure:
- `SelectLeft` → `|buf| buf.move_left()`
- `SelectRight` → `|buf| buf.move_right()`
- `SelectUp` → `|buf| buf.move_up()`
- `SelectDown` → `|buf| buf.move_down()`
- `SelectToLineStart` → `|buf| buf.move_to_line_start()`
- `SelectToLineEnd` → `|buf| buf.move_to_line_end()`
- `SelectToBufferStart` → `|buf| buf.move_to_buffer_start()`
- `SelectToBufferEnd` → `|buf| buf.move_to_buffer_end()`

Location: `crates/editor/src/buffer_target.rs`

### Step 5: Add unit tests

Add comprehensive unit tests verifying:
- Shift+Right from no selection creates selection of 1 character
- Shift+Right×3 selects 3 characters from starting position
- Shift+Left after Shift+Right shrinks selection
- Shift+Down extends selection to next line
- Plain Right after Shift+Right×3 clears selection and moves cursor
- Shift+Home selects from cursor to line start
- Shift+End selects from cursor to line end
- Selection persists when no keys are pressed
- Existing selection can be extended with Shift+Arrow
- Shift+Ctrl+A and Shift+Ctrl+E work for Emacs-style selection
- Shift+Cmd+Up/Down select to buffer start/end

Location: `crates/editor/src/buffer_target.rs` (tests module)

## Dependencies

- `text_selection_model` chunk must be complete — provides the anchor/cursor selection API on TextBuffer

## Risks and Open Questions

None identified. The implementation follows established patterns and the selection model is already well-tested.

## Deviations

None — implementation followed the planned approach.
