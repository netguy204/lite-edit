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
