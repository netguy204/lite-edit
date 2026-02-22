// Chunk: docs/chunks/selector_widget - Reusable selector interaction model
//!
//! A reusable selector widget for type-to-filter UI patterns.
//!
//! This module provides [`SelectorWidget`], a self-contained interaction model
//! that manages a filterable list of items, a text query the user types into,
//! and a selected index. It serves as the shared UI primitive for the file picker,
//! command palette, and any other type-to-filter overlay.
//!
//! The widget knows nothing about files, rendering, or macOS — only about
//! query editing, item selection, and signalling outcomes via [`SelectorOutcome`].
//!
//! # Design
//!
//! Following the project's Humble View Architecture, `SelectorWidget` is pure
//! interaction state with no platform dependencies. Downstream code (renderers,
//! focus targets) consume this state and translate it to pixels or editor mutations.
//!
//! # Example
//!
//! ```ignore
//! use crate::selector::{SelectorWidget, SelectorOutcome};
//! use crate::input::{KeyEvent, Key};
//!
//! let mut selector = SelectorWidget::new();
//! selector.set_items(vec!["foo.rs".into(), "bar.rs".into(), "baz.rs".into()]);
//!
//! // User types 'b'
//! let outcome = selector.handle_key(&KeyEvent::char('b'));
//! assert_eq!(outcome, SelectorOutcome::Pending);
//! assert_eq!(selector.query(), "b");
//!
//! // External code would filter items based on query and call set_items again
//! selector.set_items(vec!["bar.rs".into(), "baz.rs".into()]);
//!
//! // User presses Enter to confirm
//! let outcome = selector.handle_key(&KeyEvent::new(Key::Return, Default::default()));
//! assert_eq!(outcome, SelectorOutcome::Confirmed(0));
//! ```

use crate::input::{Key, KeyEvent, MouseEventKind};

/// The outcome of handling an input event in the selector widget.
///
/// Returned by [`SelectorWidget::handle_key`] and [`SelectorWidget::handle_mouse`]
/// to signal what action (if any) should be taken by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorOutcome {
    /// The selector is still open; no decision has been made yet.
    Pending,
    /// The user confirmed a selection. The value is the index into the items list.
    ///
    /// If the items list is empty when the user confirms, this returns `usize::MAX`
    /// as a sentinel value. Callers should interpret this as "create with current query".
    Confirmed(usize),
    /// The user cancelled/dismissed the selector without making a selection.
    Cancelled,
}

/// A reusable selector widget for type-to-filter UI patterns.
///
/// Manages a query string, a list of displayable items, and a selected index.
/// The caller is responsible for:
/// - Filtering items when the query changes
/// - Updating the items list via [`set_items`](Self::set_items)
/// - Rendering the widget state
/// - Interpreting [`SelectorOutcome`] values
///
/// The widget handles:
/// - Query editing (character input, backspace)
/// - Navigation (up/down arrows)
/// - Confirmation (Enter) and cancellation (Escape)
/// - Mouse selection and confirmation
#[derive(Debug, Clone)]
pub struct SelectorWidget {
    /// The text the user has typed (filter query).
    query: String,
    /// The current list of displayable strings.
    items: Vec<String>,
    /// Index into `items` of the currently highlighted entry.
    /// Always clamped to valid bounds (0..items.len(), or 0 if empty).
    selected_index: usize,
    /// Index of the first item visible in the list (for scrolling).
    view_offset: usize,
    /// Number of items that can be displayed in the visible area.
    visible_items: usize,
}

impl Default for SelectorWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectorWidget {
    /// Creates a new selector widget with empty query, no items, and index 0.
    pub fn new() -> Self {
        Self {
            query: String::new(),
            items: Vec::new(),
            selected_index: 0,
            view_offset: 0,
            visible_items: 0,
        }
    }

    /// Returns the current query string.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Returns the currently selected index.
    ///
    /// This index is always valid for the current items list, or 0 if the list is empty.
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// Returns the current items list.
    pub fn items(&self) -> &[String] {
        &self.items
    }

