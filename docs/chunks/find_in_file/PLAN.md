# Implementation Plan

## Approach

This chunk implements a live find-in-file feature (Cmd+F) that follows the
established patterns in lite-edit:

1. **Focus Model Extension**: Add a new `EditorFocus::FindInFile` variant to
   the existing focus enum, following the same pattern as `EditorFocus::Selector`.

2. **Reuse MiniBuffer**: Use the `MiniBuffer` struct (from chunk `mini_buffer_model`)
   for query input. This gives us full editing affordances (word-jump, selection,
   clipboard) for free.

3. **Humble View Architecture**: All logic (search, match selection, cursor
   positioning) stays in testable pure Rust code (`EditorState`). The renderer
   is a thin projection of state to pixels.

4. **Rendering Pattern**: Follow `render_with_selector` pattern — add a new
   `render_with_find_strip` entry point in `renderer.rs` that draws the find
   strip at the bottom of the viewport (distinct from the centered floating
   overlay used by the selector).

5. **Search Implementation**: Case-insensitive substring search on the raw
   content string. Wrap around at buffer end. Set the main buffer's selection
   to highlight the match.

Tests follow TDD and the project's testing philosophy:
- Unit tests for state transitions (focus changes, search behavior, match advancing)
- Tests exercise `EditorState` directly without Metal dependencies
- Boundary conditions: empty buffer, no match, wrap-around, Cmd+F while open

## Subsystem Considerations

No subsystems are relevant to this chunk. The focus/state management and
rendering patterns are established but not captured as formal subsystems.

## Sequence

### Step 1: Add `EditorFocus::FindInFile` variant

Extend the `EditorFocus` enum in `editor_state.rs` to add a `FindInFile` variant.
This mirrors the existing `Selector` variant structure.

**Location**: `crates/editor/src/editor_state.rs`

**Changes**:
```rust
pub enum EditorFocus {
    #[default]
    Buffer,
    Selector,
    FindInFile,  // NEW
}
```

### Step 2: Add find-in-file state fields to `EditorState`

Add the state fields needed to track the find-in-file session:

**Location**: `crates/editor/src/editor_state.rs`

**Fields to add**:
```rust
/// The MiniBuffer for the find query (when focus == FindInFile)
pub find_mini_buffer: Option<MiniBuffer>,

/// The buffer position from which the current search started
/// (used as the search origin; only advances when Enter is pressed)
pub search_origin: lite_edit_buffer::Position,
```

Update `EditorState::new()` and `EditorState::default()` to initialize these fields.

### Step 3: Implement `handle_cmd_f()` to open the find strip

Add a method to open the find-in-file strip:

**Location**: `crates/editor/src/editor_state.rs`

**Behavior**:
1. If `focus == Buffer`: create a new `MiniBuffer`, record cursor position as
   `search_origin`, set `focus = FindInFile`, mark `DirtyRegion::FullViewport`.
