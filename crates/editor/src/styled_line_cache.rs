// Chunk: docs/chunks/styled_line_cache - Cached styled lines per buffer line
//!
//! Styled line cache for reducing per-frame allocations.
//!
//! This module provides `StyledLineCache`, a per-buffer cache that stores
//! computed `StyledLine` results keyed by buffer line index. The cache sits
//! between the renderer and the underlying `BufferView`, intercepting
//! `styled_line()` calls and serving from cache when valid.
//!
//! # Performance Impact
//!
//! Every frame, the renderer calls `styled_line(line_idx)` for every visible
//! line (~40 lines), each allocating a new `StyledLine` containing a
//! `Vec<StyledSpan>`. During typical editing, only 1 line changes per
//! keystroke — yet all 40 are recomputed and reallocated.
//!
//! The cache eliminates ~90% of these allocations during typical editing:
//! - On a keystroke, only the edited line is recomputed
//! - On scroll, lines that overlap between old and new viewports are cache hits
//!
//! # Invalidation
//!
//! The cache is invalidated based on `DirtyLines` from `BufferView::take_dirty()`:
//! - `DirtyLines::None`: No invalidation
//! - `DirtyLines::Single(line)`: Invalidate that single line
//! - `DirtyLines::Range { from, to }`: Invalidate lines in `[from, to)`
//! - `DirtyLines::FromLineToEnd(line)`: Truncate cache at that line (handles
//!   line insertion/deletion which shifts all subsequent lines)

use lite_edit_buffer::{DirtyLines, StyledLine};

/// Cache for computed `StyledLine` results, keyed by buffer line index.
///
/// The cache stores `Option<StyledLine>` for each line, where `None` indicates
/// the line needs recomputation. The cache automatically grows to accommodate
/// new lines but never shrinks automatically — use `resize()` to shrink.
pub struct StyledLineCache {
    /// Cached styled lines indexed by buffer line number.
    /// `None` means the line needs recomputation.
    lines: Vec<Option<StyledLine>>,
}

impl StyledLineCache {
    /// Creates a new empty cache.
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Returns the number of lines the cache can hold.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Returns a reference to the cached styled line, if present.
    ///
    /// Returns `None` if the line index is out of bounds or the line
    /// has been invalidated (needs recomputation).
    pub fn get(&self, line: usize) -> Option<&StyledLine> {
        self.lines.get(line).and_then(|opt| opt.as_ref())
    }

    /// Stores a computed styled line in the cache.
    ///
    /// If the line index is beyond the current cache size, the cache is
    /// automatically extended with `None` entries.
    pub fn insert(&mut self, line: usize, styled: StyledLine) {
        // Extend cache if needed
        if line >= self.lines.len() {
            self.lines.resize(line + 1, None);
        }
        self.lines[line] = Some(styled);
    }

    /// Invalidates cache entries based on dirty line information.
    ///
    /// This method handles each `DirtyLines` variant appropriately:
    /// - `None`: No action
    /// - `Single(line)`: Clears that single line
    /// - `Range { from, to }`: Clears lines in `[from, to)`
    /// - `FromLineToEnd(line)`: Truncates cache at that line, since line
    ///   insertion/deletion shifts all subsequent line indices
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
                // Truncate to invalidate all lines from this point onward.
                // This is necessary because line insertion/deletion shifts
                // all subsequent line indices, making cached entries invalid.
                if *line < self.lines.len() {
                    self.lines.truncate(*line);
                }
            }
        }
    }

    /// Resizes the cache to the given line count.
    ///
    /// - If growing: extends with `None` entries (lines need computation)
    /// - If shrinking: truncates, discarding cached lines beyond the new size
    ///
    /// Call this when the buffer's line count changes to keep the cache
    /// appropriately sized.
    pub fn resize(&mut self, line_count: usize) {
        self.lines.resize(line_count, None);
    }

    /// Clears all cached entries.
    ///
    /// Call this on buffer switch / tab change to ensure stale cache
    /// entries from a previous buffer don't cause visual artifacts.
    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

