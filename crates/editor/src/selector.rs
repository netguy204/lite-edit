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

    /// Replaces the item list and clamps the selected index to valid bounds.
    ///
    /// If the new list has fewer items than the current `selected_index`,
    /// the index is clamped to `new_items.len() - 1` (or 0 if empty).
    pub fn set_items(&mut self, items: Vec<String>) {
        self.items = items;
        // Clamp selected_index to valid range
        if self.items.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.items.len() - 1);
        }
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
                SelectorOutcome::Pending
            }
            Key::Down => {
                if !self.items.is_empty() {
                    let max_index = self.items.len() - 1;
                    if self.selected_index < max_index {
                        self.selected_index += 1;
                    }
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
    /// - **Down on a list row**: Sets `selected_index` to that row, returns `Pending`.
    /// - **Up on same row as `selected_index`**: Returns `Confirmed(selected_index)`.
    /// - **Up on different row**: Sets `selected_index` to that row, returns `Pending`.
    /// - **Outside list bounds** (above or below the list): Returns `Pending` (no-op).
    /// - **Moved**: Returns `Pending` (no-op).
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

        // Compute which row was clicked
        let relative_y = position.1 - list_origin_y;
        let row = (relative_y / item_height) as usize;

        // Check if row is within valid range
        if row >= self.items.len() {
            return SelectorOutcome::Pending;
        }

        match kind {
            MouseEventKind::Down => {
                self.selected_index = row;
                SelectorOutcome::Pending
            }
            MouseEventKind::Up => {
                if row == self.selected_index {
                    SelectorOutcome::Confirmed(self.selected_index)
                } else {
                    self.selected_index = row;
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
