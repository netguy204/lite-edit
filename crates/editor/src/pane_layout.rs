// Chunk: docs/chunks/tiling_tree_model - Binary pane layout tree data model
//!
//! Binary pane layout tree data model for tiling window manager-style pane splitting.
//!
//! This module implements a binary space partitioning model for pane layout:
//! - **Leaf nodes** (`Pane`) own a `Vec<Tab>` and tab-management state
//! - **Internal nodes** (`Split`) have a direction (Horizontal/Vertical), a ratio, and two children
//!
//! The binary tree structure gives unambiguous directional targeting: "move right" means
//! "go to the other child of the nearest horizontal-split ancestor."
//!
//! # Example Tree Structure
//!
//! ```text
//! HSplit(ratio=0.5)
//! ├── Pane[A]
//! └── VSplit(ratio=0.5)
//!     ├── Pane[B]
//!     └── Pane[C]
//! ```
//!
//! This creates a layout where Pane A is on the left half, and Panes B and C
//! share the right half (B on top, C on bottom).

use crate::workspace::{Tab, WorkspaceId};

// =============================================================================
// ID Types
// =============================================================================

/// Unique identifier for a pane within a layout tree.
pub type PaneId = u64;

/// Generates a new unique pane ID.
///
/// This follows the same pattern as `Editor::gen_tab_id()`.
pub fn gen_pane_id(next_id: &mut u64) -> PaneId {
    let id = *next_id;
    *next_id += 1;
    id
}

// =============================================================================
// Direction Types
// =============================================================================

/// The direction of a split in the pane tree.
///
/// - `Horizontal`: Children are placed side-by-side (left/right)
/// - `Vertical`: Children are stacked (top/bottom)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// Children are placed side-by-side (first=left, second=right)
    Horizontal,
    /// Children are stacked vertically (first=top, second=bottom)
    Vertical,
}

impl SplitDirection {
    /// Returns true if the given direction is compatible with this split direction.
    ///
    /// - Horizontal splits are compatible with Left/Right
    /// - Vertical splits are compatible with Up/Down
    pub fn is_compatible(&self, direction: Direction) -> bool {
        match self {
            SplitDirection::Horizontal => {
                matches!(direction, Direction::Left | Direction::Right)
            }
            SplitDirection::Vertical => {
                matches!(direction, Direction::Up | Direction::Down)
            }
        }
    }
}

/// A cardinal direction for pane navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    /// Returns true if this direction goes toward the second child of a compatible split.
    ///
    /// - Right and Down go toward the second child
    /// - Left and Up go toward the first child
    pub fn is_toward_second(&self) -> bool {
        matches!(self, Direction::Right | Direction::Down)
    }

    /// Returns the opposite direction.
    pub fn opposite(&self) -> Direction {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }

    /// Returns the split direction that is compatible with this direction.
    pub fn to_split_direction(&self) -> SplitDirection {
        match self {
            Direction::Left | Direction::Right => SplitDirection::Horizontal,
            Direction::Up | Direction::Down => SplitDirection::Vertical,
        }
    }
}

// =============================================================================
// MoveTarget
// =============================================================================

/// The result of searching for a target pane in a direction.
///
/// When navigating from one pane in a direction:
/// - `ExistingPane(id)`: There is an existing pane in that direction
/// - `SplitPane(id, direction)`: No existing pane; a new split would be created
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveTarget {
    /// Target is an existing pane
    ExistingPane(PaneId),
    /// No target exists; would require splitting the given pane in the given direction
    SplitPane(PaneId, Direction),
}

// =============================================================================
// Pane
// =============================================================================

/// A pane containing tabs.
///
/// Each pane is a leaf node in the pane tree and owns its own tab collection.
/// The tab management API mirrors `Workspace` for consistency.
#[derive(Debug)]
pub struct Pane {
    /// Unique identifier for this pane
    pub id: PaneId,
    /// The workspace this pane belongs to
    pub workspace_id: WorkspaceId,
    /// The tabs in this pane
    pub tabs: Vec<Tab>,
    /// Index of the currently active tab
    pub active_tab: usize,
    /// Horizontal scroll offset for tab bar overflow (in pixels)
    pub tab_bar_view_offset: f32,
}

impl Pane {
    /// Creates a new empty pane.
    pub fn new(id: PaneId, workspace_id: WorkspaceId) -> Self {
        Self {
            id,
            workspace_id,
            tabs: Vec::new(),
            active_tab: 0,
            tab_bar_view_offset: 0.0,
        }
    }