impl Default for StyledLineCache {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Performance Instrumentation
// =============================================================================

/// Statistics about cache performance for debugging and tuning.
#[cfg(feature = "perf-instrumentation")]
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Number of cache hits (served from cache without recomputation)
    pub hits: usize,
    /// Number of cache misses (required recomputation)
    pub misses: usize,
}

#[cfg(feature = "perf-instrumentation")]
impl CacheStats {
    /// Creates a new empty stats struct.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a cache hit.
    pub fn record_hit(&mut self) {
        self.hits += 1;
    }

    /// Records a cache miss.
    pub fn record_miss(&mut self) {
        self.misses += 1;
    }

    /// Returns the cache hit rate as a percentage (0.0 to 100.0).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }

    /// Resets all counters to zero.
    pub fn reset(&mut self) {
        self.hits = 0;
        self.misses = 0;
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Basic Operations ====================

    #[test]
    fn test_cache_miss_returns_none() {
        let cache = StyledLineCache::new();
        assert!(cache.get(0).is_none());
        assert!(cache.get(100).is_none());
    }

    #[test]
    fn test_cache_hit_after_insert() {
        let mut cache = StyledLineCache::new();
        cache.resize(10);
        cache.insert(5, StyledLine::plain("hello"));
        assert_eq!(cache.get(5).unwrap(), &StyledLine::plain("hello"));
    }

    #[test]
    fn test_insert_auto_extends() {
        let mut cache = StyledLineCache::new();
        assert_eq!(cache.len(), 0);
        cache.insert(5, StyledLine::plain("hello"));
        assert_eq!(cache.len(), 6);
        assert!(cache.get(5).is_some());
        // Lines 0-4 should be None
        for i in 0..5 {
            assert!(cache.get(i).is_none());
        }
    }

    #[test]
    fn test_overwrite_existing() {
        let mut cache = StyledLineCache::new();
        cache.resize(10);
        cache.insert(5, StyledLine::plain("first"));
        cache.insert(5, StyledLine::plain("second"));
        assert_eq!(cache.get(5).unwrap(), &StyledLine::plain("second"));
    }

    // ==================== Invalidation ====================

    #[test]
    fn test_invalidate_none() {
        let mut cache = StyledLineCache::new();
        cache.resize(10);
        cache.insert(5, StyledLine::plain("hello"));
        cache.invalidate(&DirtyLines::None);
        assert!(cache.get(5).is_some());
    }

    #[test]
    fn test_invalidate_single() {
        let mut cache = StyledLineCache::new();
        cache.resize(10);
        cache.insert(5, StyledLine::plain("hello"));
        cache.insert(6, StyledLine::plain("world"));
        cache.invalidate(&DirtyLines::Single(5));
        assert!(cache.get(5).is_none());
        assert!(cache.get(6).is_some()); // Not affected
    }

    #[test]
    fn test_invalidate_single_out_of_bounds() {
        let mut cache = StyledLineCache::new();
        cache.resize(5);
        cache.insert(2, StyledLine::plain("hello"));
        // Should not panic
        cache.invalidate(&DirtyLines::Single(100));
        assert!(cache.get(2).is_some()); // Not affected
    }

    #[test]
    fn test_invalidate_range() {
        let mut cache = StyledLineCache::new();
        cache.resize(10);
        for i in 0..10 {
            cache.insert(i, StyledLine::plain("line"));
        }
        cache.invalidate(&DirtyLines::Range { from: 3, to: 7 });
        assert!(cache.get(2).is_some()); // before range
        assert!(cache.get(3).is_none()); // in range (start)
        assert!(cache.get(4).is_none()); // in range
        assert!(cache.get(5).is_none()); // in range
        assert!(cache.get(6).is_none()); // in range (end - 1)
        assert!(cache.get(7).is_some()); // after range (exclusive end)
        assert!(cache.get(8).is_some()); // after range
    }

    #[test]
    fn test_invalidate_range_partial_out_of_bounds() {
        let mut cache = StyledLineCache::new();
        cache.resize(5);
        for i in 0..5 {
            cache.insert(i, StyledLine::plain("line"));
        }
        // Range extends beyond cache size
        cache.invalidate(&DirtyLines::Range { from: 3, to: 100 });
        assert!(cache.get(2).is_some()); // before range
        assert!(cache.get(3).is_none()); // in range
        assert!(cache.get(4).is_none()); // in range
    }

    #[test]
    fn test_invalidate_from_line_to_end() {
        let mut cache = StyledLineCache::new();
        cache.resize(10);
        for i in 0..10 {
            cache.insert(i, StyledLine::plain("line"));
        }
        cache.invalidate(&DirtyLines::FromLineToEnd(5));
        assert!(cache.get(4).is_some()); // before truncation point
        assert!(cache.get(5).is_none()); // at truncation point (gone)
        assert!(cache.get(6).is_none()); // after truncation point (gone)
        assert_eq!(cache.len(), 5); // truncated
    }

    #[test]
    fn test_invalidate_from_line_to_end_at_start() {
        let mut cache = StyledLineCache::new();
        cache.resize(10);
        for i in 0..10 {
            cache.insert(i, StyledLine::plain("line"));
        }
        cache.invalidate(&DirtyLines::FromLineToEnd(0));
        assert_eq!(cache.len(), 0); // completely truncated
    }

    #[test]
    fn test_invalidate_from_line_to_end_beyond_cache() {
        let mut cache = StyledLineCache::new();
        cache.resize(5);
        cache.insert(2, StyledLine::plain("hello"));
        // Truncation point beyond cache size should be no-op
        cache.invalidate(&DirtyLines::FromLineToEnd(100));
        assert_eq!(cache.len(), 5); // unchanged
        assert!(cache.get(2).is_some());
    }

    // ==================== Clear and Resize ====================

    #[test]
    fn test_clear() {
        let mut cache = StyledLineCache::new();
        cache.resize(10);
        cache.insert(5, StyledLine::plain("hello"));
        cache.clear();
        assert!(cache.get(5).is_none());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_resize_grow() {
        let mut cache = StyledLineCache::new();
        cache.resize(5);
        cache.insert(2, StyledLine::plain("hello"));
        cache.resize(10);
        assert_eq!(cache.len(), 10);
        assert!(cache.get(2).is_some()); // preserved
        assert!(cache.get(8).is_none()); // new entry is None
    }

    #[test]
    fn test_resize_shrink() {
        let mut cache = StyledLineCache::new();
        cache.resize(10);
        cache.insert(2, StyledLine::plain("hello"));
        cache.insert(8, StyledLine::plain("world"));
        cache.resize(5);
        assert_eq!(cache.len(), 5);
        assert!(cache.get(2).is_some()); // preserved
        assert!(cache.get(8).is_none()); // truncated away
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_empty_cache() {
        let cache = StyledLineCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_default() {
        let cache = StyledLineCache::default();
        assert!(cache.is_empty());
    }

    // ==================== Perf Stats (conditional) ====================

    #[cfg(feature = "perf-instrumentation")]
    mod perf_tests {
        use super::*;

        #[test]
        fn test_cache_stats_hit_rate() {
            let mut stats = CacheStats::new();
            stats.record_hit();
            stats.record_hit();
            stats.record_hit();
            stats.record_miss();
            assert!((stats.hit_rate() - 75.0).abs() < 0.001);
        }

        #[test]
        fn test_cache_stats_empty() {
            let stats = CacheStats::new();
            assert_eq!(stats.hit_rate(), 0.0);
        }

        #[test]
        fn test_cache_stats_reset() {
            let mut stats = CacheStats::new();
            stats.record_hit();
            stats.record_miss();
            stats.reset();
            assert_eq!(stats.hits, 0);
            assert_eq!(stats.misses, 0);
        }
    }
}
