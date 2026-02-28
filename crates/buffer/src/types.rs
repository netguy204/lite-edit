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
// Chunk: docs/chunks/buffer_view_trait - Added Default derive for BufferView::take_dirty()
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DirtyLines {
    /// No lines changed (e.g., cursor-only movement or no-op deletion).
    #[default]
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

// Chunk: docs/chunks/incremental_parse - Mutation result with edit event data
/// Result of a buffer mutation, containing both rendering and parsing info.
///
/// This bundles `DirtyLines` (for rendering) with `EditInfo` (for incremental
/// tree-sitter parsing), allowing mutation sites to update both the renderer
/// and the syntax highlighter in a single pass.
#[derive(Debug, Clone)]
pub struct MutationResult {
    /// Which lines need re-rendering
    pub dirty_lines: DirtyLines,
    /// Edit info for incremental parsing (None if no text was actually changed)
    pub edit_info: Option<EditInfo>,
}

impl MutationResult {
    /// Creates a new MutationResult with the given dirty lines and edit info.
    pub fn new(dirty_lines: DirtyLines, edit_info: Option<EditInfo>) -> Self {
        Self { dirty_lines, edit_info }
    }

    /// Creates a MutationResult with no edit (just dirty lines).
    pub fn dirty_only(dirty_lines: DirtyLines) -> Self {
        Self { dirty_lines, edit_info: None }
    }

    /// Creates a MutationResult indicating no change.
    pub fn none() -> Self {
        Self { dirty_lines: DirtyLines::None, edit_info: None }
    }
}

// Chunk: docs/chunks/incremental_parse - Byte-offset information for tree-sitter incremental parsing
/// Byte-offset information for a buffer edit.
///
/// This provides everything needed to construct a tree-sitter `InputEdit`:
/// - Byte offsets: where the edit happened in the byte stream
/// - Row/col positions: where the edit happened in line/column coordinates
///
/// Tree-sitter uses byte offsets because source code is stored as bytes,
/// but column positions are in characters for human-readable coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditInfo {
    /// Byte offset where the edit starts
    pub start_byte: usize,
    /// Byte offset where the old content ends
    pub old_end_byte: usize,
    /// Byte offset where the new content ends
    pub new_end_byte: usize,
    /// Row of the edit start position (0-indexed)
    pub start_row: usize,
    /// Column of the edit start position (0-indexed, in characters)
    pub start_col: usize,
    /// Row where the old content ends
    pub old_end_row: usize,
    /// Column where the old content ends
    pub old_end_col: usize,
    /// Row where the new content ends
    pub new_end_row: usize,
    /// Column where the new content ends
    pub new_end_col: usize,
}

impl EditInfo {
    /// Creates an EditInfo for an insertion at the given position.
    ///
    /// # Arguments
    ///
    /// * `start_byte` - Byte offset where the insertion occurs
    /// * `start_row` - Row where the insertion occurs
    /// * `start_col` - Column where the insertion occurs (in characters)
    /// * `inserted_bytes` - Number of bytes inserted
    /// * `end_row` - Row after the insertion
    /// * `end_col` - Column after the insertion
    pub fn for_insert(
        start_byte: usize,
        start_row: usize,
        start_col: usize,
        inserted_bytes: usize,
        end_row: usize,
        end_col: usize,
    ) -> Self {
        Self {
            start_byte,
            old_end_byte: start_byte,
            new_end_byte: start_byte + inserted_bytes,
            start_row,
            start_col,
            old_end_row: start_row,
            old_end_col: start_col,
            new_end_row: end_row,
            new_end_col: end_col,
        }
    }