    /// Adds a tab to the pane and makes it active.
    pub fn add_tab(&mut self, tab: Tab) {
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Closes a tab at the given index, returning the removed tab.
    ///
    /// Returns `None` if the index is out of bounds.
    /// After closing, the active tab is adjusted to remain valid.
    pub fn close_tab(&mut self, index: usize) -> Option<Tab> {
        if index >= self.tabs.len() {
            return None;
        }

        let removed = self.tabs.remove(index);

        // Adjust active_tab to remain valid
        if !self.tabs.is_empty() {
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            } else if self.active_tab > index {
                self.active_tab = self.active_tab.saturating_sub(1);
            }
        } else {
            self.active_tab = 0;
        }

        Some(removed)
    }

    /// Switches to the tab at the given index.
    ///
    /// Does nothing if the index is out of bounds. When switching to a new tab,
    /// clears its unread state.
    pub fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
            self.tabs[index].clear_unread();
        }
    }

    /// Returns a reference to the active tab, if any.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    /// Returns a mutable reference to the active tab, if any.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    /// Returns the number of tabs in this pane.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Returns true if this pane has no tabs.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }
}

// =============================================================================
// PaneRect
// =============================================================================

/// A rectangle representing a pane's position and size on screen.
///
/// This is the output of layout calculation.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneRect {
    /// X position (left edge)
    pub x: f32,
    /// Y position (top edge)
    pub y: f32,
    /// Width
    pub width: f32,
    /// Height
    pub height: f32,
    /// The pane this rectangle belongs to
    pub pane_id: PaneId,
}

impl PaneRect {
    /// Returns true if the given point is inside this rectangle.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

// =============================================================================
// PaneLayoutNode
// =============================================================================

/// A node in the pane layout tree.
///
/// The tree is a binary space partitioning structure where:
/// - `Leaf` nodes contain panes with tabs
/// - `Split` nodes divide space between two children
#[derive(Debug)]
pub enum PaneLayoutNode {
    /// A leaf node containing a pane
    Leaf(Pane),
    /// A split node with two children
    Split {
        /// The direction of the split
        direction: SplitDirection,
        /// The ratio of space given to the first child (0.0 to 1.0)
        ratio: f32,
        /// The first child (left for Horizontal, top for Vertical)
        first: Box<PaneLayoutNode>,
        /// The second child (right for Horizontal, bottom for Vertical)
        second: Box<PaneLayoutNode>,
    },
}

impl PaneLayoutNode {
    /// Creates a leaf node containing the given pane.
    pub fn single_pane(pane: Pane) -> Self {
        PaneLayoutNode::Leaf(pane)
    }

    /// Returns the number of panes (leaf nodes) in this tree.
    pub fn pane_count(&self) -> usize {
        match self {
            PaneLayoutNode::Leaf(_) => 1,
            PaneLayoutNode::Split { first, second, .. } => {
                first.pane_count() + second.pane_count()
            }
        }
    }

    /// Returns a flat list of all panes in the tree.
    pub fn all_panes(&self) -> Vec<&Pane> {
        match self {
            PaneLayoutNode::Leaf(pane) => vec![pane],
            PaneLayoutNode::Split { first, second, .. } => {
                let mut panes = first.all_panes();
                panes.extend(second.all_panes());
                panes
            }
        }
    }

    /// Returns a flat list of mutable references to all panes in the tree.
    pub fn all_panes_mut(&mut self) -> Vec<&mut Pane> {
        match self {
            PaneLayoutNode::Leaf(pane) => vec![pane],
            PaneLayoutNode::Split { first, second, .. } => {
                let mut panes = first.all_panes_mut();
                panes.extend(second.all_panes_mut());
                panes
            }
        }
    }

    /// Finds a pane by ID and returns a reference to it.
    pub fn get_pane(&self, pane_id: PaneId) -> Option<&Pane> {
        match self {
            PaneLayoutNode::Leaf(pane) => {
                if pane.id == pane_id {
                    Some(pane)
                } else {
                    None
                }
            }
            PaneLayoutNode::Split { first, second, .. } => {
                first.get_pane(pane_id).or_else(|| second.get_pane(pane_id))
            }
        }
    }

    /// Finds a pane by ID and returns a mutable reference to it.
    pub fn get_pane_mut(&mut self, pane_id: PaneId) -> Option<&mut Pane> {
        match self {
            PaneLayoutNode::Leaf(pane) => {
                if pane.id == pane_id {
                    Some(pane)
                } else {
                    None
                }
            }
            PaneLayoutNode::Split { first, second, .. } => {
                // Try first child, then second
                if first.contains_pane(pane_id) {
                    first.get_pane_mut(pane_id)
                } else {
                    second.get_pane_mut(pane_id)
                }
            }
        }
    }