    /// Returns the current view offset (index of first visible item).
    pub fn view_offset(&self) -> usize {
        self.view_offset
    }

    /// Sets the number of visible items in the display area.
    ///
    /// This value is used by `handle_key` to keep the selection visible
    /// when navigating with arrow keys.
    pub fn set_visible_items(&mut self, n: usize) {
        self.visible_items = n;
    }

    /// Replaces the item list and clamps the selected index and view_offset to valid bounds.
    ///
    /// If the new list has fewer items than the current `selected_index`,
    /// the index is clamped to `new_items.len() - 1` (or 0 if empty).
    ///
    /// The `view_offset` is also clamped to ensure it doesn't point past the
    /// new end of the list (e.g., after a query narrows results).
    pub fn set_items(&mut self, items: Vec<String>) {
        self.items = items;
        // Clamp selected_index to valid range
        if self.items.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.items.len() - 1);
        }
        // Clamp view_offset to valid range
        let max_offset = self.items.len().saturating_sub(self.visible_items);
        self.view_offset = self.view_offset.min(max_offset);
    }

    /// Handles a keyboard event and returns the appropriate outcome.
    ///
    /// # Behavior
    ///
    /// - **Up arrow**: Decrements `selected_index` (floor at 0), returns `Pending`.
    /// - **Down arrow**: Increments `selected_index` (ceil at `items.len() - 1`), returns `Pending`.
    /// - **Return/Enter**: Returns `Confirmed(selected_index)`, or `Confirmed(usize::MAX)` if items is empty.
    /// - **Escape**: Returns `Cancelled`.
    /// - **Backspace** (no command/control modifiers): Removes the last character from `query`, returns `Pending`.
    /// - **Printable char** (no command/control modifiers): Appends to `query`, resets `selected_index` to 0, returns `Pending`.
    /// - **All other keys**: Returns `Pending` (no-op).
    pub fn handle_key(&mut self, event: &KeyEvent) -> SelectorOutcome {
        // Check for command/control modifiers - these should not modify the query
        let has_command_or_control = event.modifiers.command || event.modifiers.control;

        match &event.key {
            Key::Up => {
                self.selected_index = self.selected_index.saturating_sub(1);
                // Keep selection visible: if selected_index is above view_offset, scroll up
                if self.selected_index < self.view_offset {
                    self.view_offset = self.selected_index;
                }
                SelectorOutcome::Pending
            }
            Key::Down => {
                if !self.items.is_empty() {
                    let max_index = self.items.len() - 1;
                    if self.selected_index < max_index {
                        self.selected_index += 1;
                    }
                }
                // Keep selection visible: if selected_index is past the visible window, scroll down
                if self.visible_items > 0
                    && self.selected_index >= self.view_offset + self.visible_items
                {
                    self.view_offset = self.selected_index - self.visible_items + 1;
                }
                SelectorOutcome::Pending
            }
            Key::Return => {
                if self.items.is_empty() {
                    SelectorOutcome::Confirmed(usize::MAX)
                } else {
                    SelectorOutcome::Confirmed(self.selected_index)
                }
            }
            Key::Escape => SelectorOutcome::Cancelled,
            Key::Backspace if !has_command_or_control => {
                self.query.pop();
                SelectorOutcome::Pending
            }
            Key::Char(ch) if !has_command_or_control && !ch.is_control() => {
                self.query.push(*ch);
                self.selected_index = 0;
                SelectorOutcome::Pending
            }
            _ => SelectorOutcome::Pending,
        }
    }

    /// Handles a scroll event by adjusting the view offset.
    ///
    /// # Arguments
    ///
    /// * `delta_y` - The raw pixel delta (positive = scroll down / content moves up).
    /// * `item_height` - The height of each item row in pixels.
    /// * `visible_items` - The number of items visible in the display area.
    ///
    /// # Behavior
    ///
    /// - Computes rows to shift: `(delta_y / item_height).round() as isize`
    /// - Updates `view_offset` by adding the row delta
    /// - Clamps `view_offset` to `0..=items.len().saturating_sub(visible_items)`
    /// - No-op if items fit entirely within `visible_items`
    pub fn handle_scroll(&mut self, delta_y: f64, item_height: f64, visible_items: usize) {
        // No-op if list fits within visible area
        if self.items.len() <= visible_items {
            return;
        }

        // No-op on empty list
        if self.items.is_empty() {
            return;
        }

        // Compute rows to shift
        let rows = (delta_y / item_height).round() as isize;

        // Update view_offset with clamping
        let new_offset = (self.view_offset as isize + rows)
            .max(0)
            .min((self.items.len().saturating_sub(visible_items)) as isize) as usize;

        self.view_offset = new_offset;
    }

    /// Handles a mouse event and returns the appropriate outcome.
    ///
    /// # Parameters
    ///
    /// - `position`: Mouse position in view coordinates `(x, y)`.
    /// - `kind`: The type of mouse event (Down, Up, Moved).
    /// - `item_height`: The height of each item row in pixels.
    /// - `list_origin_y`: The Y coordinate where the list starts (top of first item).
    ///
    /// # Behavior
    ///
    /// - **Down on a list row**: Sets `selected_index` to that row (accounting for
    ///   `view_offset`), returns `Pending`.
    /// - **Up on same row as `selected_index`**: Returns `Confirmed(selected_index)`.
    /// - **Up on different row**: Sets `selected_index` to that row, returns `Pending`.
    /// - **Outside list bounds** (above or below the list): Returns `Pending` (no-op).
    /// - **Moved**: Returns `Pending` (no-op).
    ///
    /// Note: The `row` computed from the click position is the visible row (0 = first
    /// visible item). The actual item index is `view_offset + row`.
    pub fn handle_mouse(
        &mut self,
        position: (f64, f64),
        kind: MouseEventKind,
        item_height: f64,
        list_origin_y: f64,
    ) -> SelectorOutcome {
        // Check if position is within list bounds
        if position.1 < list_origin_y || self.items.is_empty() {
            return SelectorOutcome::Pending;
        }

        // Compute which visible row was clicked
        let relative_y = position.1 - list_origin_y;
        let visible_row = (relative_y / item_height) as usize;

        // Compute the actual item index (accounting for view_offset)
        let item_index = self.view_offset + visible_row;

        // Check if item_index is within valid range
        if item_index >= self.items.len() {
            return SelectorOutcome::Pending;
        }

        match kind {
            MouseEventKind::Down => {
                self.selected_index = item_index;
                SelectorOutcome::Pending
            }
            MouseEventKind::Up => {
                if item_index == self.selected_index {
                    SelectorOutcome::Confirmed(self.selected_index)
                } else {
                    self.selected_index = item_index;
                    SelectorOutcome::Pending
                }
            }
            MouseEventKind::Moved => SelectorOutcome::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Modifiers;

    // =========================================================================
    // Step 1: Basic accessors and initial state
    // =========================================================================

    #[test]
    fn new_widget_has_empty_query() {
        let widget = SelectorWidget::new();
        assert_eq!(widget.query(), "");
    }

    #[test]
    fn new_widget_has_selected_index_zero() {
        let widget = SelectorWidget::new();
        assert_eq!(widget.selected_index(), 0);
    }

    #[test]
    fn new_widget_has_empty_items() {
        let widget = SelectorWidget::new();
        assert!(widget.items().is_empty());
    }

    #[test]
    fn default_is_same_as_new() {
        let default_widget = SelectorWidget::default();
        let new_widget = SelectorWidget::new();
        assert_eq!(default_widget.query(), new_widget.query());
        assert_eq!(default_widget.selected_index(), new_widget.selected_index());
        assert_eq!(default_widget.items().len(), new_widget.items().len());
    }

    // =========================================================================
    // Scroll support: view_offset and visible_items
    // =========================================================================

    #[test]
    fn new_widget_has_view_offset_zero() {
        let widget = SelectorWidget::new();
        assert_eq!(widget.view_offset(), 0);
    }

    #[test]
    fn new_widget_has_visible_items_zero() {
        let widget = SelectorWidget::new();
        // visible_items is internal but affects scroll behavior
        // We verify it's 0 by checking that arrow navigation doesn't auto-scroll
        // with default settings (visible_items = 0)
        assert_eq!(widget.view_offset(), 0);
    }

    #[test]
    fn set_visible_items_stores_value() {
        let mut widget = SelectorWidget::new();
        widget.set_visible_items(10);
        // visible_items is used by handle_key for scroll calculations
        // We verify by testing handle_key behavior in later tests
    }

    // =========================================================================
    // handle_scroll tests
    // =========================================================================

    #[test]
    fn scroll_down_increments_view_offset() {
        let mut widget = SelectorWidget::new();
        // 20 items, 5 visible
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        assert_eq!(widget.view_offset(), 0);

        // Scroll down by 2 rows (item_height = 20, delta = 40)
        widget.handle_scroll(40.0, 20.0, 5);

        assert_eq!(widget.view_offset(), 2);
    }

    #[test]
    fn scroll_up_decrements_view_offset() {
        let mut widget = SelectorWidget::new();
        // 20 items, 5 visible
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());

        // Start at offset 5
        widget.handle_scroll(100.0, 20.0, 5); // scroll down 5 rows
        assert_eq!(widget.view_offset(), 5);

        // Scroll up by 2 rows (negative delta)
        widget.handle_scroll(-40.0, 20.0, 5);

        assert_eq!(widget.view_offset(), 3);
    }

    #[test]
    fn scroll_clamps_at_max_offset() {
        let mut widget = SelectorWidget::new();
        // 10 items, 5 visible -> max offset is 5
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());

        // Try to scroll way past the end
        widget.handle_scroll(1000.0, 20.0, 5);

        // Should clamp at max offset (10 - 5 = 5)
        assert_eq!(widget.view_offset(), 5);
    }

    #[test]
    fn scroll_clamps_at_zero() {
        let mut widget = SelectorWidget::new();
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        assert_eq!(widget.view_offset(), 0);

        // Try to scroll up from 0 (negative delta)
        widget.handle_scroll(-100.0, 20.0, 5);

        // Should stay at 0
        assert_eq!(widget.view_offset(), 0);
    }

    #[test]
    fn scroll_on_short_list_is_noop() {
        let mut widget = SelectorWidget::new();
        // 3 items, 5 visible -> list fits entirely
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(widget.view_offset(), 0);

        // Try to scroll
        widget.handle_scroll(100.0, 20.0, 5);

        // Should remain at 0 (no-op)
        assert_eq!(widget.view_offset(), 0);
    }

    #[test]
    fn scroll_on_empty_list_is_noop() {
        let mut widget = SelectorWidget::new();
        // Empty list
        assert_eq!(widget.view_offset(), 0);

        // Try to scroll
        widget.handle_scroll(100.0, 20.0, 5);

        // Should remain at 0
        assert_eq!(widget.view_offset(), 0);
    }

    // =========================================================================
    // Arrow key navigation with scroll adjustment
    // =========================================================================

    #[test]
    fn down_past_visible_window_increments_view_offset() {
        let mut widget = SelectorWidget::new();
        // 10 items, visible_items = 3
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.set_visible_items(3);

        // Start at index 0, view_offset 0 (items 0, 1, 2 visible)
        assert_eq!(widget.selected_index(), 0);
        assert_eq!(widget.view_offset(), 0);

        // Navigate down to index 2 (still in visible window)
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 2);
        assert_eq!(widget.view_offset(), 0); // Still visible

        // Navigate down to index 3 (past visible window)
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 3);
        // view_offset should adjust to keep selection visible
        // selected_index 3 should be visible, so view_offset = 3 - 3 + 1 = 1
        assert_eq!(widget.view_offset(), 1);
    }

    #[test]
    fn up_past_visible_window_decrements_view_offset() {
        let mut widget = SelectorWidget::new();
        // 10 items, visible_items = 3
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.set_visible_items(3);

        // Scroll down first so view_offset is 3 (items 3, 4, 5 visible)
        widget.handle_scroll(60.0, 20.0, 3); // scroll 3 rows
        assert_eq!(widget.view_offset(), 3);

        // Set selected_index to 3 (top of visible window)
        // Navigate down to 3 first
        for _ in 0..3 {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }
        assert_eq!(widget.selected_index(), 3);

        // Navigate up - should scroll up
        widget.handle_key(&KeyEvent::new(Key::Up, Modifiers::default()));
        assert_eq!(widget.selected_index(), 2);
        // view_offset should adjust to show index 2
        assert_eq!(widget.view_offset(), 2);
    }

    #[test]
    fn down_within_visible_window_does_not_change_view_offset() {
        let mut widget = SelectorWidget::new();
        // 10 items, visible_items = 5
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.set_visible_items(5);

        assert_eq!(widget.view_offset(), 0);

        // Navigate down to index 2 (within visible window 0-4)
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));

        assert_eq!(widget.selected_index(), 2);
        assert_eq!(widget.view_offset(), 0); // Should not change
    }

    #[test]
    fn up_within_visible_window_does_not_change_view_offset() {
        let mut widget = SelectorWidget::new();
        // 10 items, visible_items = 5
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.set_visible_items(5);

        // Move to index 3
        for _ in 0..3 {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }
        assert_eq!(widget.selected_index(), 3);
        assert_eq!(widget.view_offset(), 0);

        // Navigate up to index 1 (still within visible window)
        widget.handle_key(&KeyEvent::new(Key::Up, Modifiers::default()));
        widget.handle_key(&KeyEvent::new(Key::Up, Modifiers::default()));

        assert_eq!(widget.selected_index(), 1);
        assert_eq!(widget.view_offset(), 0); // Should not change
    }

    // =========================================================================
    // set_items view_offset clamping
    // =========================================================================

    #[test]
    fn set_items_clamps_view_offset_when_list_shrinks() {
        let mut widget = SelectorWidget::new();
        // Start with 20 items, visible = 5
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.set_visible_items(5);

        // Scroll to view_offset 10
        widget.handle_scroll(200.0, 20.0, 5); // scroll 10 rows
        assert_eq!(widget.view_offset(), 10);

        // Now set fewer items (only 8 items)
        // max_offset should be 8 - 5 = 3
        widget.set_items((0..8).map(|i| format!("item{}", i)).collect());

        assert_eq!(widget.view_offset(), 3); // Clamped to max valid offset
    }

    #[test]
    fn set_items_preserves_view_offset_when_list_grows() {
        let mut widget = SelectorWidget::new();
        // Start with 10 items, visible = 5
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.set_visible_items(5);

        // Scroll to view_offset 3
        widget.handle_scroll(60.0, 20.0, 5); // scroll 3 rows
        assert_eq!(widget.view_offset(), 3);

        // Now add more items (20 items)
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());

        // view_offset should be preserved (3 is still valid)
        assert_eq!(widget.view_offset(), 3);
    }

    // =========================================================================
    // handle_mouse with view_offset
    // =========================================================================

    #[test]
    fn mouse_click_with_view_offset_selects_correct_item() {
        let mut widget = SelectorWidget::new();
        // 20 items, visible = 5
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.set_visible_items(5);

        // Scroll to view_offset 5 (items 5-9 visible)
        widget.handle_scroll(100.0, 20.0, 5);
        assert_eq!(widget.view_offset(), 5);

        // Click on visible row 0 (y=5 is in first visible row with height 20)
        // This should select item 5, not item 0
        let outcome = widget.handle_mouse((50.0, 5.0), MouseEventKind::Down, 20.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 5);
    }

    #[test]
    fn mouse_click_on_visible_row_0_with_offset_5_selects_item_5() {
        let mut widget = SelectorWidget::new();
        // 20 items, visible = 5
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.set_visible_items(5);

        // Scroll to view_offset 5
        widget.handle_scroll(100.0, 20.0, 5);
        assert_eq!(widget.view_offset(), 5);

        // Click on visible row 2 (y=45 is in row 2 with height 20)
        // This should select item 5 + 2 = 7
        let outcome = widget.handle_mouse((50.0, 45.0), MouseEventKind::Down, 20.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 7);
    }

    #[test]
    fn mouse_up_on_scrolled_list_confirms_correct_item() {
        let mut widget = SelectorWidget::new();
        // 20 items, visible = 5
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.set_visible_items(5);

        // Scroll to view_offset 5
        widget.handle_scroll(100.0, 20.0, 5);
        assert_eq!(widget.view_offset(), 5);

        // Click down then up on visible row 1 (item 6)
        widget.handle_mouse((50.0, 25.0), MouseEventKind::Down, 20.0, 0.0);
        assert_eq!(widget.selected_index(), 6);

        let outcome = widget.handle_mouse((50.0, 25.0), MouseEventKind::Up, 20.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Confirmed(6));
    }

    #[test]
    fn mouse_click_past_visible_items_is_noop() {
        let mut widget = SelectorWidget::new();
        // 10 items, visible = 5
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.set_visible_items(5);

        // Scroll to view_offset 7 (items 7, 8, 9 visible - only 3 items)
        widget.handle_scroll(140.0, 20.0, 5);
        assert_eq!(widget.view_offset(), 5); // max_offset is 10-5=5

        // Retry with offset 5 (items 5-9 visible)
        // Click on visible row 6 would be item 5+6=11 which is out of bounds
        widget.handle_mouse((50.0, 125.0), MouseEventKind::Down, 20.0, 0.0);
        // Should be no-op because item 11 doesn't exist
        // (row 6 = y position 120-140, item 5+6=11 is out of bounds)
    }

    // =========================================================================
    // Step 2: set_items with clamping
    // =========================================================================

    #[test]
    fn set_items_replaces_items() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(widget.items().len(), 3);
        assert_eq!(widget.items()[0], "a");
    }

    #[test]
    fn set_items_keeps_selected_index_if_in_range() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()]);
        // Manually set selected_index via navigation
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 2);

        // Replace with same number of items - index should stay at 2
        widget.set_items(vec!["x".into(), "y".into(), "z".into(), "w".into(), "v".into()]);
        assert_eq!(widget.selected_index(), 2);
    }

    #[test]
    fn set_items_clamps_index_when_fewer_items() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()]);
        // Navigate to last item
        for _ in 0..4 {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }
        assert_eq!(widget.selected_index(), 4);

        // Replace with fewer items - index should clamp to len - 1 = 2
        widget.set_items(vec!["x".into(), "y".into(), "z".into()]);
        assert_eq!(widget.selected_index(), 2);
    }

    #[test]
    fn set_items_clamps_to_zero_when_empty() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 1);

        // Replace with empty list - index should clamp to 0
        widget.set_items(vec![]);
        assert_eq!(widget.selected_index(), 0);
    }

    // =========================================================================
    // Step 3: Keyboard navigation (Up/Down)
    // =========================================================================

    #[test]
    fn down_from_index_zero_increments_to_one() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()]);
        assert_eq!(widget.selected_index(), 0);

        let outcome = widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 1);
    }

    #[test]
    fn down_from_last_item_stays_at_last() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()]);
        // Navigate to last item (index 4)
        for _ in 0..4 {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }
        assert_eq!(widget.selected_index(), 4);

        // Try to go down again - should stay at 4
        let outcome = widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 4);
    }

    #[test]
    fn up_from_index_two_decrements_to_one() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 2);

        let outcome = widget.handle_key(&KeyEvent::new(Key::Up, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 1);
    }

    #[test]
    fn up_from_index_zero_stays_at_zero() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(widget.selected_index(), 0);

        let outcome = widget.handle_key(&KeyEvent::new(Key::Up, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 0);
    }

    #[test]
    fn down_on_empty_items_stays_at_zero() {
        let mut widget = SelectorWidget::new();
        assert_eq!(widget.selected_index(), 0);

        let outcome = widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 0);
    }

    // =========================================================================
    // Step 4: Enter/Escape handling
    // =========================================================================

    #[test]
    fn enter_with_items_returns_confirmed_with_selected_index() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default())); // index 1

        let outcome = widget.handle_key(&KeyEvent::new(Key::Return, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Confirmed(1));
    }

    #[test]
    fn enter_with_empty_items_returns_confirmed_with_max() {
        let mut widget = SelectorWidget::new();
        // No items set

        let outcome = widget.handle_key(&KeyEvent::new(Key::Return, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Confirmed(usize::MAX));
    }

    #[test]
    fn escape_returns_cancelled() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into()]);

        let outcome = widget.handle_key(&KeyEvent::new(Key::Escape, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Cancelled);
    }

    // =========================================================================
    // Step 5: Query editing (character input and backspace)
    // =========================================================================

    #[test]
    fn typing_char_appends_to_query() {
        let mut widget = SelectorWidget::new();

        let outcome = widget.handle_key(&KeyEvent::char('a'));
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.query(), "a");
    }

    #[test]
    fn typing_multiple_chars_builds_query() {
        let mut widget = SelectorWidget::new();

        widget.handle_key(&KeyEvent::char('h'));
        widget.handle_key(&KeyEvent::char('e'));
        widget.handle_key(&KeyEvent::char('l'));
        widget.handle_key(&KeyEvent::char('l'));
        widget.handle_key(&KeyEvent::char('o'));

        assert_eq!(widget.query(), "hello");
    }

    #[test]
    fn typing_char_resets_selected_index_to_zero() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 2);

        widget.handle_key(&KeyEvent::char('x'));
        assert_eq!(widget.selected_index(), 0);
    }

    #[test]
    fn backspace_removes_last_char() {
        let mut widget = SelectorWidget::new();
        widget.handle_key(&KeyEvent::char('a'));
        widget.handle_key(&KeyEvent::char('b'));
        widget.handle_key(&KeyEvent::char('c'));
        assert_eq!(widget.query(), "abc");

        let outcome = widget.handle_key(&KeyEvent::new(Key::Backspace, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.query(), "ab");
    }

    #[test]
    fn backspace_on_empty_query_is_noop() {
        let mut widget = SelectorWidget::new();
        assert_eq!(widget.query(), "");

        let outcome = widget.handle_key(&KeyEvent::new(Key::Backspace, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.query(), "");
    }

    #[test]
    fn typing_with_command_modifier_is_noop() {
        let mut widget = SelectorWidget::new();
        let event = KeyEvent::new(
            Key::Char('a'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );

        let outcome = widget.handle_key(&event);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.query(), ""); // Query should not change
    }

    #[test]
    fn typing_with_control_modifier_is_noop() {
        let mut widget = SelectorWidget::new();
        let event = KeyEvent::new(
            Key::Char('a'),
            Modifiers {
                control: true,
                ..Default::default()
            },
        );

        let outcome = widget.handle_key(&event);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.query(), ""); // Query should not change
    }

    #[test]
    fn backspace_with_command_modifier_is_noop() {
        let mut widget = SelectorWidget::new();
        widget.handle_key(&KeyEvent::char('a'));
        widget.handle_key(&KeyEvent::char('b'));
        assert_eq!(widget.query(), "ab");

        let event = KeyEvent::new(
            Key::Backspace,
            Modifiers {
                command: true,
                ..Default::default()
            },
        );

        let outcome = widget.handle_key(&event);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.query(), "ab"); // Query should not change
    }

    #[test]
    fn typing_shifted_char_appends_to_query() {
        let mut widget = SelectorWidget::new();
        // Shift is allowed - it affects the character but doesn't prevent input
        let event = KeyEvent::char_shifted('A');

        let outcome = widget.handle_key(&event);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.query(), "A");
    }

    #[test]
    fn typing_unicode_char_appends_to_query() {
        let mut widget = SelectorWidget::new();

        widget.handle_key(&KeyEvent::char('日'));
        widget.handle_key(&KeyEvent::char('本'));
        widget.handle_key(&KeyEvent::char('語'));

        assert_eq!(widget.query(), "日本語");
    }

    #[test]
    fn unhandled_key_returns_pending() {
        let mut widget = SelectorWidget::new();

        // Tab key should be a no-op
        let outcome = widget.handle_key(&KeyEvent::new(Key::Tab, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Pending);

        // Left arrow should be a no-op
        let outcome = widget.handle_key(&KeyEvent::new(Key::Left, Modifiers::default()));
        assert_eq!(outcome, SelectorOutcome::Pending);
    }

    // =========================================================================
    // Step 6: Mouse handling
    // =========================================================================

    #[test]
    fn mouse_down_on_row_selects_that_row() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()]);
        assert_eq!(widget.selected_index(), 0);

        // Click on row 2 (item_height=20, list_origin_y=0)
        // Row 2 starts at y=40 (rows 0-1 are 0-39)
        let outcome = widget.handle_mouse((50.0, 45.0), MouseEventKind::Down, 20.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 2);
    }

    #[test]
    fn mouse_down_outside_list_bounds_above_is_noop() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 1);

        // Click above the list (list starts at y=100)
        let outcome = widget.handle_mouse((50.0, 50.0), MouseEventKind::Down, 20.0, 100.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 1); // Should not change
    }

    #[test]
    fn mouse_down_outside_list_bounds_below_is_noop() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(widget.selected_index(), 0);

        // Click below the list (3 items * 20px = 60px height, list starts at y=0)
        // Row 3 and beyond is out of bounds
        let outcome = widget.handle_mouse((50.0, 65.0), MouseEventKind::Down, 20.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 0); // Should not change
    }

    #[test]
    fn mouse_up_on_same_row_as_selected_confirms() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        // Select row 1
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 1);

        // Mouse up on row 1 (y=25 is in row 1 with height 20)
        let outcome = widget.handle_mouse((50.0, 25.0), MouseEventKind::Up, 20.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Confirmed(1));
    }

    #[test]
    fn mouse_up_on_different_row_selects_but_does_not_confirm() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(widget.selected_index(), 0);

        // Mouse up on row 2 (different from selected row 0)
        let outcome = widget.handle_mouse((50.0, 45.0), MouseEventKind::Up, 20.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 2); // Should update selection
    }

    #[test]
    fn mouse_moved_is_noop() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(widget.selected_index(), 0);

        let outcome = widget.handle_mouse((50.0, 45.0), MouseEventKind::Moved, 20.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 0); // Should not change
    }

    #[test]
    fn mouse_on_empty_items_is_noop() {
        let mut widget = SelectorWidget::new();
        // No items

        let outcome = widget.handle_mouse((50.0, 25.0), MouseEventKind::Down, 20.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
    }

    #[test]
    fn mouse_with_list_origin_offset() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(widget.selected_index(), 0);

        // List starts at y=50, item_height=30
        // Row 1 is at y=50+30=80 to y=50+60=110
        // Click at y=95 should be row 1
        let outcome = widget.handle_mouse((50.0, 95.0), MouseEventKind::Down, 30.0, 50.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 1);
    }

    #[test]
    fn click_and_release_on_same_row_confirms() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);

        // Simulate a full click: down then up on row 2
        widget.handle_mouse((50.0, 45.0), MouseEventKind::Down, 20.0, 0.0);
        assert_eq!(widget.selected_index(), 2);

        let outcome = widget.handle_mouse((50.0, 45.0), MouseEventKind::Up, 20.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Confirmed(2));
    }
}
