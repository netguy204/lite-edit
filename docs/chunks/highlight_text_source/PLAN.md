# Implementation Plan

## Approach

This chunk fixes a class of bug where typed characters are invisible until a
different action triggers a highlighter sync. The root cause is twofold:

1. **Architectural**: `HighlightedBufferView::styled_line()` delegates text
   retrieval entirely to `SyntaxHighlighter::highlight_line()`, which reads
   from the highlighter's internal `self.source` field. When the buffer is
   mutated without syncing the highlighter, the renderer draws **stale text**
   from the highlighter while the cursor position comes from the current buffer.

2. **Missing sync calls**: Four buffer mutation paths don't call
   `sync_active_tab_highlighter()`:
   - `handle_insert_text()` — regular keyboard input
   - `handle_set_marked_text()` — IME composition
   - `handle_unmark_text()` — IME cancellation
   - `handle_file_drop()` — drag-and-drop file path insertion

**Strategy:**

We implement both parts of the fix:

1. **Architectural fix**: Modify `HighlightedBufferView::styled_line()` to read
   line text from the `TextBuffer` (always current), then apply style spans from
   the highlighter. This makes rendering resilient to highlighter sync gaps —
   the worst case becomes slightly stale colors for one frame rather than
   invisible text.

2. **Sync coverage fix**: Add `sync_active_tab_highlighter()` calls to all four
   missing mutation paths. This keeps highlight colors current. Even with the
   architectural fix, this is needed — without it, syntax colors would remain
   stale until the next `handle_key` action.

This follows the testing philosophy's "humble view" pattern: the text content
is always correct (from the buffer), and the styling is a visual enhancement
that gracefully degrades when stale.

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk USES the renderer
  subsystem. The renderer consumes `BufferView` for content access. Our changes
  to `HighlightedBufferView` stay within the existing `BufferView` contract —
  `styled_line()` still returns `Option<StyledLine>`. No deviation introduced.

## Sequence

### Step 1: Add a method to SyntaxHighlighter to return style spans for a line

Create a new method `highlight_spans_for_line(line_idx, line_text) -> Vec<Span>`
that returns only the styled spans for a given line, without including the text
content. This method will:

1. Look up the byte range for the line in the highlighter's source
2. Collect captures that intersect this line
3. Build spans using the **passed-in** `line_text` instead of reading from
   `self.source`

If the highlighter's source is out of sync (different line count or byte range),
return a single plain span covering the entire `line_text` as graceful fallback.

Location: `crates/syntax/src/highlighter.rs`

### Step 2: Modify HighlightedBufferView::styled_line to read text from buffer

Update `styled_line()` in both `HighlightedBufferView` and
`HighlightedBufferViewMut` to:

1. Read the line text from `self.buffer.line_content(line)` (always current)
2. If highlighter is Some, call the new `highlight_spans_for_line()` with the
   buffer's text
3. Build a `StyledLine` from the returned spans

This decouples text content from the highlighter's internal source copy.

Location: `crates/editor/src/highlighted_buffer.rs`

### Step 3: Add sync_active_tab_highlighter calls to the four mutation paths

Add `self.sync_active_tab_highlighter()` at the end of each mutation path that
modifies the buffer but was missing the sync:

1. `handle_insert_text()` — after the buffer insert for file tabs
2. `handle_set_marked_text()` — after setting marked text
3. `handle_unmark_text()` — after clearing marked text
4. `handle_file_drop()` — after inserting text for file tabs

Location: `crates/editor/src/editor_state.rs`

### Step 4: Write tests for the architectural fix

Add tests to `crates/syntax/src/highlighter.rs` that verify:

1. `highlight_spans_for_line()` returns correct spans when highlighter is in sync
2. When highlighter source is stale (different content), the method returns a
   plain span covering the passed-in text without panicking
3. Total character count of returned spans equals the input text length

### Step 5: Write integration test for styled_line with stale highlighter

Add a test to `crates/editor/src/highlighted_buffer.rs` that:

1. Creates a `TextBuffer` and a `SyntaxHighlighter` from the same source
2. Modifies the buffer WITHOUT syncing the highlighter
3. Creates `HighlightedBufferView` and calls `styled_line()`
4. Asserts that the rendered text matches the buffer's content (not the stale
   highlighter source)

This is the key semantic assertion that the architectural fix works.

### Step 6: Update GOAL.md code_paths

Update the chunk's GOAL.md frontmatter with the files touched.

## Dependencies

- `buffer_file_watching` (ACTIVE) — per `created_after` in GOAL.md frontmatter
- `highlight_injection` (ACTIVE) — per `created_after` in GOAL.md frontmatter

Both are already complete, so no blocking dependencies.

## Risks and Open Questions

- **Span alignment when highlighter is stale**: When the highlighter's source
  differs from the buffer, byte offsets won't align. The fallback (plain span)
  handles this gracefully, but syntax colors will be incorrect for that frame.
  This is acceptable per the success criteria: "correct text, potentially with
  slightly outdated syntax colors."

- **Performance**: The new `highlight_spans_for_line()` method adds minimal
  overhead — it reuses the existing capture collection and only changes where
  the text content comes from. Viewport caching in the highlighter remains
  effective.

- **Injection highlighting**: The architectural change must also work with
  injection highlighting (e.g., Markdown code blocks). The existing injection
  capture logic operates on byte ranges, which may need adjustment if the
  highlighter source and buffer source diverge. The graceful fallback to plain
  spans should cover this case.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->