---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/viewport.rs
- crates/editor/src/row_scroller.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::update_viewport_size
    implements: "Subtracts TAB_BAR_HEIGHT from window_height to compute correct content_height for visible_lines calculation"
  - ref: crates/editor/src/editor_state.rs#EditorState::update_viewport_dimensions
    implements: "Subtracts TAB_BAR_HEIGHT from window_height to compute correct content_height for visible_lines calculation"
  - ref: crates/editor/src/editor_state.rs#EditorState::run_live_search
    implements: "Calls ensure_visible_with_margin with margin=1 so matches land above the find strip"
  - ref: crates/editor/src/viewport.rs#Viewport::ensure_visible_with_margin
    implements: "Viewport method that delegates to RowScroller with bottom margin for overlays"
  - ref: crates/editor/src/row_scroller.rs#RowScroller::ensure_visible_with_margin
    implements: "Core scrolling logic with bottom_margin_rows parameter to account for overlays like find strip"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on:
- find_in_file
created_after:
- agent_lifecycle
- content_tab_bar
- terminal_input_encoding
- find_in_file
- cursor_wrap_scroll_alignment
- row_scroller_extract
- selector_row_scroller
- selector_smooth_render
---

# Chunk Goal

## Minor Goal

When the find strip is open and the viewport scrolls to reveal a match, the
match is not visible. There are two compounding root causes that together push
the match below the actual visible area.

This chunk fixes both causes so that matches are always clearly visible above
the find strip.

## Root Cause

### 1 — Viewport size over-counts visible lines (tab bar not subtracted)

`EditorState::update_viewport_dimensions` passes the full `window_height` to
`Viewport::update_size`, which computes `visible_lines = floor(window_height /
line_height)`. The content area is actually `window_height - TAB_BAR_HEIGHT`
pixels tall (the tab bar occupies the top 32 px). With a typical `line_height`
of 16 px this over-counts by `floor(32 / 16) = 2` lines. Those phantom lines
are "below" the window and never actually rendered on screen, but
`ensure_visible` still treats them as valid scroll targets, so a match can end
up scrolled to a position that is invisible.

The fix: pass `window_height - TAB_BAR_HEIGHT` (i.e. `content_height`) to
`Viewport::update_size` instead of the raw `window_height`. This is the same
`content_height` already computed for mouse and scroll event handling in the
same file.

### 2 — Find strip occludes the last visible line

`Viewport::ensure_visible` scrolls the target line to the very last visible
row (`visible_lines - 1`). The find strip (`line_height + 2 *
FIND_STRIP_PADDING_Y ≈ 24 px`) is rendered over the bottom of the content
area, hiding that last row when find mode is active.

The fix: when calling `ensure_visible` from `run_live_search` /
`advance_to_next_match`, reduce the effective viewport size by 1 so the match
lands at `visible_lines - 2` — one row above the find strip.

## Success Criteria

- `update_viewport_dimensions` passes `window_height - TAB_BAR_HEIGHT` to
  `Viewport::update_size`; the existing `test_update_size` tests are updated
  to reflect the corrected line count and continue to pass.
- When find mode is active and scrolling is needed to reveal a match, the
  match lands at most at the second-to-last visible row, fully above the find
  strip.
- When find mode is not active, `ensure_visible` behaves exactly as before —
  no regression for normal cursor scrolling.
- The find-strip margin is applied only at the call sites in `run_live_search`
  / `advance_to_next_match` (or via a `ensure_visible_with_margin` helper),
  not by changing the generic `ensure_visible` signature.
- Manual verification: open a file, press Ctrl+F, type a query whose nearest
  match would land near the bottom of the viewport — the match is clearly
  visible above the find strip with no extra scrolling required.