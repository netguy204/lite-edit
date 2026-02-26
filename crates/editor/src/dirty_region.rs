// Subsystem: docs/subsystems/viewport_scroll - Viewport mapping & scroll arithmetic
// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
//!
//! Dirty region tracking for screen-space rendering
//!
//! This module provides the `DirtyRegion` enum, which represents which parts of the
//! screen need to be re-rendered. It's the screen-space complement to the buffer's
//! `DirtyLines` enum.
//!
//! The key distinction:
//! - `DirtyLines` (in buffer crate) tracks buffer-coordinate changes
//! - `DirtyRegion` tracks screen-coordinate changes
//!
//! The `Viewport` converts `DirtyLines` to `DirtyRegion` based on scroll offset.

/// Screen-space dirty region for rendering
///
/// This enum indicates which parts of the viewport need to be re-rendered.
/// Screen coordinates are 0-indexed from the top of the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyRegion {
    /// No screen lines changed (e.g., mutation outside viewport)
    None,
    /// A range of screen lines changed [from, to)
    Lines { from: usize, to: usize },
    /// The entire viewport needs re-rendering
    /// Used when scroll offset changes or when changes span large regions
    FullViewport,
}

impl DirtyRegion {
    /// Returns true if no screen region is dirty
    pub fn is_none(&self) -> bool {
        matches!(self, DirtyRegion::None)
    }

    /// Returns true if any screen region needs re-rendering
    pub fn is_dirty(&self) -> bool {
        !self.is_none()
    }

    /// Merges another dirty region into this one, producing the smallest
    /// region that covers both.
    ///
    /// # Merge semantics:
    /// - `None` is the identity element
    /// - `Any + FullViewport → FullViewport`
    /// - `Lines(a,b) + Lines(c,d) → Lines(min(a,c), max(b,d))`
    pub fn merge(&mut self, other: DirtyRegion) {
        *self = match (&*self, &other) {
            // None is the identity element
            (DirtyRegion::None, _) => other,
            (_, DirtyRegion::None) => return,

            // FullViewport absorbs everything
            (DirtyRegion::FullViewport, _) | (_, DirtyRegion::FullViewport) => {
                DirtyRegion::FullViewport
            }

            // Two line ranges: merge to cover both
            (DirtyRegion::Lines { from: a, to: b }, DirtyRegion::Lines { from: c, to: d }) => {
                DirtyRegion::Lines {
                    from: (*a).min(*c),
                    to: (*b).max(*d),
                }
            }
        };
    }

    /// Creates a dirty region for a single screen line
    pub fn single_line(line: usize) -> Self {
        DirtyRegion::Lines {
            from: line,
            to: line + 1,
        }
    }

    /// Creates a dirty region for a range of screen lines [from, to)
    pub fn line_range(from: usize, to: usize) -> Self {
        if from >= to {
            DirtyRegion::None
        } else {
            DirtyRegion::Lines { from, to }
        }
    }
}

impl Default for DirtyRegion {
    fn default() -> Self {
        DirtyRegion::None
    }
}

// =============================================================================
// InvalidationKind
// =============================================================================

/// Invalidation category for rendering optimization.
///
/// Different invalidation kinds allow the renderer to skip work:
/// - Content-only frames skip pane rect recalculation
/// - Layout frames trigger full pane rect recomputation
/// - Overlay frames render overlay layer without re-rendering content (future optimization)
///
/// The priority order (from highest to lowest) is: Layout > Overlay > Content > None.
/// When merging invalidation kinds, the higher priority kind wins.
// Chunk: docs/chunks/invalidation_separation - Separate invalidation kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InvalidationKind {
    /// No invalidation
    #[default]
    None,
    /// Content changed within existing layout (typing, cursor blink, PTY output)
    /// Contains the screen-space dirty region for partial redraw optimization
    Content(DirtyRegion),
    /// Overlay changed (find bar, selector, dialog appeared/changed)
    /// Currently treated as Content for simplicity; future optimization could
    /// render overlay layer only
    Overlay,
    /// Layout changed (pane resize, split/unsplit, tab bar change, workspace switch)
    /// Implies full content re-render after layout recalculation
    Layout,
}

impl InvalidationKind {
    /// Returns true if no invalidation is needed
    pub fn is_none(&self) -> bool {
        matches!(self, InvalidationKind::None)
    }

