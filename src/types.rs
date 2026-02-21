// Chunk: docs/chunks/text_buffer - Text buffer data structure with gap buffer backing

/// Position in the buffer as (line, column) where both are 0-indexed.
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
}
