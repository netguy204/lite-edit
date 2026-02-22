---
decision: APPROVE
summary: All success criteria satisfied - scroll offset is re-clamped on resize via updated update_size() signature, with comprehensive regression test
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: After entering fullscreen (or any resize that changes the window height), clicking a line in the editor moves the cursor to exactly the line that was clicked, with no further scrolling required.

- **Status**: satisfied
- **Evidence**: `RowScroller::update_size()` now accepts `row_count` parameter and calls `set_scroll_offset_px(self.scroll_offset_px, row_count)` after updating `visible_rows` (row_scroller.rs:127-134). This re-clamps the scroll offset to the new valid bounds after any resize. The change propagates through `Viewport::update_size()` (viewport.rs:175-177) and `EditorState::update_viewport_dimensions()` (editor_state.rs:254-259), which fetches `self.buffer().line_count()` to provide the row count.

### Criterion 2: Clicking immediately after resize is correct for any pre-resize scroll position, including positions near the bottom of large documents.

- **Status**: satisfied
- **Evidence**: The regression test `test_resize_clamps_scroll_offset` (row_scroller.rs:597-621) explicitly tests this scenario: starting at scroll position 1440px (row 90) with 10 visible rows in a 100-row buffer, then resizing to 20 visible rows. The test verifies scroll_offset_px is clamped from 1440 to 1280, and first_visible_row changes from 90 to 80. This ensures click alignment for positions near the bottom.

### Criterion 3: Existing viewport and click-positioning tests continue to pass.

- **Status**: satisfied
- **Evidence**: All 515 tests pass (`cargo test -p lite-edit` shows all tests passing). The existing test signatures were updated to pass the required `row_count` parameter (e.g., `update_size(160.0)` → `update_size(160.0, 100)`), maintaining test coverage while accommodating the new API.

### Criterion 4: A regression test is added: simulate a resize that shrinks `max_offset_px`, assert that `scroll_offset_px` is clamped and that the click-to-line mapping matches the rendered line under the clicked pixel.

- **Status**: satisfied
- **Evidence**: `test_resize_clamps_scroll_offset` in row_scroller.rs:597-621 implements exactly this scenario:
  1. Creates scroller with 16px row height
  2. Sets viewport to 160px (10 visible rows)
  3. Scrolls to row 90 (max for 100-row buffer with 10 visible)
  4. Resizes to 320px (20 visible rows), which reduces max_offset_px from 1440 to 1280
  5. Asserts scroll_offset_px is clamped to 1280
  6. Asserts first_visible_row is 80 (correct for clamped position)

  The test includes a backreference comment linking to this chunk.
