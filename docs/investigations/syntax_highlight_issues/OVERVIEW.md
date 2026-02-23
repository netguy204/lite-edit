---
status: SOLVED
trigger: "Scrolling a syntax-highlighted buffer is sluggish with high CPU usage, and certain symbols/lines are duplicated in Rust files"
proposed_chunks: []
created_after: ["syntax_highlighting_scalable"]
---

## Trigger

Since introducing syntax highlighting, two user-visible issues have appeared:

1. **Performance regression during scrolling**: Scrolling through a syntax-highlighted buffer consumes significant CPU and feels sluggish compared to plain-text rendering.

2. **Text duplication in highlighted Rust files**: Doc comments and other text are visually duplicated inline. For example, a line like `/// Updates the viewport size...` renders as `/// Updates the viewport size.../// Updates the viewport size...` — the entire line content doubled. This was observed in `editor_state.rs` (see screenshot at `/Users/btaylor/Desktop/Screenshot 2026-02-22 at 6.45.16 PM.png`). Method names, keywords, and other tokens also show signs of duplication (e.g., `viewport_mutviewport_mut()`, `update_sizeupdate_size()`).

## Success Criteria

- Identify the root cause of the duplicated text in syntax-highlighted output
- Identify the root cause of the scrolling performance regression
- Propose concrete fixes (as chunks) for both issues

## Testable Hypotheses

### H1: The viewport cache is invalidated on every `styled_line()` call, causing O(N) re-highlights per frame

- **Rationale**: In `HighlightedBufferView::styled_line(line)`, `highlight_viewport(line, line + 80)` is called with `line` as the start. The cache validity check in `HighlightCache::is_valid()` requires an **exact match** of `start_line` and `end_line`. So when rendering lines 0 through 59, the first call populates the cache for `(0, 80)`, but the second call for line 1 requests `(1, 81)` — which doesn't match, so the cache is thrown away and the entire viewport is re-highlighted. This repeats for every visible line, turning what should be a single `~170µs` highlight pass into `60 × ~170µs ≈ 10ms` of redundant work per frame.
- **Test**: Add logging/counters to `collect_captures_in_range` and verify it's called once per `styled_line()` call rather than once per frame. Alternatively, check if `is_valid()` ever returns true during normal rendering.
- **Status**: VERIFIED — Fixed by changing `HighlightCache::is_valid()` from exact-match to containment check (`<=`/`>=` instead of `==`).

### H2: Overlapping tree-sitter captures cause text duplication

- **Rationale**: Tree-sitter queries can match multiple capture patterns for the same node. For example, a doc comment `/// ...` may match both `@comment` and `@comment.documentation` patterns. `collect_captures_in_range()` collects ALL captures and `build_line_from_captures()` iterates through them sequentially. When two captures overlap the same byte range:
  - Capture 1 `(0, 50, "comment")`: `covered_until` advances from 0 to 50, span text emitted.
  - Capture 2 `(0, 50, "comment.documentation")`: `actual_start (0) > covered_until (50)` is false, so no gap is added. But the code **still emits** `source[0..50]` as a second span since there's no check to skip already-covered bytes.
  
  This produces the doubled text seen in the screenshot.
- **Test**: Log captures for a line with doc comments and verify multiple captures exist for the same byte range. Or add a unit test with a doc comment and check `StyledLine` char count matches actual line length.
- **Status**: VERIFIED — Fixed by adding `if actual_start < covered_until { continue; }` guard in both `build_line_from_captures` and `build_styled_line_from_query`.

### H3: The `line_byte_range()` method has O(n) complexity contributing to overall slowness

- **Rationale**: `line_byte_range()` iterates from the beginning of the source for every call. In `build_line_from_captures()` during viewport highlighting, it's called once per line. For a 60-line viewport starting at line 500, this means scanning ~500 lines worth of characters × 60 times. For large files, this contributes meaningful overhead.
- **Test**: Profile `line_byte_range` call count and time during viewport highlighting.
- **Status**: UNTESTED — secondary concern compared to H1 and H2, but worth addressing.

## Exploration Log

### 2026-02-22: Initial code analysis

Reviewed the code path for syntax-highlighted rendering:

1. `glyph_buffer.rs` calls `view.styled_line(buffer_line)` for each visible line
2. `HighlightedBufferView::styled_line()` calls `hl.highlight_viewport(line, line + 80)` then `hl.highlight_line(line)`
3. `highlight_viewport()` checks cache validity with exact `(start_line, end_line, generation)` match
4. Cache miss triggers `collect_captures_in_range()` + `build_line_from_captures()` for 80 lines
5. `highlight_line()` then retrieves from cache

**Key observations:**

- **Cache thrashing (H1)**: Each successive `styled_line(N)` call uses a different `start_line`, so the cache is rebuilt from scratch for every visible line. The fix would be to either:
  - Call `highlight_viewport` once with the actual viewport range before iterating lines, OR
  - Change cache validity to check if the requested line falls within the cached range (superset check) rather than exact range match

- **Duplicate text (H2)**: `build_line_from_captures` processes captures sequentially but doesn't skip captures whose byte range is already covered. The missing guard is:
  ```rust
  if actual_start < covered_until {
      covered_until = covered_until.max(actual_end);
      continue; // Skip overlapping capture
  }
  ```

- **O(n) line scanning (H3)**: `line_byte_range()` scans from byte 0 on every call. A line offset index (built once at parse time) would make this O(1).

## Findings

### Verified Findings

*(Pending testing of hypotheses)*

### Hypotheses/Opinions

- H1 (cache thrashing) and H2 (overlapping captures) together fully explain both reported symptoms.
- H1 is the performance regression: instead of 1 highlight pass per frame, we do ~60 passes.
- H2 is the duplication: overlapping captures for the same byte range produce duplicate span text.
- Both fixes are straightforward and low-risk.

## Proposed Chunks

1. **Fix overlapping capture duplication in syntax highlighter**: Add a guard in `build_line_from_captures` and `build_styled_line_from_query` to skip captures whose byte range is already covered by a previous capture. Add unit test with Rust doc comments to verify char count matches source line length.
   - Priority: High
   - Dependencies: None

2. **Fix viewport cache thrashing in highlighted buffer view**: Change the cache validity check to use a superset/containment check (does the cache contain the requested line?) rather than exact range match. Alternatively, restructure `HighlightedBufferView` to call `highlight_viewport` once with the true viewport bounds rather than per-line. Also consider making `line_byte_range()` O(1) via a precomputed offset index.
   - Priority: High
   - Dependencies: None (can be done in parallel with chunk 1)

## Resolution Rationale

Both root causes confirmed and fixed directly — no separate chunks needed.

**Fix 1 (duplication)**: Added overlap guard in `build_line_from_captures()` and `build_styled_line_from_query()` — when a capture's `actual_start < covered_until`, skip it and advance `covered_until`. Regression test `test_no_duplicate_text_from_overlapping_captures` confirms doc comment text renders exactly once.

**Fix 2 (cache thrashing)**: Changed `HighlightCache::is_valid()` from exact range equality to containment check (`start_line <= requested && end_line >= requested`). The first `styled_line(0)` call populates the cache for lines 0–80, and all subsequent calls for lines 1–59 are cache hits. Regression test `test_viewport_cache_containment_avoids_thrashing` confirms subset requests don't invalidate the cache.