    /// Returns true if this subtree contains the given pane ID.
    pub fn contains_pane(&self, pane_id: PaneId) -> bool {
        match self {
            PaneLayoutNode::Leaf(pane) => pane.id == pane_id,
            PaneLayoutNode::Split { first, second, .. } => {
                first.contains_pane(pane_id) || second.contains_pane(pane_id)
            }
        }
    }

    /// Finds the nearest leaf in a subtree when entering from a direction.
    ///
    /// For example, when entering from the Left (i.e., moving Right into this subtree),
    /// returns the leftmost leaf. This is used for directional navigation.
    ///
    /// - Entering from Left: return leftmost leaf
    /// - Entering from Right: return rightmost leaf
    /// - Entering from Up: return topmost leaf
    /// - Entering from Down: return bottommost leaf
    pub fn nearest_leaf_toward(&self, direction: Direction) -> PaneId {
        match self {
            PaneLayoutNode::Leaf(pane) => pane.id,
            PaneLayoutNode::Split {
                direction: split_dir,
                first,
                second,
                ..
            } => {
                // Determine which child to descend into based on direction
                let go_to_second = if split_dir.is_compatible(direction) {
                    // Direction aligns with split axis
                    direction.is_toward_second()
                } else {
                    // Direction perpendicular to split axis - default to first
                    false
                };

                if go_to_second {
                    second.nearest_leaf_toward(direction)
                } else {
                    first.nearest_leaf_toward(direction)
                }
            }
        }
    }

    /// Finds the target pane when moving from a pane in a direction.
    ///
    /// Returns:
    /// - `ExistingPane(id)`: The pane in that direction
    /// - `SplitPane(id, direction)`: No target exists; would require creating a new pane
    pub fn find_target_in_direction(&self, pane_id: PaneId, direction: Direction) -> MoveTarget {
        // Build path from root to the pane
        let path = match self.path_to_pane(pane_id) {
            Some(p) => p,
            None => return MoveTarget::SplitPane(pane_id, direction),
        };

        // Walk up from the pane looking for a compatible split ancestor
        // that we can cross to find a target
        for segment in path.iter().rev() {
            if let PathNode::Split {
                split_direction,
                first,
                second,
                ..
            } = segment
            {
                // Check if this split is compatible with our movement direction
                if split_direction.is_compatible(direction) {
                    // Determine which child we're in
                    let in_first = first.contains_pane(pane_id);

                    // Check if we can cross to the other child
                    if in_first && direction.is_toward_second() {
                        // We're in first, want to go to second
                        let target_id = second.nearest_leaf_toward(direction.opposite());
                        return MoveTarget::ExistingPane(target_id);
                    } else if !in_first && !direction.is_toward_second() {
                        // We're in second, want to go to first
                        let target_id = first.nearest_leaf_toward(direction.opposite());
                        return MoveTarget::ExistingPane(target_id);
                    }
                    // Otherwise, this split doesn't help - continue up
                }
            }
        }

        // No compatible ancestor found - would need to split
        MoveTarget::SplitPane(pane_id, direction)
    }

    /// Builds a path from the root to a pane.
    ///
    /// Returns `None` if the pane is not found.
    fn path_to_pane(&self, pane_id: PaneId) -> Option<Vec<PathNode<'_>>> {
        match self {
            PaneLayoutNode::Leaf(pane) => {
                if pane.id == pane_id {
                    Some(vec![PathNode::Leaf(pane_id)])
                } else {
                    None
                }
            }
            PaneLayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                // Try first child
                if let Some(mut path) = first.path_to_pane(pane_id) {
                    path.insert(
                        0,
                        PathNode::Split {
                            split_direction: *direction,
                            ratio: *ratio,
                            first: first.as_ref(),
                            second: second.as_ref(),
                        },
                    );
                    return Some(path);
                }

                // Try second child
                if let Some(mut path) = second.path_to_pane(pane_id) {
                    path.insert(
                        0,
                        PathNode::Split {
                            split_direction: *direction,
                            ratio: *ratio,
                            first: first.as_ref(),
                            second: second.as_ref(),
                        },
                    );
                    return Some(path);
                }

                None
            }
        }
    }
}

/// A segment of the path from root to a pane.
///
/// Used internally by `path_to_pane` and `find_target_in_direction`.
#[derive(Debug)]
enum PathNode<'a> {
    #[allow(dead_code)] // PaneId is used for Debug representation
    Leaf(PaneId),
    Split {
        split_direction: SplitDirection,
        #[allow(dead_code)]
        ratio: f32,
        first: &'a PaneLayoutNode,
        second: &'a PaneLayoutNode,
    },
}

