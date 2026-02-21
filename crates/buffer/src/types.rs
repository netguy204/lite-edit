// Chunk: docs/chunks/text_buffer - Text buffer data structure with gap buffer backing

/// Position in the buffer as (line, column) where both are 0-indexed.
// Chunk: docs/chunks/text_selection_model - Selection anchor and range API (added Ord)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Position {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare by line first, then by column
        match self.line.cmp(&other.line) {
            std::cmp::Ordering::Equal => self.col.cmp(&other.col),
            ord => ord,
        }
    }
}

/// Information about which lines were dirtied by a mutation.
/// Used by the render loop to compute DirtyRegion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirtyLines {
    /// No lines changed (e.g., cursor-only movement or no-op deletion).
    None,
    /// A single line changed (most insertions, deletions within a line).
    Single(usize),
    /// A range of lines changed [from, to). Used when lines are joined/split.
    Range { from: usize, to: usize },
    /// Everything from a line to the end of the buffer changed.
    /// Used when a line split pushes all subsequent lines down,
    /// or when joining lines pulls subsequent lines up.
    FromLineToEnd(usize),
}

impl DirtyLines {
    /// Returns true if no lines were dirtied.
    pub fn is_none(&self) -> bool {
        matches!(self, DirtyLines::None)
    }

    /// Returns the starting line of the dirty region, if any.
    pub fn start_line(&self) -> Option<usize> {
        match self {
            DirtyLines::None => None,
            DirtyLines::Single(line) => Some(*line),
            DirtyLines::Range { from, .. } => Some(*from),
            DirtyLines::FromLineToEnd(line) => Some(*line),
        }
    }