    /// Returns true if any invalidation is needed (alias for `!is_none()`)
    pub fn is_dirty(&self) -> bool {
        !self.is_none()
    }

    /// Returns true if this invalidation requires pane layout recalculation
    ///
    /// Only Layout invalidation requires recalculating pane rects. Content and
    /// Overlay can reuse cached pane rects from the previous frame.
    pub fn requires_layout_recalc(&self) -> bool {
        matches!(self, InvalidationKind::Layout)
    }

    /// Returns the content region if this is a Content invalidation
    pub fn content_region(&self) -> Option<DirtyRegion> {
        match self {
            InvalidationKind::Content(region) => Some(*region),
            _ => None,
        }
    }

    /// Merges another invalidation kind into this one.
    ///
    /// # Merge semantics:
    /// - `None` is the identity element
    /// - `Layout` absorbs everything (highest priority)
    /// - `Overlay` absorbs `Content` but yields to `Layout`
    /// - `Content` merges underlying `DirtyRegion` values
    pub fn merge(&mut self, other: InvalidationKind) {
        *self = match (&*self, &other) {
            // None is the identity element
            (InvalidationKind::None, _) => other,
            (_, InvalidationKind::None) => return,

            // Layout absorbs everything (highest priority)
            (InvalidationKind::Layout, _) | (_, InvalidationKind::Layout) => {
                InvalidationKind::Layout
            }

            // Overlay absorbs Content
            (InvalidationKind::Overlay, InvalidationKind::Content(_)) => InvalidationKind::Overlay,
            (InvalidationKind::Content(_), InvalidationKind::Overlay) => InvalidationKind::Overlay,
            (InvalidationKind::Overlay, InvalidationKind::Overlay) => InvalidationKind::Overlay,

            // Two Content values: merge underlying DirtyRegion
            (InvalidationKind::Content(a), InvalidationKind::Content(b)) => {
                let mut merged = *a;
                merged.merge(*b);
                InvalidationKind::Content(merged)
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Merge: identity ====================

    #[test]
    fn merge_none_with_lines() {
        let mut d = DirtyRegion::None;
        d.merge(DirtyRegion::Lines { from: 3, to: 7 });
        assert_eq!(d, DirtyRegion::Lines { from: 3, to: 7 });
    }

    #[test]
    fn merge_lines_with_none() {
        let mut d = DirtyRegion::Lines { from: 3, to: 7 };
        d.merge(DirtyRegion::None);
        assert_eq!(d, DirtyRegion::Lines { from: 3, to: 7 });
    }

    #[test]
    fn merge_none_with_none() {
        let mut d = DirtyRegion::None;
        d.merge(DirtyRegion::None);
        assert_eq!(d, DirtyRegion::None);
    }

    // ==================== Merge: FullViewport absorbs ====================

    #[test]
    fn merge_full_with_lines() {
        let mut d = DirtyRegion::FullViewport;
        d.merge(DirtyRegion::Lines { from: 3, to: 7 });
        assert_eq!(d, DirtyRegion::FullViewport);
    }

    #[test]
    fn merge_lines_with_full() {
        let mut d = DirtyRegion::Lines { from: 3, to: 7 };
        d.merge(DirtyRegion::FullViewport);
        assert_eq!(d, DirtyRegion::FullViewport);
    }

    #[test]
    fn merge_full_with_full() {
        let mut d = DirtyRegion::FullViewport;
        d.merge(DirtyRegion::FullViewport);
        assert_eq!(d, DirtyRegion::FullViewport);
    }

    #[test]
    fn merge_none_with_full() {
        let mut d = DirtyRegion::None;
        d.merge(DirtyRegion::FullViewport);
        assert_eq!(d, DirtyRegion::FullViewport);
    }

    // ==================== Merge: Lines ====================

    #[test]
    fn merge_overlapping_lines() {
        let mut d = DirtyRegion::Lines { from: 3, to: 7 };
        d.merge(DirtyRegion::Lines { from: 5, to: 10 });
        assert_eq!(d, DirtyRegion::Lines { from: 3, to: 10 });
    }

    #[test]
    fn merge_disjoint_lines() {
        let mut d = DirtyRegion::Lines { from: 3, to: 5 };
        d.merge(DirtyRegion::Lines { from: 8, to: 12 });
        assert_eq!(d, DirtyRegion::Lines { from: 3, to: 12 });
    }

    #[test]
    fn merge_nested_lines() {
        let mut d = DirtyRegion::Lines { from: 2, to: 10 };
        d.merge(DirtyRegion::Lines { from: 4, to: 7 });
        assert_eq!(d, DirtyRegion::Lines { from: 2, to: 10 });
    }

    #[test]
    fn merge_same_lines() {
        let mut d = DirtyRegion::Lines { from: 3, to: 7 };
        d.merge(DirtyRegion::Lines { from: 3, to: 7 });
        assert_eq!(d, DirtyRegion::Lines { from: 3, to: 7 });
    }

    // ==================== Helper constructors ====================

    #[test]
    fn test_single_line() {
        assert_eq!(
            DirtyRegion::single_line(5),
            DirtyRegion::Lines { from: 5, to: 6 }
        );
    }

    #[test]
    fn test_line_range() {
        assert_eq!(
            DirtyRegion::line_range(3, 7),
            DirtyRegion::Lines { from: 3, to: 7 }
        );
    }

    #[test]
    fn test_line_range_empty() {
        assert_eq!(DirtyRegion::line_range(5, 5), DirtyRegion::None);
        assert_eq!(DirtyRegion::line_range(7, 3), DirtyRegion::None);
    }

    // ==================== Predicates ====================

    #[test]
    fn test_is_none() {
        assert!(DirtyRegion::None.is_none());
        assert!(!DirtyRegion::FullViewport.is_none());
        assert!(!DirtyRegion::Lines { from: 0, to: 1 }.is_none());
    }

    #[test]
    fn test_is_dirty() {
        assert!(!DirtyRegion::None.is_dirty());
        assert!(DirtyRegion::FullViewport.is_dirty());
        assert!(DirtyRegion::Lines { from: 0, to: 1 }.is_dirty());
    }

    // ==================== Multi-event sequences ====================

    #[test]
    fn merge_multiple_events() {
        let mut d = DirtyRegion::None;
        d.merge(DirtyRegion::single_line(3));
        d.merge(DirtyRegion::single_line(5));
        d.merge(DirtyRegion::single_line(4));
        assert_eq!(d, DirtyRegion::Lines { from: 3, to: 6 });
    }

    #[test]
    fn merge_then_full_viewport() {
        let mut d = DirtyRegion::None;
        d.merge(DirtyRegion::single_line(3));
        d.merge(DirtyRegion::Lines { from: 7, to: 10 });
        d.merge(DirtyRegion::FullViewport);
        assert_eq!(d, DirtyRegion::FullViewport);
    }
}

// =============================================================================
// InvalidationKind tests
// =============================================================================

#[cfg(test)]
mod invalidation_tests {
    use super::*;

    // ==================== Merge: None identity ====================

    #[test]
    fn merge_none_with_content() {
        let mut inv = InvalidationKind::None;
        inv.merge(InvalidationKind::Content(DirtyRegion::single_line(5)));
        assert!(matches!(inv, InvalidationKind::Content(_)));
    }

    #[test]
    fn merge_content_with_none() {
        let mut inv = InvalidationKind::Content(DirtyRegion::single_line(5));
        inv.merge(InvalidationKind::None);
        assert!(matches!(inv, InvalidationKind::Content(_)));
    }

    #[test]
    fn merge_none_with_none() {
        let mut inv = InvalidationKind::None;
        inv.merge(InvalidationKind::None);
        assert_eq!(inv, InvalidationKind::None);
    }

    // ==================== Merge: Layout absorbs all ====================

    #[test]
    fn merge_content_with_layout() {
        let mut inv = InvalidationKind::Content(DirtyRegion::single_line(5));
        inv.merge(InvalidationKind::Layout);
        assert_eq!(inv, InvalidationKind::Layout);
    }

    #[test]
    fn merge_layout_with_content() {
        let mut inv = InvalidationKind::Layout;
        inv.merge(InvalidationKind::Content(DirtyRegion::FullViewport));
        assert_eq!(inv, InvalidationKind::Layout);
    }

    #[test]
    fn merge_layout_absorbs_overlay() {
        let mut inv = InvalidationKind::Layout;
        inv.merge(InvalidationKind::Overlay);
        assert_eq!(inv, InvalidationKind::Layout);
    }

    #[test]
    fn merge_overlay_with_layout() {
        let mut inv = InvalidationKind::Overlay;
        inv.merge(InvalidationKind::Layout);
        assert_eq!(inv, InvalidationKind::Layout);
    }

    #[test]
    fn merge_layout_absorbs_all() {
        let mut inv = InvalidationKind::Layout;
        inv.merge(InvalidationKind::Content(DirtyRegion::FullViewport));
        inv.merge(InvalidationKind::Overlay);
        assert_eq!(inv, InvalidationKind::Layout);
    }

    // ==================== Merge: Overlay absorbs Content ====================

    #[test]
    fn merge_content_with_overlay() {
        let mut inv = InvalidationKind::Content(DirtyRegion::single_line(5));
        inv.merge(InvalidationKind::Overlay);
        assert_eq!(inv, InvalidationKind::Overlay);
    }

    #[test]
    fn merge_overlay_with_content() {
        let mut inv = InvalidationKind::Overlay;
        inv.merge(InvalidationKind::Content(DirtyRegion::single_line(5)));
        assert_eq!(inv, InvalidationKind::Overlay);
    }

    #[test]
    fn merge_overlay_with_overlay() {
        let mut inv = InvalidationKind::Overlay;
        inv.merge(InvalidationKind::Overlay);
        assert_eq!(inv, InvalidationKind::Overlay);
    }

    // ==================== Merge: Content regions ====================

    #[test]
    fn merge_content_regions() {
        let mut inv = InvalidationKind::Content(DirtyRegion::Lines { from: 0, to: 5 });
        inv.merge(InvalidationKind::Content(DirtyRegion::Lines { from: 3, to: 8 }));
        assert_eq!(
            inv,
            InvalidationKind::Content(DirtyRegion::Lines { from: 0, to: 8 })
        );
    }

    #[test]
    fn merge_content_regions_disjoint() {
        let mut inv = InvalidationKind::Content(DirtyRegion::Lines { from: 0, to: 2 });
        inv.merge(InvalidationKind::Content(DirtyRegion::Lines { from: 5, to: 8 }));
        assert_eq!(
            inv,
            InvalidationKind::Content(DirtyRegion::Lines { from: 0, to: 8 })
        );
    }

    // ==================== requires_layout_recalc ====================

    #[test]
    fn requires_layout_recalc_none() {
        assert!(!InvalidationKind::None.requires_layout_recalc());
    }

    #[test]
    fn requires_layout_recalc_content() {
        assert!(!InvalidationKind::Content(DirtyRegion::FullViewport).requires_layout_recalc());
    }

    #[test]
    fn requires_layout_recalc_overlay() {
        // Overlay doesn't require layout recalc - it just paints over existing content
        assert!(!InvalidationKind::Overlay.requires_layout_recalc());
    }

    #[test]
    fn requires_layout_recalc_layout() {
        assert!(InvalidationKind::Layout.requires_layout_recalc());
    }

    // ==================== content_region ====================

    #[test]
    fn content_region_extraction() {
        let inv = InvalidationKind::Content(DirtyRegion::Lines { from: 3, to: 7 });
        assert_eq!(inv.content_region(), Some(DirtyRegion::Lines { from: 3, to: 7 }));
    }

    #[test]
    fn content_region_from_layout() {
        let inv = InvalidationKind::Layout;
        assert_eq!(inv.content_region(), None);
    }

    #[test]
    fn content_region_from_none() {
        let inv = InvalidationKind::None;
        assert_eq!(inv.content_region(), None);
    }

    #[test]
    fn content_region_from_overlay() {
        let inv = InvalidationKind::Overlay;
        assert_eq!(inv.content_region(), None);
    }

    // ==================== is_none / is_dirty ====================

    #[test]
    fn test_is_none() {
        assert!(InvalidationKind::None.is_none());
        assert!(!InvalidationKind::Content(DirtyRegion::FullViewport).is_none());
        assert!(!InvalidationKind::Layout.is_none());
        assert!(!InvalidationKind::Overlay.is_none());
    }

    #[test]
    fn test_is_dirty() {
        assert!(!InvalidationKind::None.is_dirty());
        assert!(InvalidationKind::Content(DirtyRegion::FullViewport).is_dirty());
        assert!(InvalidationKind::Layout.is_dirty());
        assert!(InvalidationKind::Overlay.is_dirty());
    }
}