// =============================================================================
// Layout Calculation
// =============================================================================

/// Calculates the screen rectangles for all panes in a layout tree.
///
/// # Arguments
///
/// * `bounds` - The bounding rectangle `(x, y, width, height)` for the entire layout
/// * `node` - The root of the pane tree
///
/// # Returns
///
/// A vector of `PaneRect` values, one for each pane in the tree.
pub fn calculate_pane_rects(
    bounds: (f32, f32, f32, f32),
    node: &PaneLayoutNode,
) -> Vec<PaneRect> {
    let (x, y, width, height) = bounds;

    match node {
        PaneLayoutNode::Leaf(pane) => {
            vec![PaneRect {
                x,
                y,
                width,
                height,
                pane_id: pane.id,
            }]
        }
        PaneLayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let (first_bounds, second_bounds) = match direction {
                SplitDirection::Horizontal => {
                    let first_width = width * ratio;
                    let second_width = width * (1.0 - ratio);
                    (
                        (x, y, first_width, height),
                        (x + first_width, y, second_width, height),
                    )
                }
                SplitDirection::Vertical => {
                    let first_height = height * ratio;
                    let second_height = height * (1.0 - ratio);
                    (
                        (x, y, width, first_height),
                        (x, y + first_height, width, second_height),
                    )
                }
            };

            let mut rects = calculate_pane_rects(first_bounds, first);
            rects.extend(calculate_pane_rects(second_bounds, second));
            rects
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use lite_edit_buffer::TextBuffer;

    const TEST_LINE_HEIGHT: f32 = 16.0;
    const EPSILON: f32 = 0.001;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < EPSILON
    }

    // Helper to create a simple file tab for testing
    fn test_tab(id: u64) -> Tab {
        crate::workspace::Tab::new_file(
            id,
            TextBuffer::new(),
            format!("Tab {}", id),
            None,
            TEST_LINE_HEIGHT,
        )
    }

    // Helper to create a pane with a given ID
    fn test_pane(id: PaneId) -> Pane {
        Pane::new(id, 0) // workspace_id = 0 for testing
    }

    // =========================================================================
    // Direction and SplitDirection Tests (Step 1)
    // =========================================================================

    #[test]
    fn test_split_direction_is_compatible() {
        assert!(SplitDirection::Horizontal.is_compatible(Direction::Left));
        assert!(SplitDirection::Horizontal.is_compatible(Direction::Right));
        assert!(!SplitDirection::Horizontal.is_compatible(Direction::Up));
        assert!(!SplitDirection::Horizontal.is_compatible(Direction::Down));

        assert!(SplitDirection::Vertical.is_compatible(Direction::Up));
        assert!(SplitDirection::Vertical.is_compatible(Direction::Down));
        assert!(!SplitDirection::Vertical.is_compatible(Direction::Left));
        assert!(!SplitDirection::Vertical.is_compatible(Direction::Right));
    }

    #[test]
    fn test_direction_is_toward_second() {
        assert!(!Direction::Left.is_toward_second());
        assert!(Direction::Right.is_toward_second());
        assert!(!Direction::Up.is_toward_second());
        assert!(Direction::Down.is_toward_second());
    }

    #[test]
    fn test_direction_opposite() {
        assert_eq!(Direction::Left.opposite(), Direction::Right);
        assert_eq!(Direction::Right.opposite(), Direction::Left);
        assert_eq!(Direction::Up.opposite(), Direction::Down);
        assert_eq!(Direction::Down.opposite(), Direction::Up);
    }

    #[test]
    fn test_direction_to_split_direction() {
        assert_eq!(Direction::Left.to_split_direction(), SplitDirection::Horizontal);
        assert_eq!(Direction::Right.to_split_direction(), SplitDirection::Horizontal);
        assert_eq!(Direction::Up.to_split_direction(), SplitDirection::Vertical);
        assert_eq!(Direction::Down.to_split_direction(), SplitDirection::Vertical);
    }

    // =========================================================================
    // Pane Tab Management Tests (Step 2 & 13)
    // =========================================================================

    #[test]
    fn test_pane_new() {
        let pane = Pane::new(1, 100);
        assert_eq!(pane.id, 1);
        assert_eq!(pane.workspace_id, 100);
        assert!(pane.tabs.is_empty());
        assert_eq!(pane.active_tab, 0);
        assert_eq!(pane.tab_bar_view_offset, 0.0);
    }

    #[test]
    fn test_pane_add_tab() {
        let mut pane = test_pane(1);
        let tab = test_tab(1);
        pane.add_tab(tab);

        assert_eq!(pane.tab_count(), 1);
        assert_eq!(pane.active_tab, 0);
        assert!(!pane.is_empty());
    }

    #[test]
    fn test_pane_add_multiple_tabs() {
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));
        pane.add_tab(test_tab(2));
        pane.add_tab(test_tab(3));

        assert_eq!(pane.tab_count(), 3);
        assert_eq!(pane.active_tab, 2); // Last added is active
    }

    #[test]
    fn test_pane_close_tab() {
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));
        pane.add_tab(test_tab(2));
        pane.add_tab(test_tab(3));
        // active_tab = 2

        let removed = pane.close_tab(1);
        assert!(removed.is_some());
        assert_eq!(pane.tab_count(), 2);
        assert_eq!(pane.active_tab, 1); // Adjusted from 2 to 1
    }

    #[test]
    fn test_pane_close_tab_at_end() {
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));
        pane.add_tab(test_tab(2));
        // active_tab = 1

        let removed = pane.close_tab(1);
        assert!(removed.is_some());
        assert_eq!(pane.tab_count(), 1);
        assert_eq!(pane.active_tab, 0);
    }

    #[test]
    fn test_pane_close_last_tab() {
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));

        let removed = pane.close_tab(0);
        assert!(removed.is_some());
        assert!(pane.is_empty());
        assert_eq!(pane.active_tab, 0);
    }

    #[test]
    fn test_pane_close_tab_invalid_index() {
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));

        let removed = pane.close_tab(10);
        assert!(removed.is_none());
        assert_eq!(pane.tab_count(), 1);
    }

    #[test]
    fn test_pane_switch_tab() {
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));
        pane.add_tab(test_tab(2));
        // active_tab = 1

        pane.switch_tab(0);
        assert_eq!(pane.active_tab, 0);
    }

    #[test]
    fn test_pane_switch_tab_invalid() {
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));
        // active_tab = 0

        pane.switch_tab(10);
        assert_eq!(pane.active_tab, 0); // Unchanged
    }

    #[test]
    fn test_pane_switch_tab_clears_unread() {
        let mut pane = test_pane(1);
        let tab1 = test_tab(1);
        let mut tab2 = test_tab(2);
        tab2.mark_unread();

        pane.add_tab(tab1);
        pane.add_tab(tab2);
        pane.switch_tab(0); // Switch to first tab

        // Second tab should still be unread
        assert!(pane.tabs[1].unread);

        // Switch to second tab - should clear unread
        pane.switch_tab(1);
        assert!(!pane.tabs[1].unread);
    }

    #[test]
    fn test_pane_active_tab() {
        let mut pane = test_pane(1);
        assert!(pane.active_tab().is_none());

        pane.add_tab(test_tab(1));
        assert!(pane.active_tab().is_some());
        assert_eq!(pane.active_tab().unwrap().id, 1);
    }

    #[test]
    fn test_pane_active_tab_mut() {
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));

        let tab = pane.active_tab_mut().unwrap();
        tab.dirty = true;

        assert!(pane.tabs[0].dirty);
    }

    // =========================================================================
    // PaneRect Tests (Step 3)
    // =========================================================================

    #[test]
    fn test_pane_rect_contains() {
        let rect = PaneRect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
            pane_id: 1,
        };

        // Inside
        assert!(rect.contains(10.0, 20.0));
        assert!(rect.contains(50.0, 40.0));
        assert!(rect.contains(109.9, 69.9));

        // Outside
        assert!(!rect.contains(9.9, 20.0));
        assert!(!rect.contains(10.0, 19.9));
        assert!(!rect.contains(110.0, 20.0));
        assert!(!rect.contains(10.0, 70.0));
    }

    // =========================================================================
    // Layout Calculation Tests (Step 5 & 6)
    // =========================================================================

    #[test]
    fn test_single_pane_fills_bounds() {
        let pane = test_pane(1);
        let tree = PaneLayoutNode::single_pane(pane);

        let rects = calculate_pane_rects((0.0, 0.0, 800.0, 600.0), &tree);

        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].pane_id, 1);
        assert!(approx_eq(rects[0].x, 0.0));
        assert!(approx_eq(rects[0].y, 0.0));
        assert!(approx_eq(rects[0].width, 800.0));
        assert!(approx_eq(rects[0].height, 600.0));
    }

    #[test]
    fn test_horizontal_split_divides_width() {
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),
            second: Box::new(PaneLayoutNode::Leaf(test_pane(2))),
        };

        let rects = calculate_pane_rects((0.0, 0.0, 800.0, 600.0), &tree);

        assert_eq!(rects.len(), 2);

        // First pane (left half)
        let first = rects.iter().find(|r| r.pane_id == 1).unwrap();
        assert!(approx_eq(first.x, 0.0));
        assert!(approx_eq(first.y, 0.0));
        assert!(approx_eq(first.width, 400.0));
        assert!(approx_eq(first.height, 600.0));

        // Second pane (right half)
        let second = rects.iter().find(|r| r.pane_id == 2).unwrap();
        assert!(approx_eq(second.x, 400.0));
        assert!(approx_eq(second.y, 0.0));
        assert!(approx_eq(second.width, 400.0));
        assert!(approx_eq(second.height, 600.0));
    }

    #[test]
    fn test_vertical_split_divides_height() {
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),
            second: Box::new(PaneLayoutNode::Leaf(test_pane(2))),
        };

        let rects = calculate_pane_rects((0.0, 0.0, 800.0, 600.0), &tree);

        assert_eq!(rects.len(), 2);

        // First pane (top half)
        let first = rects.iter().find(|r| r.pane_id == 1).unwrap();
        assert!(approx_eq(first.x, 0.0));
        assert!(approx_eq(first.y, 0.0));
        assert!(approx_eq(first.width, 800.0));
        assert!(approx_eq(first.height, 300.0));

        // Second pane (bottom half)
        let second = rects.iter().find(|r| r.pane_id == 2).unwrap();
        assert!(approx_eq(second.x, 0.0));
        assert!(approx_eq(second.y, 300.0));
        assert!(approx_eq(second.width, 800.0));
        assert!(approx_eq(second.height, 300.0));
    }

    #[test]
    fn test_nested_splits() {
        // HSplit(Pane[A], VSplit(Pane[B], Pane[C]))
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))), // A
            second: Box::new(PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(test_pane(2))),  // B
                second: Box::new(PaneLayoutNode::Leaf(test_pane(3))), // C
            }),
        };

        let rects = calculate_pane_rects((0.0, 0.0, 800.0, 600.0), &tree);

        assert_eq!(rects.len(), 3);

        // A: left half
        let a = rects.iter().find(|r| r.pane_id == 1).unwrap();
        assert!(approx_eq(a.x, 0.0));
        assert!(approx_eq(a.y, 0.0));
        assert!(approx_eq(a.width, 400.0));
        assert!(approx_eq(a.height, 600.0));

        // B: top-right quarter
        let b = rects.iter().find(|r| r.pane_id == 2).unwrap();
        assert!(approx_eq(b.x, 400.0));
        assert!(approx_eq(b.y, 0.0));
        assert!(approx_eq(b.width, 400.0));
        assert!(approx_eq(b.height, 300.0));

        // C: bottom-right quarter
        let c = rects.iter().find(|r| r.pane_id == 3).unwrap();
        assert!(approx_eq(c.x, 400.0));
        assert!(approx_eq(c.y, 300.0));
        assert!(approx_eq(c.width, 400.0));
        assert!(approx_eq(c.height, 300.0));
    }

    #[test]
    fn test_non_default_ratios() {
        // 30/70 horizontal split
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.3,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),
            second: Box::new(PaneLayoutNode::Leaf(test_pane(2))),
        };

        let rects = calculate_pane_rects((0.0, 0.0, 1000.0, 600.0), &tree);

        assert_eq!(rects.len(), 2);

        let first = rects.iter().find(|r| r.pane_id == 1).unwrap();
        assert!(approx_eq(first.width, 300.0));

        let second = rects.iter().find(|r| r.pane_id == 2).unwrap();
        assert!(approx_eq(second.x, 300.0));
        assert!(approx_eq(second.width, 700.0));
    }

    // =========================================================================
    // Tree Traversal Tests (Step 7 & 8)
    // =========================================================================

    #[test]
    fn test_pane_count_single() {
        let tree = PaneLayoutNode::single_pane(test_pane(1));
        assert_eq!(tree.pane_count(), 1);
    }

    #[test]
    fn test_pane_count_split() {
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),
            second: Box::new(PaneLayoutNode::Leaf(test_pane(2))),
        };
        assert_eq!(tree.pane_count(), 2);
    }

    #[test]
    fn test_pane_count_nested() {
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),
            second: Box::new(PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(test_pane(2))),
                second: Box::new(PaneLayoutNode::Leaf(test_pane(3))),
            }),
        };
        assert_eq!(tree.pane_count(), 3);
    }

    #[test]
    fn test_all_panes() {
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),
            second: Box::new(PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(test_pane(2))),
                second: Box::new(PaneLayoutNode::Leaf(test_pane(3))),
            }),
        };

        let panes = tree.all_panes();
        assert_eq!(panes.len(), 3);

        let ids: Vec<PaneId> = panes.iter().map(|p| p.id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
        assert!(ids.contains(&3));
    }

    #[test]
    fn test_all_panes_mut() {
        let mut tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),
            second: Box::new(PaneLayoutNode::Leaf(test_pane(2))),
        };

        let panes = tree.all_panes_mut();
        assert_eq!(panes.len(), 2);

        // Modify through mutable references
        for pane in panes {
            pane.tab_bar_view_offset = 100.0;
        }

        // Verify modifications
        for pane in tree.all_panes() {
            assert_eq!(pane.tab_bar_view_offset, 100.0);
        }
    }

    #[test]
    fn test_get_pane() {
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),
            second: Box::new(PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(test_pane(2))),
                second: Box::new(PaneLayoutNode::Leaf(test_pane(3))),
            }),
        };

        assert!(tree.get_pane(1).is_some());
        assert_eq!(tree.get_pane(1).unwrap().id, 1);

        assert!(tree.get_pane(2).is_some());
        assert_eq!(tree.get_pane(2).unwrap().id, 2);

        assert!(tree.get_pane(3).is_some());
        assert_eq!(tree.get_pane(3).unwrap().id, 3);

        assert!(tree.get_pane(999).is_none());
    }

    #[test]
    fn test_get_pane_mut() {
        let mut tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),
            second: Box::new(PaneLayoutNode::Leaf(test_pane(2))),
        };

        // Modify pane 2
        let pane = tree.get_pane_mut(2).unwrap();
        pane.tab_bar_view_offset = 42.0;

        // Verify
        assert_eq!(tree.get_pane(2).unwrap().tab_bar_view_offset, 42.0);
    }

    #[test]
    fn test_contains_pane() {
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),
            second: Box::new(PaneLayoutNode::Leaf(test_pane(2))),
        };

        assert!(tree.contains_pane(1));
        assert!(tree.contains_pane(2));
        assert!(!tree.contains_pane(999));
    }

    // =========================================================================
    // nearest_leaf_toward Tests (Step 11 & 12)
    // =========================================================================

    #[test]
    fn test_nearest_leaf_toward_single_pane() {
        let tree = PaneLayoutNode::single_pane(test_pane(1));

        assert_eq!(tree.nearest_leaf_toward(Direction::Left), 1);
        assert_eq!(tree.nearest_leaf_toward(Direction::Right), 1);
        assert_eq!(tree.nearest_leaf_toward(Direction::Up), 1);
        assert_eq!(tree.nearest_leaf_toward(Direction::Down), 1);
    }

    #[test]
    fn test_nearest_leaf_toward_vsplit() {
        // VSplit(A, B) - A is top, B is bottom
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),  // A (top)
            second: Box::new(PaneLayoutNode::Leaf(test_pane(2))), // B (bottom)
        };

        // Up → first (top) → A
        assert_eq!(tree.nearest_leaf_toward(Direction::Up), 1);
        // Down → second (bottom) → B
        assert_eq!(tree.nearest_leaf_toward(Direction::Down), 2);
        // Left/Right are perpendicular → default to first → A
        assert_eq!(tree.nearest_leaf_toward(Direction::Left), 1);
        assert_eq!(tree.nearest_leaf_toward(Direction::Right), 1);
    }

    #[test]
    fn test_nearest_leaf_toward_hsplit() {
        // HSplit(A, B) - A is left, B is right
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))),  // A (left)
            second: Box::new(PaneLayoutNode::Leaf(test_pane(2))), // B (right)
        };

        // Left → first (left) → A
        assert_eq!(tree.nearest_leaf_toward(Direction::Left), 1);
        // Right → second (right) → B
        assert_eq!(tree.nearest_leaf_toward(Direction::Right), 2);
        // Up/Down are perpendicular → default to first → A
        assert_eq!(tree.nearest_leaf_toward(Direction::Up), 1);
        assert_eq!(tree.nearest_leaf_toward(Direction::Down), 1);
    }

    #[test]
    fn test_nearest_leaf_toward_nested() {
        // HSplit(A, VSplit(B, C))
        // Layout:
        // +---+---+
        // | A | B |
        // |   +---+
        // |   | C |
        // +---+---+
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))), // A
            second: Box::new(PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(test_pane(2))),  // B
                second: Box::new(PaneLayoutNode::Leaf(test_pane(3))), // C
            }),
        };

        // Entering from Left (moving right into tree) → leftmost leaf → A
        assert_eq!(tree.nearest_leaf_toward(Direction::Left), 1);

        // Entering from Right → rightmost leaf
        // HSplit: go to second (VSplit)
        // VSplit: direction is Right (perpendicular to VSplit), default to first → B
        assert_eq!(tree.nearest_leaf_toward(Direction::Right), 2);

        // Entering from Up → topmost leaf
        // HSplit: Up is perpendicular, default to first → A
        assert_eq!(tree.nearest_leaf_toward(Direction::Up), 1);

        // Entering from Down → bottommost leaf
        // HSplit: Down is perpendicular, default to first → A
        assert_eq!(tree.nearest_leaf_toward(Direction::Down), 1);
    }

    // =========================================================================
    // find_target_in_direction Tests (Step 9 & 10)
    // =========================================================================

    #[test]
    fn test_find_target_in_direction_basic() {
        // HSplit(A, VSplit(B, C))
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))), // A
            second: Box::new(PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(test_pane(2))),  // B
                second: Box::new(PaneLayoutNode::Leaf(test_pane(3))), // C
            }),
        };

        // Moving left from C → should find A (crosses HSplit boundary)
        let target = tree.find_target_in_direction(3, Direction::Left);
        assert_eq!(target, MoveTarget::ExistingPane(1));

        // Moving right from A → should find B (leftmost of VSplit, entering from left)
        let target = tree.find_target_in_direction(1, Direction::Right);
        assert_eq!(target, MoveTarget::ExistingPane(2));

        // Moving down from B → should find C (same VSplit)
        let target = tree.find_target_in_direction(2, Direction::Down);
        assert_eq!(target, MoveTarget::ExistingPane(3));

        // Moving up from C → should find B (same VSplit)
        let target = tree.find_target_in_direction(3, Direction::Up);
        assert_eq!(target, MoveTarget::ExistingPane(2));
    }

    #[test]
    fn test_find_target_in_direction_no_target() {
        // HSplit(A, VSplit(B, C))
        let tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(test_pane(1))), // A
            second: Box::new(PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(test_pane(2))),  // B
                second: Box::new(PaneLayoutNode::Leaf(test_pane(3))), // C
            }),
        };

        // Moving right from C → no target (C is rightmost in every horizontal ancestor)
        let target = tree.find_target_in_direction(3, Direction::Right);
        assert_eq!(target, MoveTarget::SplitPane(3, Direction::Right));

        // Moving left from A → no target (A is leftmost)
        let target = tree.find_target_in_direction(1, Direction::Left);
        assert_eq!(target, MoveTarget::SplitPane(1, Direction::Left));

        // Moving up from B → no target (B is topmost in the VSplit)
        let target = tree.find_target_in_direction(2, Direction::Up);
        assert_eq!(target, MoveTarget::SplitPane(2, Direction::Up));

        // Moving down from C → no target (C is bottommost)
        let target = tree.find_target_in_direction(3, Direction::Down);
        assert_eq!(target, MoveTarget::SplitPane(3, Direction::Down));
    }

    #[test]
    fn test_find_target_single_pane() {
        let tree = PaneLayoutNode::single_pane(test_pane(1));

        // All directions should return SplitPane since there's no other pane
        assert_eq!(
            tree.find_target_in_direction(1, Direction::Left),
            MoveTarget::SplitPane(1, Direction::Left)
        );
        assert_eq!(
            tree.find_target_in_direction(1, Direction::Right),
            MoveTarget::SplitPane(1, Direction::Right)
        );
        assert_eq!(
            tree.find_target_in_direction(1, Direction::Up),
            MoveTarget::SplitPane(1, Direction::Up)
        );
        assert_eq!(
            tree.find_target_in_direction(1, Direction::Down),
            MoveTarget::SplitPane(1, Direction::Down)
        );
    }

    #[test]
    fn test_find_target_nonexistent_pane() {
        let tree = PaneLayoutNode::single_pane(test_pane(1));

        // Looking for a pane that doesn't exist
        let target = tree.find_target_in_direction(999, Direction::Left);
        assert_eq!(target, MoveTarget::SplitPane(999, Direction::Left));
    }

    // =========================================================================
    // gen_pane_id Tests (Step 14)
    // =========================================================================

    #[test]
    fn test_gen_pane_id() {
        let mut next_id = 0u64;

        let id1 = gen_pane_id(&mut next_id);
        let id2 = gen_pane_id(&mut next_id);
        let id3 = gen_pane_id(&mut next_id);

        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(id3, 2);
        assert_eq!(next_id, 3);
    }
}
