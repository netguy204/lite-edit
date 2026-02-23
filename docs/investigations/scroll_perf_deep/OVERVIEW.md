---
status: SOLVED
trigger: "Scrolling a syntax-highlighted large Rust file still consumes ~60% CPU after fixing cache thrashing and symbol duplication in syntax_highlight_issues investigation"
proposed_chunks:
  - prompt: "Build a precomputed line offset index in SyntaxHighlighter to make line_byte_range() O(1) instead of O(n). Store a Vec<usize> of byte offsets for each line start, rebuilt on parse/edit. Also make line_count() O(1) by caching the count. This eliminates the dominant CPU cost during viewport highlighting of large files — profiling shows viewport highlight at line 4000 takes 6ms (75% of budget) due to O(n) line scanning, while a precomputed index reduces 62 lookups from 4432µs to 0.02µs."
    chunk_directory: highlight_line_offset_index
    depends_on: []
  - prompt: "Reduce per-frame allocations in the syntax highlighting hot path. (1) Store capture names as u32 indices instead of String in collect_captures_in_range(). (2) Use binary search instead of linear scan when filtering captures per line in build_line_from_captures(). (3) Cache and reuse the captures Vec across frames via a RefCell<Vec> to avoid re-allocation."
    chunk_directory: highlight_capture_alloc
    depends_on: [0]
  - prompt: "Eliminate redundant styled_line() calls in glyph_buffer update_from_buffer_with_wrap. Currently each visible buffer line calls view.styled_line() 3 times (background, glyph, underline phases). Restructure to call styled_line() once per buffer line and reuse the result across all phases. Profiling shows this saves ~7µs/frame (59% clone reduction) — minor in isolation but worth cleaning up."
    chunk_directory: glyph_single_styled_line
    depends_on: []
---

## Trigger

After the `syntax_highlight_issues` investigation fixed cache thrashing (exact-match → containment check) and symbol duplication (overlapping captures), scrolling through a syntax-highlighted large Rust file still consumes ~60% CPU. The previous investigation's H3 (O(n) `line_byte_range`) was left UNTESTED and there are additional allocation and redundant-work costs in the hot path.

## Success Criteria

- Identify all remaining performance bottlenecks in the scroll → highlight → render pipeline
- Quantify the cost of each bottleneck with profiling data
- Propose concrete fixes with measured expected impact

## Testable Hypotheses

### H1: `line_byte_range()` O(n) scanning dominates viewport highlighting cost for large files

- **Rationale**: `line_byte_range(line_idx)` iterates from byte 0 of the source for every call. During `highlight_viewport(start, end)`, it's called 62+ times (2 for viewport bounds + 60 for `build_line_from_captures`). Each call at line 4000 scans ~4000 lines of characters.
- **Test**: Measure `highlight_viewport()` at different scroll positions; if cost scales linearly with position, `line_byte_range()` is the cause.
- **Status**: **VERIFIED** — profiling confirms linear scaling:

  | Scroll position | `highlight_viewport` time | % of 8ms budget |
  |---|---|---|
  | Line 0 | 490µs | 6.1% |
  | Line 500 | 1,249µs | 15.6% |
  | Line 1000 | 1,926µs | 24.1% |
  | Line 2000 | 3,584µs | 44.8% |
  | Line 4000 | 6,032µs | **75.4%** |

  The 12× slowdown from line 0 to line 4000 directly correlates with O(n) scanning. Isolated measurement: 62 O(n) lookups at line 4000 = **4,432µs** vs 62 O(1) index lookups = **0.02µs** (220,000× faster).

### H2: `styled_line()` is called 3× per visible buffer line per frame, causing redundant clones

- **Rationale**: `glyph_buffer::update_from_buffer_with_wrap()` calls `view.styled_line(buffer_line)` in three separate phases (background, glyph, underline). Each clone copies a `Vec<Span>` with owned Strings.
- **Test**: Measure 60 vs 180 `highlight_line()` calls (cache hit path).
- **Status**: **VERIFIED but low impact** — profiling shows:
  - 60 calls (1× per line): 5.0µs/frame
  - 180 calls (3× per line): 12.3µs/frame
  - Savings: 7.3µs/frame (59% reduction in clone cost, but only 0.09% of 8ms budget)

### H3: String allocation per capture in `collect_captures_in_range()`

- **Rationale**: Each tree-sitter capture creates `(*name).to_string()` — a heap allocation. Hundreds of captures per viewport.
- **Test**: Implicit in viewport timing. Would need allocation profiler to isolate.
- **Status**: **PLAUSIBLE** — the ~490µs base cost at line 0 (where line_byte_range is cheap) includes capture collection + String allocation + line building. Not separately measured but worth optimizing.

### H4: `line_count()` scans entire source via `chars().filter()`

- **Rationale**: Called once per `highlight_viewport()` for clamping. Scans all characters.
- **Test**: Measure directly.
- **Status**: **VERIFIED** — profiling shows:
  - `chars().filter()`: **102µs** per call (1.3% of budget)
  - `bytes().filter()`: 26µs per call
  - Precomputed: 0µs (free from line offset index)

### H5: GPU buffer allocation every frame

- **Status**: **UNTESTED** — likely minor. Not profiled since it requires Metal context.

## Exploration Log

### 2026-02-22: Code path analysis

Traced the full scroll → render pipeline:

