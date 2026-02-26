<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The `StyledLineCache` is a per-buffer cache that stores computed `StyledLine` results keyed by buffer line index. The cache sits between the renderer and the underlying `BufferView`, intercepting `styled_line()` calls and serving from cache when valid.

**Strategy:**
1. Create `StyledLineCache` struct in the editor crate (near `glyph_buffer.rs`)
2. Integrate the cache into the render path, invalidating based on `DirtyLines` from `take_dirty()`
3. Handle line insertion/deletion by shifting or invalidating cache entries from the mutation point

**Key insight from ARCHITECTURE_REVIEW.md (recommendation #3):**
> Every call to `styled_line(line_idx)` allocates a new `StyledLine` containing a `Vec<StyledSpan>`. For a 40-line viewport, that's 40 `Vec` allocations per frame — even for lines that haven't changed.

The cache eliminates ~90% of these allocations during typical editing (only the edited line changes).

**Why cache at the editor layer, not BufferView:**
- `BufferView` is a trait implemented by both `TextBuffer` (via `HighlightedBufferView`) and `TerminalBuffer`
- Each implementation has different costs for `styled_line()` — syntax highlighting for text buffers, cell iteration for terminals
- A single cache at the render layer handles both uniformly and avoids duplicating cache logic

**Testing approach (per TESTING_PHILOSOPHY.md):**
- Unit tests verify cache hit/miss behavior and invalidation correctness
- No visual/GPU testing needed — the cache is pure data structure manipulation
- Boundary tests: empty buffer, line insertion/deletion, `FromLineToEnd` invalidation

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk IMPLEMENTS a cache layer that integrates with the rendering pipeline. The cache invalidation is driven by `DirtyLines` which flows through the existing dirty tracking.

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport's `visible_range()` to determine which lines to cache. The cache benefits scroll performance by retaining lines from the previous viewport that overlap with the new viewport.

## Sequence

### Step 1: Define `StyledLineCache` struct

Create `crates/editor/src/styled_line_cache.rs` with:

```rust
// Chunk: docs/chunks/styled_line_cache - Cached styled lines per buffer line
use lite_edit_buffer::{DirtyLines, StyledLine};

pub struct StyledLineCache {
    /// Cached styled lines indexed by buffer line number.
    /// `None` means the line needs recomputation.
    lines: Vec<Option<StyledLine>>,
}
```

Core API:
- `new() -> Self` — creates empty cache
- `get(&self, line: usize) -> Option<&StyledLine>` — returns cached line if present
- `insert(&mut self, line: usize, styled: StyledLine)` — stores a computed line
- `invalidate(&mut self, dirty: &DirtyLines)` — clears affected entries based on dirty info
- `resize(&mut self, line_count: usize)` — adjusts cache size when buffer line count changes
- `clear(&mut self)` — clears all entries (for buffer switch / tab change)

Location: `crates/editor/src/styled_line_cache.rs`

### Step 2: Implement invalidation logic

The `invalidate()` method handles each `DirtyLines` variant:

```rust
pub fn invalidate(&mut self, dirty: &DirtyLines) {
    match dirty {
        DirtyLines::None => {}
        DirtyLines::Single(line) => {
            if *line < self.lines.len() {
                self.lines[*line] = None;
            }
        }
        DirtyLines::Range { from, to } => {
            for line in *from..*to {
                if line < self.lines.len() {
                    self.lines[line] = None;
                }
            }
        }
        DirtyLines::FromLineToEnd(line) => {
            // Truncate to invalidate all lines from this point
            if *line < self.lines.len() {
                self.lines.truncate(*line);
            }
        }
    }
}
```

**Line insertion/deletion handling:**
- `FromLineToEnd(line)` triggers truncation — all lines from `line` onward become invalid because line indices shift
- This is conservative but correct; shifted lines will be recomputed on next access

Location: `crates/editor/src/styled_line_cache.rs`

### Step 3: Write unit tests for cache behavior

Test cases (TDD — write tests first, then implementation):

1. **Cache miss returns None:**
   ```rust
   #[test]
   fn test_cache_miss_returns_none() {
       let cache = StyledLineCache::new();
       assert!(cache.get(0).is_none());
   }
   ```

2. **Cache hit after insert:**
   ```rust
   #[test]
   fn test_cache_hit_after_insert() {
       let mut cache = StyledLineCache::new();
       cache.resize(10);
       cache.insert(5, StyledLine::plain("hello"));
       assert_eq!(cache.get(5).unwrap(), &StyledLine::plain("hello"));
   }
   ```

3. **Single line invalidation:**
   ```rust
   #[test]
   fn test_invalidate_single() {
       let mut cache = StyledLineCache::new();
       cache.resize(10);
       cache.insert(5, StyledLine::plain("hello"));
       cache.invalidate(&DirtyLines::Single(5));
       assert!(cache.get(5).is_none());
   }
   ```

4. **Range invalidation:**
   ```rust
   #[test]
   fn test_invalidate_range() {
       let mut cache = StyledLineCache::new();
       cache.resize(10);
       for i in 0..10 { cache.insert(i, StyledLine::plain("line")); }
       cache.invalidate(&DirtyLines::Range { from: 3, to: 7 });
       assert!(cache.get(2).is_some());  // before range
       assert!(cache.get(3).is_none());  // in range
       assert!(cache.get(6).is_none());  // in range
       assert!(cache.get(7).is_some());  // after range (exclusive end)
   }
   ```

5. **FromLineToEnd truncation:**
   ```rust
   #[test]
   fn test_invalidate_from_line_to_end() {
       let mut cache = StyledLineCache::new();
       cache.resize(10);
       for i in 0..10 { cache.insert(i, StyledLine::plain("line")); }
       cache.invalidate(&DirtyLines::FromLineToEnd(5));
       assert!(cache.get(4).is_some());  // before
       assert!(cache.get(5).is_none());  // at point
       assert_eq!(cache.len(), 5);        // truncated
   }
   ```

6. **Clear on buffer switch:**
   ```rust
   #[test]
   fn test_clear() {
       let mut cache = StyledLineCache::new();
       cache.resize(10);
       cache.insert(5, StyledLine::plain("hello"));
       cache.clear();
       assert!(cache.get(5).is_none());
       assert_eq!(cache.len(), 0);
   }
   ```

Location: `crates/editor/src/styled_line_cache.rs` (in `#[cfg(test)]` module)

### Step 4: Integrate cache into `GlyphBuffer`

The cache will be owned by the code that iterates visible lines — currently in `GlyphBuffer::update_glyphs_wrapped()` and `GlyphBuffer::update_glyphs()`.

Modify `GlyphBuffer` to:
1. Add a `StyledLineCache` field
2. Before the render pass, call `cache.invalidate(dirty_lines)` with the dirty info from `BufferView::take_dirty()`
3. Replace direct `view.styled_line(line)` calls with a pattern that checks cache first:

```rust
fn get_or_compute_styled_line(
    &mut self,
    line: usize,
    view: &dyn BufferView,
) -> Option<&StyledLine> {
    if self.styled_line_cache.get(line).is_none() {
        if let Some(styled) = view.styled_line(line) {
            self.styled_line_cache.insert(line, styled);
        } else {
            return None;
        }
    }
    self.styled_line_cache.get(line)
}
```

**Key change:** The pre-collected `styled_lines: Vec<Option<_>>` pattern currently used in `update_glyphs_wrapped()` will be replaced with cache lookups. This eliminates the per-frame Vec allocation for this collection as well.

Location: `crates/editor/src/glyph_buffer.rs`

### Step 5: Handle buffer switch / tab change

When a tab is switched, the cache must be cleared or replaced. Options:
1. **Per-tab cache:** Each tab owns its own `StyledLineCache`
2. **Single cache with clear:** One cache in `GlyphBuffer`, cleared on tab switch

Option 2 is simpler since the cache is already in `GlyphBuffer`. Add a `clear_styled_line_cache()` method called from tab switch logic.

The tab switch detection can happen in `Renderer::render_pane()` by tracking the previous buffer identity (e.g., via a buffer ID or pointer comparison).

Location: `crates/editor/src/glyph_buffer.rs`, `crates/editor/src/renderer.rs`

### Step 6: Handle resize (line count changes)

When the buffer's line count changes (lines added/deleted), call `cache.resize(new_line_count)`:
- If growing: extend with `None` entries
- If shrinking: truncate

This is already handled implicitly by `FromLineToEnd` invalidation, but explicit resize ensures the cache stays sized appropriately.

Location: `crates/editor/src/styled_line_cache.rs`, called from `glyph_buffer.rs`

### Step 7: Add perf instrumentation (optional but recommended)

Under `#[cfg(feature = "perf-instrumentation")]`, track:
- Cache hit/miss counts per frame
- Recomputation time vs. cache hit time

This validates the expected 90% hit rate during typical editing.

```rust
#[cfg(feature = "perf-instrumentation")]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
}
```

Location: `crates/editor/src/styled_line_cache.rs`

### Step 8: Integration test with HighlightedBufferView

Verify that the cache integrates correctly with syntax-highlighted buffers:
1. Type a character — only that line should be recomputed
2. Scroll — overlapping lines should be cache hits
3. Insert newline (splitting a line) — current line + all below should be invalidated

This can be a manual test or an automated integration test using `EditorContext`.

Location: `crates/editor/tests/` (new integration test file)

## Dependencies

- **typescript_highlight_layering** (listed in `created_after`): Ensures the highlighting infrastructure is stable before adding caching on top.

No new external crates needed — the cache uses only standard library types.

## Risks and Open Questions

1. **Memory overhead:** The cache holds `StyledLine` clones for all visible lines plus scroll overlap. For a 40-line viewport with ~10 spans/line averaging 20 chars, that's ~40 * 10 * 20 * 4 = ~32KB per tab. This is acceptable.

2. **Terminal buffer frequency:** Terminal buffers can change many lines per PTY read. The cache still provides value when only partial grid updates occur (e.g., cursor movement, single-line echo). For full-screen redraws (vim, less), the cache provides no benefit but adds no overhead (cache is invalidated and rebuilt).

3. **Cache identity on buffer switch:** Need to ensure the cache is properly invalidated when switching tabs. A stale cache would cause visual artifacts. Solution: Clear cache on tab switch or track buffer identity.

4. **Integration with pane-per-frame caching:** The `Renderer` currently operates on a per-pane basis. The cache should be per-pane or per-buffer, not global to the renderer. The plan attaches it to `GlyphBuffer`, which is already per-pane.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->