2. If `focus == FindInFile`: no-op (Cmd+F while open does nothing).
3. If `focus == Selector`: no-op (don't open find while file picker is open).

Wire this to Cmd+F in `handle_key()` (alongside existing Cmd+P, Cmd+Q, etc.).

### Step 4: Implement `close_find_strip()`

Add a method to close the find-in-file strip:

**Location**: `crates/editor/src/editor_state.rs`

**Behavior**:
1. Set `find_mini_buffer = None`.
2. Set `focus = Buffer`.
3. Mark `DirtyRegion::FullViewport`.
4. Leave the main buffer's cursor and selection at their current positions
   (the last match position).

### Step 5: Implement `find_next_match()` search function

Add a helper function for case-insensitive forward substring search:

**Location**: `crates/editor/src/editor_state.rs` (private helper)

**Signature**:
```rust
fn find_next_match(
    buffer: &TextBuffer,
    query: &str,
    start_pos: Position,
) -> Option<(Position, Position)>
```

**Behavior**:
1. If query is empty, return `None`.
2. Get the buffer content as a single string.
3. Convert `start_pos` to a byte offset in the content.
4. Search forward (case-insensitive) from that byte offset.
5. If found, convert the match byte range back to `(start_pos, end_pos)`.
6. If not found before end, wrap around and search from the beginning up to `start_pos`.
7. Return the match range or `None` if no match.

### Step 6: Implement `handle_key_find()` for find-strip key routing

Add a method to handle key events when `focus == FindInFile`:

**Location**: `crates/editor/src/editor_state.rs`

**Key routing**:
- `Key::Escape` → call `close_find_strip()`
- `Key::Return` → advance `search_origin` past current match, re-run search
- All other keys → delegate to `find_mini_buffer.handle_key(event)`, then if
  content changed, run live search

**Live search on content change**:
1. Get the query from `find_mini_buffer.content()`.
2. Call `find_next_match(buffer, query, search_origin)`.
3. If match found: set the main buffer's selection to the match range, scroll
   viewport to make match visible.
4. If no match: clear the main buffer's selection.
5. Mark `DirtyRegion::FullViewport`.

### Step 7: Wire key routing through `handle_key()`

Update `handle_key()` to route events based on focus:

**Location**: `crates/editor/src/editor_state.rs`

**Changes**:
1. Add Cmd+F handling (call `handle_cmd_f()`).
2. Add `EditorFocus::FindInFile` case to the match that routes to
   `handle_key_find()` (analogous to the existing `handle_key_selector()` path).

### Step 8: Wire mouse/scroll events for find mode

When `focus == FindInFile`:
- **Mouse events**: Route to the buffer (user can scroll/click in main buffer
  while searching). The find strip doesn't handle mouse events.
- **Scroll events**: Route to the buffer (scroll the main content).

This is different from Selector mode where mouse/scroll go to the overlay.

**Location**: `crates/editor/src/editor_state.rs` (update `handle_mouse()` and
`handle_scroll()`)

### Step 9: Add find strip geometry calculation

Add layout calculation for the find strip (bottom-anchored, 1 line tall):

**Location**: `crates/editor/src/selector_overlay.rs` (or a new section in that file)

**Struct**:
```rust
pub struct FindStripGeometry {
    pub strip_y: f32,      // Y coordinate (bottom of viewport - line_height)
    pub strip_height: f32, // 1 line height
    pub content_x: f32,    // X where "find:" label starts
    pub query_x: f32,      // X where query text starts (after label)
    pub cursor_x: f32,     // X of cursor position in query
}
```

**Function**:
```rust
pub fn calculate_find_strip_geometry(
    view_width: f32,
    view_height: f32,
    line_height: f32,
    query_len: usize,
    cursor_col: usize,
    glyph_width: f32,
) -> FindStripGeometry
```

### Step 10: Add `FindStripGlyphBuffer` for rendering

Create a glyph buffer for the find strip (similar to `SelectorGlyphBuffer`):

**Location**: `crates/editor/src/selector_overlay.rs` (new struct alongside existing)

**Elements to render**:
1. Background rect (same color as selector overlay: `OVERLAY_BACKGROUND_COLOR`)
2. "find:" label text (dim color)
3. Query text
4. Blinking cursor (if visible)

**Quad ranges**:
- `background_range`
- `label_range`
- `query_text_range`
- `cursor_range`

### Step 11: Add `render_with_find_strip()` to Renderer

Add a new rendering entry point:

**Location**: `crates/editor/src/renderer.rs`

**Signature**:
```rust
pub fn render_with_find_strip(
    &mut self,
    view: &MetalView,
    editor: &Editor,
    find_query: &str,
    find_cursor_col: usize,
    find_cursor_visible: bool,
)
```

**Behavior**:
1. Render the left rail.
2. Render the editor content (but reduce visible area by 1 line at bottom for
   the find strip — or just let it overlap).
3. Render the find strip on top at the bottom.

Alternatively, integrate into `render_with_editor()` by adding an optional find
strip state parameter.

### Step 12: Wire rendering in main loop

Update the main render loop to call the appropriate render method based on
`EditorState.focus`:

**Location**: Main event loop (wherever `renderer.render_with_editor()` is called)

**Logic**:
```rust
match state.focus {
    EditorFocus::Buffer => renderer.render_with_editor(..., None, false),
    EditorFocus::Selector => renderer.render_with_editor(..., Some(&selector), cursor_visible),
    EditorFocus::FindInFile => {
        // Get find mini buffer state
        if let Some(ref mb) = state.find_mini_buffer {
            renderer.render_with_find_strip(..., mb.content(), mb.cursor_col(), cursor_visible);
        }
    }
}
```

### Step 13: Unit tests for find state transitions

Write tests verifying focus model behavior:

**Location**: `crates/editor/src/editor_state.rs` (test module)

**Tests**:
- `test_cmd_f_transitions_to_find_focus`: Cmd+F from Buffer → focus is `FindInFile`
- `test_cmd_f_creates_mini_buffer`: `find_mini_buffer` is `Some` after Cmd+F
- `test_cmd_f_records_search_origin`: `search_origin` equals cursor position
- `test_escape_closes_find_strip`: Escape → focus returns to `Buffer`, `find_mini_buffer` is `None`
- `test_cmd_f_while_open_is_noop`: Cmd+F when focus is `FindInFile` → no change

### Step 14: Unit tests for live search behavior

Write tests verifying search and match selection:

**Location**: `crates/editor/src/editor_state.rs` (test module)

**Tests**:
- `test_typing_in_find_selects_match`: Type query → buffer selection covers match
- `test_no_match_clears_selection`: Type query with no matches → buffer selection is None
- `test_enter_advances_to_next_match`: Enter → search origin moves, next match selected
- `test_search_wraps_around`: Match found after wrap at buffer end
- `test_case_insensitive_match`: "HELLO" matches "hello" in buffer

### Step 15: Unit tests for edge cases

**Location**: `crates/editor/src/editor_state.rs` (test module)

**Tests**:
- `test_find_in_empty_buffer`: Cmd+F on empty buffer, type query → no crash, no match
- `test_empty_query_no_selection`: Empty query string → no selection
- `test_multiple_enter_advances`: Multiple Enter presses cycle through matches

## Dependencies

- **mini_buffer_model** (chunk): Must be complete. This chunk depends on the
  `MiniBuffer` struct for query input. ✓ Listed in `depends_on` in GOAL.md.

## Risks and Open Questions

1. **Viewport adjustment for find strip**: The find strip takes 1 line at the
   bottom. We may need to reduce the visible buffer area by 1 line when find is
   open to prevent content from being hidden behind the strip. Alternatively,
   we could let it overlap (simpler, but text may be obscured).

2. **Position ↔ byte offset conversion**: The search operates on the raw content
   string, but `Position` is line/col based. Need to carefully implement the
   conversion functions. The existing `TextBuffer` has some position utilities
   that may help.

3. **Selection anchor behavior**: When setting the match as a selection, we need
   to set both the cursor and the selection anchor. The `TextBuffer` API supports
   this via `set_selection_anchor()` and `set_cursor_unchecked()`.

4. **Cursor visibility during find**: The main buffer's cursor should probably
   not blink while find is active (the find strip cursor blinks instead). Need
   to pass the right `cursor_visible` value to the buffer renderer.

## Deviations

_To be populated during implementation._