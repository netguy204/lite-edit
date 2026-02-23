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
// MoveResult
// =============================================================================

// Chunk: docs/chunks/tiling_tab_movement - Directional tab movement operations

/// The result of a tab move operation.
///
/// Provides information about what happened during the move, allowing callers
/// to update focus state appropriately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveResult {
    /// Tab moved to an existing pane.
    MovedToExisting {
        /// The pane the tab was moved from.
        source_pane_id: PaneId,
        /// The pane the tab was moved to (now focused).
        target_pane_id: PaneId,
    },
    /// Tab moved to a newly created pane via split.
    MovedToNew {
        /// The pane the tab was moved from.
        source_pane_id: PaneId,
        /// The newly created pane (now focused).
        new_pane_id: PaneId,
    },
    /// Move was rejected (single-tab pane with no existing target).
    Rejected,
    /// Source pane not found in tree.
    SourceNotFound,
}

// =============================================================================
// CleanupResult
// =============================================================================

/// The result of empty pane cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupResult {
    /// No changes made.
    NoChange,
    /// Empty panes were collapsed.
    Collapsed,
    /// Root pane is empty (caller must handle).
    RootEmpty,
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

    /// Removes and returns the active tab, if any.
    ///
    /// After removal, the active_tab index is adjusted to remain valid.
    /// This is a convenience wrapper around `close_tab(self.active_tab)`.
    pub fn remove_active_tab(&mut self) -> Option<Tab> {
        if self.tabs.is_empty() {
            None
        } else {
            self.close_tab(self.active_tab)
        }
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
// Tab Movement Operations
// =============================================================================

// Chunk: docs/chunks/tiling_tab_movement - Directional tab movement operations

/// Moves the active tab from a source pane in the given direction.
///
/// This function:
/// 1. Uses `find_target_in_direction` to determine the move target.
/// 2. For `ExistingPane(target_id)`: removes the active tab from the source pane
///    and adds it to the target pane.
/// 3. For `SplitPane(pane_id, direction)`: removes the active tab from the source
///    pane, creates a new pane containing that tab, and replaces the source pane's
///    leaf node with a split node.
/// 4. Rejects the move if the source pane has only one tab and no existing target
///    exists (splitting a single-tab pane is a no-op).
/// 5. Automatically calls `cleanup_empty_panes` at the end.
///
/// # Arguments
///
/// * `root` - The root of the pane layout tree.
/// * `source_pane_id` - The ID of the pane containing the tab to move.
/// * `direction` - The direction to move the tab.
/// * `new_pane_id_fn` - A closure that generates new pane IDs.
///
/// # Returns
///
/// A `MoveResult` indicating what happened.
pub fn move_tab(
    root: &mut PaneLayoutNode,
    source_pane_id: PaneId,
    direction: Direction,
    mut new_pane_id_fn: impl FnMut() -> PaneId,
) -> MoveResult {
    // Step 1: Find the target
    let target = root.find_target_in_direction(source_pane_id, direction);

    // Step 2: Check preconditions
    let source_pane = match root.get_pane(source_pane_id) {
        Some(p) => p,
        None => return MoveResult::SourceNotFound,
    };

    // Source must have at least one tab
    if source_pane.tabs.is_empty() {
        return MoveResult::SourceNotFound;
    }

    // If source has only one tab and target is SplitPane, reject
    // (splitting a single-tab pane creates an empty sibling that collapses - a no-op)
    let source_tab_count = source_pane.tab_count();
    if source_tab_count == 1 {
        if let MoveTarget::SplitPane(_, _) = target {
            return MoveResult::Rejected;
        }
    }

    // Step 3: Execute move based on target type
    let result = match target {
        MoveTarget::ExistingPane(target_pane_id) => {
            // Move to existing pane
            move_tab_to_existing(root, source_pane_id, target_pane_id)
        }
        MoveTarget::SplitPane(pane_id, dir) => {
            // Create a new pane via split
            let new_pane_id = new_pane_id_fn();
            let workspace_id = root.get_pane(pane_id).map(|p| p.workspace_id).unwrap_or(0);
            move_tab_to_new_split(root, pane_id, dir, new_pane_id, workspace_id)
        }
    };

    // Step 4: Cleanup empty panes
    cleanup_empty_panes(root);

    result
}

/// Moves the active tab from source pane to an existing target pane.
fn move_tab_to_existing(
    root: &mut PaneLayoutNode,
    source_pane_id: PaneId,
    target_pane_id: PaneId,
) -> MoveResult {
    // Remove tab from source
    let tab = {
        let source = match root.get_pane_mut(source_pane_id) {
            Some(p) => p,
            None => return MoveResult::SourceNotFound,
        };
        match source.remove_active_tab() {
            Some(t) => t,
            None => return MoveResult::SourceNotFound,
        }
    };

    // Add to target
    let target = match root.get_pane_mut(target_pane_id) {
        Some(p) => p,
        None => {
            // Put the tab back in the source
            if let Some(source) = root.get_pane_mut(source_pane_id) {
                source.add_tab(tab);
            }
            return MoveResult::SourceNotFound;
        }
    };
    target.add_tab(tab);

    MoveResult::MovedToExisting {
        source_pane_id,
        target_pane_id,
    }
}

/// Moves the active tab from source pane to a new pane via split.
fn move_tab_to_new_split(
    root: &mut PaneLayoutNode,
    source_pane_id: PaneId,
    direction: Direction,
    new_pane_id: PaneId,
    workspace_id: WorkspaceId,
) -> MoveResult {
    // Remove tab from source
    let tab = {
        let source = match root.get_pane_mut(source_pane_id) {
            Some(p) => p,
            None => return MoveResult::SourceNotFound,
        };
        match source.remove_active_tab() {
            Some(t) => t,
            None => return MoveResult::SourceNotFound,
        }
    };

    // Create new pane with the tab
    let mut new_pane = Pane::new(new_pane_id, workspace_id);
    new_pane.add_tab(tab);

    // Replace the source pane with a split containing both
    let success = root.replace_pane_with_split(source_pane_id, new_pane, direction);

    if success {
        MoveResult::MovedToNew {
            source_pane_id,
            new_pane_id,
        }
    } else {
        // This shouldn't happen if source_pane_id was valid, but handle it
        MoveResult::SourceNotFound
    }
}

impl PaneLayoutNode {
    /// Replaces a leaf pane with a split containing that pane and a new pane.
    ///
    /// Returns `true` if the replacement was made, `false` if the pane wasn't found.
    ///
    /// The split direction is determined by `direction.to_split_direction()`, and
    /// child ordering is determined by `direction.is_toward_second()`:
    /// - Right/Down: original pane is First, new pane is Second
    /// - Left/Up: new pane is First, original pane is Second
    fn replace_pane_with_split(
        &mut self,
        pane_id: PaneId,
        new_pane: Pane,
        direction: Direction,
    ) -> bool {
        match self {
            PaneLayoutNode::Leaf(pane) => {
                if pane.id == pane_id {
                    // Take ownership of the original pane
                    let original_pane =
                        std::mem::replace(pane, Pane::new(0, 0)); // Placeholder

                    // Build the split node
                    let split_direction = direction.to_split_direction();
                    let new_is_second = direction.is_toward_second();

                    let (first, second) = if new_is_second {
                        (
                            Box::new(PaneLayoutNode::Leaf(original_pane)),
                            Box::new(PaneLayoutNode::Leaf(new_pane)),
                        )
                    } else {
                        (
                            Box::new(PaneLayoutNode::Leaf(new_pane)),
                            Box::new(PaneLayoutNode::Leaf(original_pane)),
                        )
                    };

                    // Replace self with the split
                    *self = PaneLayoutNode::Split {
                        direction: split_direction,
                        ratio: 0.5,
                        first,
                        second,
                    };

                    true
                } else {
                    false
                }
            }
            PaneLayoutNode::Split { first, second, .. } => {
                // Check which subtree contains the pane to avoid unnecessary cloning
                if first.contains_pane(pane_id) {
                    first.replace_pane_with_split(pane_id, new_pane, direction)
                } else if second.contains_pane(pane_id) {
                    second.replace_pane_with_split(pane_id, new_pane, direction)
                } else {
                    false
                }
            }
        }
    }
}

// =============================================================================
// Empty Pane Cleanup
// =============================================================================

// Chunk: docs/chunks/tiling_tab_movement - Directional tab movement operations

/// Cleans up empty panes by collapsing them and promoting their siblings.
///
/// After any operation that might leave a pane empty (like moving a tab out),
/// this function should be called to maintain tree invariants.
///
/// # Algorithm
///
/// - If root is a `Leaf` with empty pane → return `RootEmpty`.
/// - If root is a `Split`, recursively cleanup children.
/// - After recursing, if either child is a `Leaf` with empty pane,
///   replace this `Split` with the non-empty sibling.
///
/// # Returns
///
/// A `CleanupResult` indicating what happened.
pub fn cleanup_empty_panes(root: &mut PaneLayoutNode) -> CleanupResult {
    cleanup_empty_panes_impl(root, true)
}

fn cleanup_empty_panes_impl(node: &mut PaneLayoutNode, is_root: bool) -> CleanupResult {
    match node {
        PaneLayoutNode::Leaf(pane) => {
            if pane.is_empty() {
                if is_root {
                    CleanupResult::RootEmpty
                } else {
                    // Parent will handle collapsing
                    CleanupResult::NoChange
                }
            } else {
                CleanupResult::NoChange
            }
        }
        PaneLayoutNode::Split { first, second, .. } => {
            // Recursively cleanup children - track if any nested cleanup happened
            let first_result = cleanup_empty_panes_impl(first, false);
            let second_result = cleanup_empty_panes_impl(second, false);
            let nested_collapsed = matches!(first_result, CleanupResult::Collapsed)
                || matches!(second_result, CleanupResult::Collapsed);

            // Check if either child is an empty leaf
            let first_empty = matches!(first.as_ref(), PaneLayoutNode::Leaf(p) if p.is_empty());
            let second_empty = matches!(second.as_ref(), PaneLayoutNode::Leaf(p) if p.is_empty());

            if first_empty && second_empty {
                // Both children are empty - this shouldn't happen in normal operation,
                // but handle it by keeping the structure
                if is_root {
                    CleanupResult::RootEmpty
                } else {
                    CleanupResult::NoChange
                }
            } else if first_empty {
                // Promote second child
                let second_node = std::mem::replace(
                    second.as_mut(),
                    PaneLayoutNode::Leaf(Pane::new(0, 0)),
                );
                *node = second_node;
                CleanupResult::Collapsed
            } else if second_empty {
                // Promote first child
                let first_node = std::mem::replace(
                    first.as_mut(),
                    PaneLayoutNode::Leaf(Pane::new(0, 0)),
                );
                *node = first_node;
                CleanupResult::Collapsed
            } else if nested_collapsed {
                // No direct collapse at this level, but a nested collapse happened
                CleanupResult::Collapsed
            } else {
                CleanupResult::NoChange
            }
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

    // =========================================================================
    // move_tab Tests
    // =========================================================================

    #[test]
    fn test_move_tab_split_creation() {
        // Two tabs in a single pane, move one right → HSplit with two panes
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1)); // Tab A
        pane.add_tab(test_tab(2)); // Tab B (active)

        let mut tree = PaneLayoutNode::single_pane(pane);
        let mut next_pane_id = 2u64;

        let result = move_tab(&mut tree, 1, Direction::Right, || {
            let id = next_pane_id;
            next_pane_id += 1;
            id
        });

        // Should create a new pane
        assert!(matches!(result, MoveResult::MovedToNew { source_pane_id: 1, new_pane_id: 2 }));

        // Tree should now be a horizontal split
        assert_eq!(tree.pane_count(), 2);

        // Source pane (id=1) should have one tab
        let source = tree.get_pane(1).unwrap();
        assert_eq!(source.tab_count(), 1);

        // New pane (id=2) should have one tab (the moved tab)
        let new_pane = tree.get_pane(2).unwrap();
        assert_eq!(new_pane.tab_count(), 1);
        assert_eq!(new_pane.tabs[0].id, 2); // Tab B
    }

    #[test]
    fn test_move_tab_to_existing_neighbor() {
        // HSplit(Pane[A, B], Pane[C]), move B right → B joins Pane[C]
        let mut pane1 = test_pane(1);
        pane1.add_tab(test_tab(1)); // Tab A
        pane1.add_tab(test_tab(2)); // Tab B (active)

        let mut pane2 = test_pane(2);
        pane2.add_tab(test_tab(3)); // Tab C

        let mut tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane1)),
            second: Box::new(PaneLayoutNode::Leaf(pane2)),
        };

        let result = move_tab(&mut tree, 1, Direction::Right, || panic!("Should not create new pane"));

        // Should move to existing pane
        assert!(matches!(result, MoveResult::MovedToExisting { source_pane_id: 1, target_pane_id: 2 }));

        // Source pane should have one tab (A)
        let source = tree.get_pane(1).unwrap();
        assert_eq!(source.tab_count(), 1);
        assert_eq!(source.tabs[0].id, 1);

        // Target pane should have two tabs (C, B) - B is now active (last added)
        let target = tree.get_pane(2).unwrap();
        assert_eq!(target.tab_count(), 2);
        assert_eq!(target.active_tab, 1); // B is active
    }

    #[test]
    fn test_move_tab_single_tab_rejected() {
        // Pane with one tab, no existing target → move rejected
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));

        let mut tree = PaneLayoutNode::single_pane(pane);

        let result = move_tab(&mut tree, 1, Direction::Right, || panic!("Should not create new pane"));

        // Should be rejected
        assert_eq!(result, MoveResult::Rejected);

        // Tree unchanged
        assert_eq!(tree.pane_count(), 1);
        let pane = tree.get_pane(1).unwrap();
        assert_eq!(pane.tab_count(), 1);
    }

    #[test]
    fn test_move_tab_single_tab_transfer() {
        // Pane with one tab, existing target → tab moves, source empties, tree collapses
        let mut pane1 = test_pane(1);
        pane1.add_tab(test_tab(1)); // Only one tab

        let mut pane2 = test_pane(2);
        pane2.add_tab(test_tab(2));

        let mut tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane1)),
            second: Box::new(PaneLayoutNode::Leaf(pane2)),
        };

        let result = move_tab(&mut tree, 1, Direction::Right, || panic!("Should not create new pane"));

        // Should move to existing
        assert!(matches!(result, MoveResult::MovedToExisting { source_pane_id: 1, target_pane_id: 2 }));

        // Tree should collapse to single pane (pane 2 with both tabs)
        assert_eq!(tree.pane_count(), 1);

        // Target pane should have both tabs
        let target = tree.get_pane(2).unwrap();
        assert_eq!(target.tab_count(), 2);
    }

    #[test]
    fn test_move_tab_source_not_found() {
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));

        let mut tree = PaneLayoutNode::single_pane(pane);

        let result = move_tab(&mut tree, 999, Direction::Right, || panic!("Should not create new pane"));

        assert_eq!(result, MoveResult::SourceNotFound);
    }

    #[test]
    fn test_move_tab_direction_ordering_right() {
        // Move right creates new pane as Second child
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));
        pane.add_tab(test_tab(2));

        let mut tree = PaneLayoutNode::single_pane(pane);
        let mut next_pane_id = 2u64;

        move_tab(&mut tree, 1, Direction::Right, || {
            let id = next_pane_id;
            next_pane_id += 1;
            id
        });

        // Verify layout: source should be on the left, new pane on the right
        let rects = calculate_pane_rects((0.0, 0.0, 800.0, 600.0), &tree);
        let source_rect = rects.iter().find(|r| r.pane_id == 1).unwrap();
        let new_rect = rects.iter().find(|r| r.pane_id == 2).unwrap();

        // Source pane should be on the left (smaller x)
        assert!(source_rect.x < new_rect.x);
    }

    #[test]
    fn test_move_tab_direction_ordering_left() {
        // Move left creates new pane as First child
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));
        pane.add_tab(test_tab(2));

        let mut tree = PaneLayoutNode::single_pane(pane);
        let mut next_pane_id = 2u64;

        move_tab(&mut tree, 1, Direction::Left, || {
            let id = next_pane_id;
            next_pane_id += 1;
            id
        });

        // Verify layout: new pane should be on the left, source on the right
        let rects = calculate_pane_rects((0.0, 0.0, 800.0, 600.0), &tree);
        let source_rect = rects.iter().find(|r| r.pane_id == 1).unwrap();
        let new_rect = rects.iter().find(|r| r.pane_id == 2).unwrap();

        // New pane should be on the left (smaller x)
        assert!(new_rect.x < source_rect.x);
    }

    #[test]
    fn test_move_tab_direction_ordering_down() {
        // Move down creates new pane as Second child (vertical split)
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));
        pane.add_tab(test_tab(2));

        let mut tree = PaneLayoutNode::single_pane(pane);
        let mut next_pane_id = 2u64;

        move_tab(&mut tree, 1, Direction::Down, || {
            let id = next_pane_id;
            next_pane_id += 1;
            id
        });

        // Verify layout: source should be on top, new pane on bottom
        let rects = calculate_pane_rects((0.0, 0.0, 800.0, 600.0), &tree);
        let source_rect = rects.iter().find(|r| r.pane_id == 1).unwrap();
        let new_rect = rects.iter().find(|r| r.pane_id == 2).unwrap();

        // Source pane should be on top (smaller y)
        assert!(source_rect.y < new_rect.y);
    }

    #[test]
    fn test_move_tab_direction_ordering_up() {
        // Move up creates new pane as First child (vertical split)
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));
        pane.add_tab(test_tab(2));

        let mut tree = PaneLayoutNode::single_pane(pane);
        let mut next_pane_id = 2u64;

        move_tab(&mut tree, 1, Direction::Up, || {
            let id = next_pane_id;
            next_pane_id += 1;
            id
        });

        // Verify layout: new pane should be on top, source on bottom
        let rects = calculate_pane_rects((0.0, 0.0, 800.0, 600.0), &tree);
        let source_rect = rects.iter().find(|r| r.pane_id == 1).unwrap();
        let new_rect = rects.iter().find(|r| r.pane_id == 2).unwrap();

        // New pane should be on top (smaller y)
        assert!(new_rect.y < source_rect.y);
    }

    // =========================================================================
    // remove_active_tab Tests
    // =========================================================================

    #[test]
    fn test_remove_active_tab_basic() {
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));
        pane.add_tab(test_tab(2));
        pane.add_tab(test_tab(3));
        // active_tab = 2 (third tab)

        let removed = pane.remove_active_tab();
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, 3);
        assert_eq!(pane.tab_count(), 2);
        assert_eq!(pane.active_tab, 1); // Adjusted to last tab
    }

    #[test]
    fn test_remove_active_tab_from_empty() {
        let mut pane = test_pane(1);
        let removed = pane.remove_active_tab();
        assert!(removed.is_none());
    }

    #[test]
    fn test_remove_active_tab_last_tab() {
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));

        let removed = pane.remove_active_tab();
        assert!(removed.is_some());
        assert!(pane.is_empty());
    }

    // =========================================================================
    // cleanup_empty_panes Tests
    // =========================================================================

    #[test]
    fn test_cleanup_no_empty_panes() {
        // Tree with no empty panes should be unchanged
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1));

        let mut tree = PaneLayoutNode::single_pane(pane);

        let result = cleanup_empty_panes(&mut tree);
        assert_eq!(result, CleanupResult::NoChange);
        assert_eq!(tree.pane_count(), 1);
    }

    #[test]
    fn test_cleanup_single_split_with_empty_leaf() {
        // HSplit(Pane[], Pane[A]) → Pane[A]
        let empty_pane = test_pane(1);
        let mut full_pane = test_pane(2);
        full_pane.add_tab(test_tab(1));

        let mut tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(empty_pane)),
            second: Box::new(PaneLayoutNode::Leaf(full_pane)),
        };

        let result = cleanup_empty_panes(&mut tree);
        assert_eq!(result, CleanupResult::Collapsed);

        // Tree should collapse to single pane
        assert_eq!(tree.pane_count(), 1);
        assert!(tree.get_pane(2).is_some());
    }

    #[test]
    fn test_cleanup_nested_tree_collapse() {
        // HSplit(Pane[A], VSplit(Pane[], Pane[B])) → HSplit(Pane[A], Pane[B])
        let mut pane_a = test_pane(1);
        pane_a.add_tab(test_tab(1));

        let empty_pane = test_pane(2);

        let mut pane_b = test_pane(3);
        pane_b.add_tab(test_tab(2));

        let mut tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane_a)),
            second: Box::new(PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(empty_pane)),
                second: Box::new(PaneLayoutNode::Leaf(pane_b)),
            }),
        };

        let result = cleanup_empty_panes(&mut tree);
        assert_eq!(result, CleanupResult::Collapsed);

        // Tree should now be HSplit(Pane[A], Pane[B])
        assert_eq!(tree.pane_count(), 2);
        assert!(tree.get_pane(1).is_some());
        assert!(tree.get_pane(3).is_some());
    }

    #[test]
    fn test_cleanup_root_empty_pane() {
        // Single empty pane at root
        let mut tree = PaneLayoutNode::single_pane(test_pane(1));

        let result = cleanup_empty_panes(&mut tree);
        assert_eq!(result, CleanupResult::RootEmpty);
    }

    #[test]
    fn test_cleanup_promote_second_sibling() {
        // HSplit(Pane[A], Pane[]) → Pane[A]
        let mut pane_a = test_pane(1);
        pane_a.add_tab(test_tab(1));

        let empty_pane = test_pane(2);

        let mut tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane_a)),
            second: Box::new(PaneLayoutNode::Leaf(empty_pane)),
        };

        let result = cleanup_empty_panes(&mut tree);
        assert_eq!(result, CleanupResult::Collapsed);

        // Tree should collapse to pane A
        assert_eq!(tree.pane_count(), 1);
        assert!(tree.get_pane(1).is_some());
    }

    // =========================================================================
    // Integration Tests: Move + Cleanup Scenarios
    // =========================================================================

    #[test]
    fn test_integration_split_creation_structure() {
        // Start with Pane[A, B], move B right → HSplit(Pane[A], Pane[B])
        let mut pane = test_pane(1);
        pane.add_tab(test_tab(1)); // Tab A
        pane.add_tab(test_tab(2)); // Tab B (active)

        let mut tree = PaneLayoutNode::single_pane(pane);
        let mut next_pane_id = 2u64;

        let result = move_tab(&mut tree, 1, Direction::Right, || {
            let id = next_pane_id;
            next_pane_id += 1;
            id
        });

        assert!(matches!(result, MoveResult::MovedToNew { .. }));

        // Verify structure is HSplit
        match &tree {
            PaneLayoutNode::Split { direction, .. } => {
                assert_eq!(*direction, SplitDirection::Horizontal);
            }
            _ => panic!("Expected a split"),
        }

        // Verify tab distribution
        let pane1 = tree.get_pane(1).unwrap();
        assert_eq!(pane1.tab_count(), 1);
        assert_eq!(pane1.tabs[0].id, 1); // Tab A

        let pane2 = tree.get_pane(2).unwrap();
        assert_eq!(pane2.tab_count(), 1);
        assert_eq!(pane2.tabs[0].id, 2); // Tab B
    }

    #[test]
    fn test_integration_nested_tree_navigation() {
        // HSplit(Pane[A], VSplit(Pane[B], Pane[C, D]))
        // Move D left → D joins Pane A
        // Pane C doesn't empty, so VSplit remains
        let mut pane_a = test_pane(1);
        pane_a.add_tab(test_tab(1)); // Tab A

        let mut pane_b = test_pane(2);
        pane_b.add_tab(test_tab(2)); // Tab B

        let mut pane_c = test_pane(3);
        pane_c.add_tab(test_tab(3)); // Tab C
        pane_c.add_tab(test_tab(4)); // Tab D (active)

        let mut tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane_a)),
            second: Box::new(PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(pane_b)),
                second: Box::new(PaneLayoutNode::Leaf(pane_c)),
            }),
        };

        let result = move_tab(&mut tree, 3, Direction::Left, || panic!("Should not create new pane"));

        assert!(matches!(result, MoveResult::MovedToExisting { source_pane_id: 3, target_pane_id: 1 }));

        // Pane A should now have 2 tabs (A and D)
        let pane_a = tree.get_pane(1).unwrap();
        assert_eq!(pane_a.tab_count(), 2);

        // Pane C should have 1 tab (C only)
        let pane_c = tree.get_pane(3).unwrap();
        assert_eq!(pane_c.tab_count(), 1);

        // Tree structure should be unchanged (no collapse needed)
        assert_eq!(tree.pane_count(), 3);
    }

    #[test]
    fn test_integration_nested_tree_collapse() {
        // HSplit(Pane[A], VSplit(Pane[B], Pane[C]))
        // Move C left → C joins Pane A, Pane C empties, VSplit collapses
        // Result: HSplit(Pane[A, C], Pane[B])
        let mut pane_a = test_pane(1);
        pane_a.add_tab(test_tab(1)); // Tab A

        let mut pane_b = test_pane(2);
        pane_b.add_tab(test_tab(2)); // Tab B

        let mut pane_c = test_pane(3);
        pane_c.add_tab(test_tab(3)); // Tab C (only tab, active)

        let mut tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane_a)),
            second: Box::new(PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(pane_b)),
                second: Box::new(PaneLayoutNode::Leaf(pane_c)),
            }),
        };

        let result = move_tab(&mut tree, 3, Direction::Left, || panic!("Should not create new pane"));

        assert!(matches!(result, MoveResult::MovedToExisting { source_pane_id: 3, target_pane_id: 1 }));

        // Tree should now be HSplit(Pane[A, C], Pane[B])
        assert_eq!(tree.pane_count(), 2);

        // Pane A should have tabs A and C
        let pane_a = tree.get_pane(1).unwrap();
        assert_eq!(pane_a.tab_count(), 2);

        // Pane B should still exist
        let pane_b = tree.get_pane(2).unwrap();
        assert_eq!(pane_b.tab_count(), 1);
    }

    #[test]
    fn test_integration_deep_tree_collapse_to_single() {
        // Progressively move all tabs to one pane, collapsing tree back to single pane
        // Start: HSplit(Pane[A], Pane[B])
        let mut pane_a = test_pane(1);
        pane_a.add_tab(test_tab(1)); // Tab A

        let mut pane_b = test_pane(2);
        pane_b.add_tab(test_tab(2)); // Tab B

        let mut tree = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane_a)),
            second: Box::new(PaneLayoutNode::Leaf(pane_b)),
        };

        // Move B left → B joins A, tree collapses to single pane
        let result = move_tab(&mut tree, 2, Direction::Left, || panic!("Should not create new pane"));

        assert!(matches!(result, MoveResult::MovedToExisting { source_pane_id: 2, target_pane_id: 1 }));

        // Tree should collapse to single pane
        assert_eq!(tree.pane_count(), 1);

        // Single pane should have both tabs
        let pane = tree.get_pane(1).unwrap();
        assert_eq!(pane.tab_count(), 2);
    }

    // =========================================================================
    // workspace_id Preservation Tests
    // =========================================================================

    #[test]
    fn test_move_tab_preserves_workspace_id() {
        // When a new pane is created during a split, it should inherit workspace_id
        let workspace_id = 42u64;

        let mut pane = Pane::new(1, workspace_id);
        pane.add_tab(test_tab(1));
        pane.add_tab(test_tab(2));

        let mut tree = PaneLayoutNode::single_pane(pane);
        let mut next_pane_id = 2u64;

        move_tab(&mut tree, 1, Direction::Right, || {
            let id = next_pane_id;
            next_pane_id += 1;
            id
        });

        // Verify new pane has the same workspace_id
        let new_pane = tree.get_pane(2).unwrap();
        assert_eq!(new_pane.workspace_id, workspace_id);
    }
}