1. **Scroll event** → `buffer_target::handle_scroll()` → marks `DirtyRegion::FullViewport`
2. **Render loop** → `render_if_dirty()` → `render_with_editor()` → `update_glyph_buffer()`
3. **Glyph buffer** → `update_from_buffer_with_wrap()` iterates visible lines in 5 phases
4. **Each phase** calls `view.styled_line()` or `view.line_len()` per visible buffer line
5. **styled_line()** on `HighlightedBufferView` → `highlight_viewport()` + `highlight_line()`
6. **highlight_viewport()** (on cache miss) → `line_count()` + `line_byte_range()` × (2 + N) + `collect_captures_in_range()` + `build_line_from_captures()` × N

### 2026-02-22: Profiling (see `prototypes/profile_scroll.rs`)

Ran profiling test against `editor_state.rs` (5,911 lines, 224KB):

```
cargo test -p lite-edit-syntax --release --test profile_scroll -- --nocapture
```

**Key results:**

1. **`highlight_viewport` at line 4000: 6,032µs (75% of 8ms budget)** — confirms H1 as dominant bottleneck. Cost is ~12× higher than at line 0 (490µs), proving O(n) line scanning scales with position.

2. **Isolated O(n) vs O(1) line lookups**: 62 lookups at line 4000:
   - Current O(n): 4,432µs per batch
   - Proposed O(1): 0.02µs per batch
   - Building the index: 94µs one-time cost

3. **StyledLine clones**: 180 calls (current) = 12.3µs vs 60 calls (optimized) = 5.0µs. Savings of 7.3µs/frame — real but negligible vs H1.

4. **line_count() scan**: 102µs per call with `chars().filter()`. Eliminated for free by the line offset index.

**Where the 8ms budget goes at line 4000 (current):**

| Component | Cost | % of 8ms |
|---|---|---|
| `line_byte_range()` O(n) scanning | ~4,432µs | 55.4% |
| `line_count()` scanning | ~102µs | 1.3% |
| Tree-sitter query + capture collection | ~1,200µs | 15.0% |
| Line building from captures | ~300µs | 3.8% |
| StyledLine clones (180×) | ~12µs | 0.2% |
| **Total** | **~6,050µs** | **75.6%** |

**After proposed line offset index fix (estimated):**

| Component | Cost | % of 8ms |
|---|---|---|
| `line_byte_range()` via index | ~0µs | 0% |
| `line_count()` via index | ~0µs | 0% |
| Tree-sitter query + capture collection | ~1,200µs | 15.0% |
| Line building from captures | ~300µs | 3.8% |
| StyledLine clones (180×) | ~12µs | 0.2% |
| **Total** | **~1,512µs** | **18.9%** |

This would bring the highlight cost from 75% down to ~19% of the 8ms budget, making it position-independent.

## Findings

### Verified Findings (with profiling data)

1. **`line_byte_range()` O(n) scanning is the dominant bottleneck** — accounts for ~4.4ms of the 6ms viewport highlight cost at line 4000. Cost scales linearly with scroll position. A precomputed line offset index (94µs build cost) eliminates this entirely. (Evidence: profiling test, 62 lookups: 4,432µs → 0.02µs)

2. **`line_count()` adds 102µs per viewport highlight** — scans entire source. Eliminated for free by the line offset index. (Evidence: profiling test)

3. **StyledLine clone overhead is negligible** — 12µs/frame for 180 clones. Still worth fixing for cleanliness (saves 7µs) but not a performance priority. (Evidence: profiling test)

4. **Tree-sitter query + capture processing is the residual ~1.5ms cost** — this is the irreducible cost of syntax highlighting and represents 19% of the budget. Acceptable.

### Hypotheses/Opinions

- The line offset index fix alone should reduce scrolling CPU from ~60% to under 15% for large files.
- The remaining ~1.5ms query cost could be reduced further with capture name indices and binary search (chunk 2), but this is optimization of a healthy budget, not fixing a bottleneck.
- Chunk 3 (triple styled_line) is a code quality improvement more than a performance fix at 7µs savings.

## Proposed Chunks

1. **Precomputed line offset index in SyntaxHighlighter** (HIGH priority): Build `Vec<usize>` of line-start byte offsets → O(1) `line_byte_range()` and `line_count()`. **Expected savings: ~4.5ms/frame at line 4000** (55% of budget → 0%).
   - Priority: High
   - Dependencies: None

2. **Reduce per-frame allocations in highlight hot path** (LOW priority): Capture name indices, binary search, Vec reuse. **Expected savings: maybe 200-400µs** — the ~1.2ms query cost includes tree-sitter iteration which can't be eliminated.
   - Priority: Low
   - Dependencies: Chunk 1 (to measure incremental impact)

3. **Eliminate redundant styled_line() calls in glyph buffer** (LOW priority): Call once per line, share across phases. **Expected savings: 7µs/frame**.
   - Priority: Low
   - Dependencies: None

## Resolution Rationale

Profiling definitively confirmed H1 (`line_byte_range()` O(n) scanning) as the dominant bottleneck, responsible for ~4.4ms of the 6ms viewport highlight cost at line 4000 in a 5,911-line file. The fix (precomputed line offset index) is straightforward with measured build cost of 94µs and lookup cost near zero. H2 (triple styled_line) was verified but measured at only 7µs/frame — negligible. The investigation is SOLVED with clear, data-backed prioritization.