    /// Merges another dirty region into this one, producing the smallest
    /// region that covers both.
    ///
    /// This is used in the drain-all-then-render loop: each event produces
    /// a `DirtyLines`, and they are merged together so we render once at
    /// the end covering everything that changed.
    pub fn merge(&mut self, other: DirtyLines) {
        *self = match (&*self, &other) {
            // None is the identity element
            (DirtyLines::None, _) => other,
            (_, DirtyLines::None) => return,

            // FromLineToEnd absorbs everything — take the earlier start
            (DirtyLines::FromLineToEnd(a), DirtyLines::FromLineToEnd(b)) => {
                DirtyLines::FromLineToEnd((*a).min(*b))
            }
            (DirtyLines::FromLineToEnd(a), other) | (other, DirtyLines::FromLineToEnd(a)) => {
                let b = other.start_line().unwrap();
                DirtyLines::FromLineToEnd((*a).min(b))
            }

            // Two singles
            (DirtyLines::Single(a), DirtyLines::Single(b)) => {
                if a == b {
                    DirtyLines::Single(*a)
                } else {
                    DirtyLines::Range {
                        from: (*a).min(*b),
                        to: (*a).max(*b) + 1,
                    }
                }
            }

            // Single + Range or Range + Single
            (DirtyLines::Single(a), DirtyLines::Range { from, to })
            | (DirtyLines::Range { from, to }, DirtyLines::Single(a)) => DirtyLines::Range {
                from: (*from).min(*a),
                to: (*to).max(*a + 1),
            },

            // Two ranges
            (DirtyLines::Range { from: a, to: b }, DirtyLines::Range { from: c, to: d }) => {
                DirtyLines::Range {
                    from: (*a).min(*c),
                    to: (*b).max(*d),
                }
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Merge: identity ====================

    #[test]
    fn merge_none_with_single() {
        let mut d = DirtyLines::None;
        d.merge(DirtyLines::Single(5));
        assert_eq!(d, DirtyLines::Single(5));
    }

    #[test]
    fn merge_single_with_none() {
        let mut d = DirtyLines::Single(5);
        d.merge(DirtyLines::None);
        assert_eq!(d, DirtyLines::Single(5));
    }

    #[test]
    fn merge_none_with_none() {
        let mut d = DirtyLines::None;
        d.merge(DirtyLines::None);
        assert_eq!(d, DirtyLines::None);
    }

    // ==================== Merge: singles ====================

    #[test]
    fn merge_same_single() {
        let mut d = DirtyLines::Single(3);
        d.merge(DirtyLines::Single(3));
        assert_eq!(d, DirtyLines::Single(3));
    }

    #[test]
    fn merge_adjacent_singles() {
        let mut d = DirtyLines::Single(3);
        d.merge(DirtyLines::Single(4));
        assert_eq!(d, DirtyLines::Range { from: 3, to: 5 });
    }

    #[test]
    fn merge_distant_singles() {
        let mut d = DirtyLines::Single(3);
        d.merge(DirtyLines::Single(10));
        assert_eq!(d, DirtyLines::Range { from: 3, to: 11 });
    }

    #[test]
    fn merge_singles_reversed_order() {
        let mut d = DirtyLines::Single(10);
        d.merge(DirtyLines::Single(3));
        assert_eq!(d, DirtyLines::Range { from: 3, to: 11 });
    }

    // ==================== Merge: ranges ====================

    #[test]
    fn merge_overlapping_ranges() {
        let mut d = DirtyLines::Range { from: 3, to: 7 };
        d.merge(DirtyLines::Range { from: 5, to: 10 });
        assert_eq!(d, DirtyLines::Range { from: 3, to: 10 });
    }

    #[test]
    fn merge_disjoint_ranges() {
        let mut d = DirtyLines::Range { from: 3, to: 5 };
        d.merge(DirtyLines::Range { from: 8, to: 12 });
        assert_eq!(d, DirtyLines::Range { from: 3, to: 12 });
    }

    #[test]
    fn merge_nested_ranges() {
        let mut d = DirtyLines::Range { from: 2, to: 10 };
        d.merge(DirtyLines::Range { from: 4, to: 7 });
        assert_eq!(d, DirtyLines::Range { from: 2, to: 10 });
    }

    // ==================== Merge: single + range ====================

    #[test]
    fn merge_single_extends_range_below() {
        let mut d = DirtyLines::Range { from: 5, to: 10 };
        d.merge(DirtyLines::Single(2));
        assert_eq!(d, DirtyLines::Range { from: 2, to: 10 });
    }

    #[test]
    fn merge_single_extends_range_above() {
        let mut d = DirtyLines::Range { from: 5, to: 10 };
        d.merge(DirtyLines::Single(15));
        assert_eq!(d, DirtyLines::Range { from: 5, to: 16 });
    }

    #[test]
    fn merge_single_inside_range() {
        let mut d = DirtyLines::Range { from: 5, to: 10 };
        d.merge(DirtyLines::Single(7));
        assert_eq!(d, DirtyLines::Range { from: 5, to: 10 });
    }

    // ==================== Merge: FromLineToEnd ====================

    #[test]
    fn merge_from_line_to_end_takes_earlier() {
        let mut d = DirtyLines::FromLineToEnd(5);
        d.merge(DirtyLines::FromLineToEnd(3));
        assert_eq!(d, DirtyLines::FromLineToEnd(3));
    }

    #[test]
    fn merge_from_line_to_end_absorbs_single() {
        let mut d = DirtyLines::Single(2);
        d.merge(DirtyLines::FromLineToEnd(5));
        assert_eq!(d, DirtyLines::FromLineToEnd(2));
    }

    #[test]
    fn merge_from_line_to_end_absorbs_range() {
        let mut d = DirtyLines::Range { from: 3, to: 7 };
        d.merge(DirtyLines::FromLineToEnd(5));
        assert_eq!(d, DirtyLines::FromLineToEnd(3));
    }

    #[test]
    fn merge_none_with_from_line_to_end() {
        let mut d = DirtyLines::None;
        d.merge(DirtyLines::FromLineToEnd(5));
        assert_eq!(d, DirtyLines::FromLineToEnd(5));
    }

    // ==================== Merge: multi-event sequences ====================

    #[test]
    fn merge_three_events_typing_on_same_line() {
        // Three characters typed on line 5
        let mut d = DirtyLines::None;
        d.merge(DirtyLines::Single(5));
        d.merge(DirtyLines::Single(5));
        d.merge(DirtyLines::Single(5));
        assert_eq!(d, DirtyLines::Single(5));
    }

    #[test]
    fn merge_insert_then_newline() {
        // Type a char on line 3, then press Enter (splits, dirtying 3+)
        let mut d = DirtyLines::None;
        d.merge(DirtyLines::Single(3));
        d.merge(DirtyLines::FromLineToEnd(3));
        assert_eq!(d, DirtyLines::FromLineToEnd(3));
    }

    #[test]
    fn merge_edits_on_different_lines() {
        // Delete on line 2, then insert on line 8
        let mut d = DirtyLines::None;
        d.merge(DirtyLines::Single(2));
        d.merge(DirtyLines::Single(8));
        assert_eq!(d, DirtyLines::Range { from: 2, to: 9 });
    }
}