    /// Creates an EditInfo for a deletion at the given position.
    ///
    /// # Arguments
    ///
    /// * `start_byte` - Byte offset where the deletion starts (after deletion)
    /// * `start_row` - Row where the deletion starts (after deletion)
    /// * `start_col` - Column where the deletion starts (after deletion)
    /// * `deleted_bytes` - Number of bytes deleted
    /// * `old_end_row` - Row where the deleted content ended
    /// * `old_end_col` - Column where the deleted content ended
    pub fn for_delete(
        start_byte: usize,
        start_row: usize,
        start_col: usize,
        deleted_bytes: usize,
        old_end_row: usize,
        old_end_col: usize,
    ) -> Self {
        Self {
            start_byte,
            old_end_byte: start_byte + deleted_bytes,
            new_end_byte: start_byte,
            start_row,
            start_col,
            old_end_row,
            old_end_col,
            new_end_row: start_row,
            new_end_col: start_col,
        }
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

    // ==================== MutationResult tests ====================

    #[test]
    fn mutation_result_new() {
        let result = MutationResult::new(
            DirtyLines::Single(5),
            Some(EditInfo::for_insert(10, 5, 3, 1, 5, 4)),
        );
        assert_eq!(result.dirty_lines, DirtyLines::Single(5));
        assert!(result.edit_info.is_some());
    }

    #[test]
    fn mutation_result_dirty_only() {
        let result = MutationResult::dirty_only(DirtyLines::Single(5));
        assert_eq!(result.dirty_lines, DirtyLines::Single(5));
        assert!(result.edit_info.is_none());
    }

    #[test]
    fn mutation_result_none() {
        let result = MutationResult::none();
        assert_eq!(result.dirty_lines, DirtyLines::None);
        assert!(result.edit_info.is_none());
    }

    // ==================== EditInfo tests ====================

    #[test]
    fn edit_info_for_insert_single_char() {
        // Insert 'x' (1 byte) at row 2, col 5, byte 25
        let info = EditInfo::for_insert(25, 2, 5, 1, 2, 6);

        assert_eq!(info.start_byte, 25);
        assert_eq!(info.old_end_byte, 25); // No old content replaced
        assert_eq!(info.new_end_byte, 26); // 25 + 1 byte
        assert_eq!(info.start_row, 2);
        assert_eq!(info.start_col, 5);
        assert_eq!(info.old_end_row, 2);
        assert_eq!(info.old_end_col, 5);
        assert_eq!(info.new_end_row, 2);
        assert_eq!(info.new_end_col, 6);
    }

    #[test]
    fn edit_info_for_insert_multibyte_char() {
        // Insert '日' (3 bytes) at row 0, col 0, byte 0
        let info = EditInfo::for_insert(0, 0, 0, 3, 0, 1);

        assert_eq!(info.start_byte, 0);
        assert_eq!(info.old_end_byte, 0);
        assert_eq!(info.new_end_byte, 3);
        assert_eq!(info.new_end_col, 1); // Column advances by 1 char
    }

    #[test]
    fn edit_info_for_insert_newline() {
        // Insert '\n' (1 byte) at row 3, col 10, byte 50
        let info = EditInfo::for_insert(50, 3, 10, 1, 4, 0);

        assert_eq!(info.start_row, 3);
        assert_eq!(info.start_col, 10);
        assert_eq!(info.new_end_row, 4); // New line
        assert_eq!(info.new_end_col, 0);
    }

    #[test]
    fn edit_info_for_delete_single_char() {
        // Delete 'x' (1 byte) - cursor was at row 2, col 6 before,
        // now at row 2, col 5 after
        let info = EditInfo::for_delete(25, 2, 5, 1, 2, 6);

        assert_eq!(info.start_byte, 25);
        assert_eq!(info.old_end_byte, 26); // 25 + 1 deleted byte
        assert_eq!(info.new_end_byte, 25); // Same as start
        assert_eq!(info.start_row, 2);
        assert_eq!(info.start_col, 5);
        assert_eq!(info.old_end_row, 2);
        assert_eq!(info.old_end_col, 6);
        assert_eq!(info.new_end_row, 2);
        assert_eq!(info.new_end_col, 5);
    }

    #[test]
    fn edit_info_for_delete_newline() {
        // Delete newline - joining line 3 with line 4
        // Before: row 3 col 10, after newline: row 4 col 0
        // After delete: row 3 col 10
        let info = EditInfo::for_delete(50, 3, 10, 1, 4, 0);

        assert_eq!(info.old_end_row, 4);
        assert_eq!(info.old_end_col, 0);
        assert_eq!(info.new_end_row, 3);
        assert_eq!(info.new_end_col, 10);
    }
}
