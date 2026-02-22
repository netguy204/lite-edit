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

use crate::font::FontMetrics;
use crate::input::{Key, KeyEvent, MouseEventKind};
use crate::mini_buffer::MiniBuffer;
use crate::row_scroller::RowScroller;

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
// Chunk: docs/chunks/file_picker_mini_buffer - MiniBuffer-backed query editing
pub struct SelectorWidget {
    /// Single-line MiniBuffer for query editing with full affordance set
    /// (word-jump, kill-line, shift-selection, clipboard, Emacs bindings).
    mini_buffer: MiniBuffer,
    /// The current list of displayable strings.
    items: Vec<String>,
    /// Index into `items` of the currently highlighted entry.
    /// Always clamped to valid bounds (0..items.len(), or 0 if empty).
    selected_index: usize,
    /// Scroll state for the item list, providing fractional-pixel scroll tracking.
    scroll: RowScroller,
}

impl Default for SelectorWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectorWidget {
    /// Creates a new selector widget with empty query, no items, and index 0.
    // Chunk: docs/chunks/file_picker_mini_buffer - Zero-argument constructor with default FontMetrics
    pub fn new() -> Self {
        // Default metrics for MiniBuffer (values don't affect query behavior,
        // only internal viewport calculations which aren't used by selector)
        let metrics = FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        };
        Self {
            mini_buffer: MiniBuffer::new(metrics),
            items: Vec::new(),
            selected_index: 0,
            scroll: RowScroller::new(metrics.line_height as f32),
        }
    }

    /// Returns the current query string.
    // Chunk: docs/chunks/file_picker_mini_buffer - Query accessor delegating to mini_buffer.content()
    pub fn query(&self) -> String {
        self.mini_buffer.content()
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

    // Chunk: docs/chunks/file_picker_scroll - Setter for visible area height
    /// Updates the visible size from the pixel height of the list area.
    ///
    /// This forwards to `RowScroller::update_size(height_px, row_count)`, which computes
    /// visible_rows from `height_px / row_height` and clamps scroll offset.
    pub fn update_visible_size(&mut self, height_px: f32) {
        self.scroll.update_size(height_px, self.items.len());
    }

    /// Sets the row height (item height) in pixels.
    ///
    /// Call this when font metrics change.
    pub fn set_item_height(&mut self, height: f32) {
        // Reconstruct the scroller with the new row height, preserving scroll state
        let offset = self.scroll.scroll_offset_px();
        let visible_rows = self.scroll.visible_rows();
        self.scroll = RowScroller::new(height);
        self.scroll.update_size(visible_rows as f32 * height, self.items.len());
        self.scroll.set_scroll_offset_px(offset, self.items.len());
    }

    // Chunk: docs/chunks/file_picker_scroll - Clamps scroll offset when item list shrinks
    /// Replaces the item list and clamps the selected index and scroll offset to valid bounds.
    ///
    /// If the new list has fewer items than the current `selected_index`,
    /// the index is clamped to `new_items.len() - 1` (or 0 if empty).
    ///
    /// The scroll offset is re-clamped to the new item count without resetting
    /// to zero (e.g., after a query narrows results).
    pub fn set_items(&mut self, items: Vec<String>) {
        self.items = items;
        // Clamp selected_index to valid range
        if self.items.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.items.len() - 1);
        }
        // Re-clamp scroll offset to new item count without resetting to zero
        let px = self.scroll.scroll_offset_px();
        self.scroll.set_scroll_offset_px(px, self.items.len());
    }

    /// Handles a keyboard event and returns the appropriate outcome.
    ///
    /// # Behavior
    ///
    /// - **Up arrow**: Decrements `selected_index` (floor at 0), returns `Pending`.
    /// - **Down arrow**: Increments `selected_index` (ceil at `items.len() - 1`), returns `Pending`.
    /// - **Return/Enter**: Returns `Confirmed(selected_index)`, or `Confirmed(usize::MAX)` if items is empty.
    /// - **Escape**: Returns `Cancelled`.
    /// - **All other keys**: Delegated to `MiniBuffer` for query editing. If the query
    ///   changes, resets `selected_index` to 0. Returns `Pending`.
    ///
    /// The MiniBuffer provides full editing affordances: character input, backspace,
    /// word navigation (Option+Left/Right), kill-line (Ctrl+K), selection (Shift+arrows),
    /// clipboard operations (Cmd+C/V/X), and Emacs-style bindings (Ctrl+A/E/K).
    // Chunk: docs/chunks/file_picker_mini_buffer - Key handling with MiniBuffer delegation
    // Chunk: docs/chunks/file_picker_scroll - Keeps selection visible when navigating
    pub fn handle_key(&mut self, event: &KeyEvent) -> SelectorOutcome {
        match &event.key {
            Key::Up => {
                self.selected_index = self.selected_index.saturating_sub(1);
                self.scroll.ensure_visible(self.selected_index, self.items.len());
                SelectorOutcome::Pending
            }
            Key::Down => {
                if !self.items.is_empty() {
                    let max_index = self.items.len() - 1;
                    if self.selected_index < max_index {
                        self.selected_index += 1;
                    }
                }
                self.scroll.ensure_visible(self.selected_index, self.items.len());
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
            _ => {
                // Delegate all other keys to MiniBuffer
                let prev_query = self.mini_buffer.content();
                self.mini_buffer.handle_key(event.clone());
                if self.mini_buffer.content() != prev_query {
                    self.selected_index = 0;
                }
                SelectorOutcome::Pending
            }
        }
    }

    // Chunk: docs/chunks/file_picker_scroll - Translates pixel deltas into scroll offset
    /// Handles a scroll event by adjusting the scroll offset.
    ///
    /// # Arguments
    ///
    /// * `delta_y` - The raw pixel delta (positive = scroll down / content moves up).
    ///
    /// # Behavior
    ///
    /// Accumulates the raw pixel delta via `RowScroller::set_scroll_offset_px`.
    /// No rounding to row boundaries — fractional positions are preserved for
    /// smooth scrolling.
    pub fn handle_scroll(&mut self, delta_y: f64) {
        let new_px = self.scroll.scroll_offset_px() + delta_y as f32;
        self.scroll.set_scroll_offset_px(new_px, self.items.len());
    }

    // Chunk: docs/chunks/file_picker_scroll - Maps visible row to actual item index
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
    ///   scroll offset), returns `Pending`.
    /// - **Up on same row as `selected_index`**: Returns `Confirmed(selected_index)`.
    /// - **Up on different row**: Sets `selected_index` to that row, returns `Pending`.
    /// - **Outside list bounds** (above or below the list): Returns `Pending` (no-op).
    /// - **Moved**: Returns `Pending` (no-op).
    ///
    /// Note: The hit-testing accounts for fractional scroll offset to ensure the
    /// clicked pixel maps to the same item the renderer draws at that position.
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

        // Compute which item was clicked, accounting for fractional scroll offset
        let first = self.scroll.first_visible_row();
        let frac = self.scroll.scroll_fraction_px() as f64;
        let relative_y = position.1 - list_origin_y + frac;
        let row = (relative_y / item_height).floor() as usize;
        let item_index = first + row;

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

    // =========================================================================
    // New public accessors for RowScroller-based scroll state
    // =========================================================================

    // Chunk: docs/chunks/file_picker_scroll - Accessor for first visible item index
    /// Returns the index of the first visible item.
    ///
    /// Delegates to `RowScroller::first_visible_row()`.
    pub fn first_visible_item(&self) -> usize {
        self.scroll.first_visible_row()
    }

    /// Returns the fractional pixel offset within the top row.
    ///
    /// Delegates to `RowScroller::scroll_fraction_px()`. Renderers use this
    /// to offset item drawing for smooth sub-row scrolling.
    pub fn scroll_fraction_px(&self) -> f32 {
        self.scroll.scroll_fraction_px()
    }

    /// Returns the range of items visible in the viewport.
    ///
    /// Delegates to `RowScroller::visible_range(item_count)`. The range
    /// includes partially visible items at the top and bottom.
    pub fn visible_item_range(&self) -> std::ops::Range<usize> {
        self.scroll.visible_range(self.items.len())
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
    // Scroll support: first_visible_item and update_visible_size
    // =========================================================================

    #[test]
    fn new_widget_has_first_visible_item_zero() {
        let widget = SelectorWidget::new();
        assert_eq!(widget.first_visible_item(), 0);
    }

    #[test]
    fn new_widget_has_zero_scroll_fraction() {
        let widget = SelectorWidget::new();
        assert!((widget.scroll_fraction_px() - 0.0).abs() < 0.001);
    }

    #[test]
    fn update_visible_size_sets_visible_rows() {
        let mut widget = SelectorWidget::new();
        // row_height is 16.0 from default FontMetrics
        widget.update_visible_size(80.0); // 80 / 16 = 5 visible rows
        // We verify by checking that scrolling works correctly
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        // Scroll 5 rows (80 pixels)
        widget.handle_scroll(80.0);
        assert_eq!(widget.first_visible_item(), 5);
    }

    // =========================================================================
    // handle_scroll tests
    // =========================================================================

    #[test]
    fn scroll_down_increments_first_visible_item() {
        let mut widget = SelectorWidget::new();
        // 20 items, 5 visible (row_height = 16.0)
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows
        assert_eq!(widget.first_visible_item(), 0);

        // Scroll down by 2 rows (row_height = 16, delta = 32)
        widget.handle_scroll(32.0);

        assert_eq!(widget.first_visible_item(), 2);
    }

    #[test]
    fn scroll_up_decrements_first_visible_item() {
        let mut widget = SelectorWidget::new();
        // 20 items, 5 visible
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Start at row 5 (scroll 80 pixels)
        widget.handle_scroll(80.0);
        assert_eq!(widget.first_visible_item(), 5);

        // Scroll up by 2 rows (negative delta)
        widget.handle_scroll(-32.0);

        assert_eq!(widget.first_visible_item(), 3);
    }

    #[test]
    fn scroll_clamps_at_max_offset() {
        let mut widget = SelectorWidget::new();
        // 10 items, 5 visible -> max offset is 5
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Try to scroll way past the end
        widget.handle_scroll(1000.0);

        // Should clamp at max offset (10 - 5 = 5)
        assert_eq!(widget.first_visible_item(), 5);
    }

    #[test]
    fn scroll_clamps_at_zero() {
        let mut widget = SelectorWidget::new();
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0);
        assert_eq!(widget.first_visible_item(), 0);

        // Try to scroll up from 0 (negative delta)
        widget.handle_scroll(-100.0);

        // Should stay at 0
        assert_eq!(widget.first_visible_item(), 0);
    }

    #[test]
    fn scroll_on_short_list_is_noop() {
        let mut widget = SelectorWidget::new();
        // 3 items, 5 visible -> list fits entirely
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.update_visible_size(80.0);
        assert_eq!(widget.first_visible_item(), 0);

        // Try to scroll
        widget.handle_scroll(100.0);

        // Should remain at 0 (no-op)
        assert_eq!(widget.first_visible_item(), 0);
    }

    #[test]
    fn scroll_on_empty_list_is_noop() {
        let mut widget = SelectorWidget::new();
        // Empty list
        widget.update_visible_size(80.0);
        assert_eq!(widget.first_visible_item(), 0);

        // Try to scroll
        widget.handle_scroll(100.0);

        // Should remain at 0
        assert_eq!(widget.first_visible_item(), 0);
    }

    // =========================================================================
    // Arrow key navigation with scroll adjustment
    // =========================================================================

    #[test]
    fn down_past_visible_window_increments_first_visible_item() {
        let mut widget = SelectorWidget::new();
        // 10 items, visible_rows = 3 (row_height = 16)
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(48.0); // 3 visible rows

        // Start at index 0, first_visible_item 0 (items 0, 1, 2 visible)
        assert_eq!(widget.selected_index(), 0);
        assert_eq!(widget.first_visible_item(), 0);

        // Navigate down to index 2 (still in visible window)
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 2);
        assert_eq!(widget.first_visible_item(), 0); // Still visible

        // Navigate down to index 3 (past visible window)
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 3);
        // first_visible_item should adjust to keep selection visible
        // selected_index 3 should be visible, so first_visible_item = 3 - 3 + 1 = 1
        assert_eq!(widget.first_visible_item(), 1);
    }

    #[test]
    fn up_past_visible_window_decrements_first_visible_item() {
        let mut widget = SelectorWidget::new();
        // 10 items, visible_rows = 3
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(48.0); // 3 visible rows

        // Navigate down to item 5 first (this will auto-scroll as we go)
        for _ in 0..5 {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }
        assert_eq!(widget.selected_index(), 5);
        // first_visible_item should have scrolled to keep index 5 visible
        // With visible_rows=3, keeping index 5 visible means first_visible_item=3
        assert_eq!(widget.first_visible_item(), 3);

        // Navigate up - selected_index goes from 5 to 4, still visible in window 3-5
        widget.handle_key(&KeyEvent::new(Key::Up, Modifiers::default()));
        assert_eq!(widget.selected_index(), 4);
        assert_eq!(widget.first_visible_item(), 3); // No change needed

        // Navigate up again - selected_index goes to 3, still visible
        widget.handle_key(&KeyEvent::new(Key::Up, Modifiers::default()));
        assert_eq!(widget.selected_index(), 3);
        assert_eq!(widget.first_visible_item(), 3); // No change needed

        // Navigate up again - selected_index goes to 2, which is above the window
        widget.handle_key(&KeyEvent::new(Key::Up, Modifiers::default()));
        assert_eq!(widget.selected_index(), 2);
        // first_visible_item should adjust to show index 2
        assert_eq!(widget.first_visible_item(), 2);
    }

    #[test]
    fn down_within_visible_window_does_not_change_first_visible_item() {
        let mut widget = SelectorWidget::new();
        // 10 items, visible_rows = 5
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        assert_eq!(widget.first_visible_item(), 0);

        // Navigate down to index 2 (within visible window 0-4)
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));

        assert_eq!(widget.selected_index(), 2);
        assert_eq!(widget.first_visible_item(), 0); // Should not change
    }

    #[test]
    fn up_within_visible_window_does_not_change_first_visible_item() {
        let mut widget = SelectorWidget::new();
        // 10 items, visible_rows = 5
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Move to index 3
        for _ in 0..3 {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }
        assert_eq!(widget.selected_index(), 3);
        assert_eq!(widget.first_visible_item(), 0);

        // Navigate up to index 1 (still within visible window)
        widget.handle_key(&KeyEvent::new(Key::Up, Modifiers::default()));
        widget.handle_key(&KeyEvent::new(Key::Up, Modifiers::default()));

        assert_eq!(widget.selected_index(), 1);
        assert_eq!(widget.first_visible_item(), 0); // Should not change
    }

    // =========================================================================
    // set_items scroll offset clamping
    // =========================================================================

    #[test]
    fn set_items_clamps_scroll_offset_when_list_shrinks() {
        let mut widget = SelectorWidget::new();
        // Start with 20 items, visible = 5 (row_height = 16)
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Scroll to row 10 (160 pixels)
        widget.handle_scroll(160.0);
        assert_eq!(widget.first_visible_item(), 10);

        // Now set fewer items (only 8 items)
        // max_offset should be 8 - 5 = 3
        widget.set_items((0..8).map(|i| format!("item{}", i)).collect());

        assert_eq!(widget.first_visible_item(), 3); // Clamped to max valid offset
    }

    #[test]
    fn set_items_preserves_scroll_offset_when_list_grows() {
        let mut widget = SelectorWidget::new();
        // Start with 10 items, visible = 5
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Scroll to row 3 (48 pixels)
        widget.handle_scroll(48.0);
        assert_eq!(widget.first_visible_item(), 3);

        // Now add more items (20 items)
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());

        // first_visible_item should be preserved (3 is still valid)
        assert_eq!(widget.first_visible_item(), 3);
    }

    // =========================================================================
    // handle_mouse with scroll offset
    // =========================================================================

    #[test]
    fn mouse_click_with_scroll_offset_selects_correct_item() {
        let mut widget = SelectorWidget::new();
        // 20 items, visible = 5 (row_height = 16)
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Scroll to row 5 (80 pixels)
        widget.handle_scroll(80.0);
        assert_eq!(widget.first_visible_item(), 5);

        // Click on visible row 0 (y=5 is in first visible row with height 16)
        // This should select item 5, not item 0
        let outcome = widget.handle_mouse((50.0, 5.0), MouseEventKind::Down, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 5);
    }

    #[test]
    fn mouse_click_on_visible_row_0_with_offset_5_selects_item_5() {
        let mut widget = SelectorWidget::new();
        // 20 items, visible = 5
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Scroll to row 5
        widget.handle_scroll(80.0);
        assert_eq!(widget.first_visible_item(), 5);

        // Click on visible row 2 (y=35 is in row 2 with height 16)
        // This should select item 5 + 2 = 7
        let outcome = widget.handle_mouse((50.0, 35.0), MouseEventKind::Down, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 7);
    }

    #[test]
    fn mouse_up_on_scrolled_list_confirms_correct_item() {
        let mut widget = SelectorWidget::new();
        // 20 items, visible = 5
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Scroll to row 5
        widget.handle_scroll(80.0);
        assert_eq!(widget.first_visible_item(), 5);

        // Click down then up on visible row 1 (item 6)
        // Row 1 is at y=16 to y=32 with height 16
        widget.handle_mouse((50.0, 20.0), MouseEventKind::Down, 16.0, 0.0);
        assert_eq!(widget.selected_index(), 6);

        let outcome = widget.handle_mouse((50.0, 20.0), MouseEventKind::Up, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Confirmed(6));
    }

    #[test]
    fn mouse_click_past_visible_items_is_noop() {
        let mut widget = SelectorWidget::new();
        // 10 items, visible = 5
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Scroll to max offset (items 5-9 visible)
        widget.handle_scroll(1000.0); // Will clamp
        assert_eq!(widget.first_visible_item(), 5); // max_offset is 10-5=5

        // Click on visible row 6 would be item 5+6=11 which is out of bounds
        // Row 6 starts at y=96 with height 16
        widget.handle_mouse((50.0, 100.0), MouseEventKind::Down, 16.0, 0.0);
        // Should be no-op because item 11 doesn't exist
        // (row 6 = y position 96-112, item 5+6=11 is out of bounds)
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
    fn backspace_with_command_modifier_deletes_to_start() {
        // With MiniBuffer integration, Cmd+Backspace now deletes to start of line
        // (this is a new affordance we gain from MiniBuffer)
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
        assert_eq!(widget.query(), ""); // Cmd+Backspace deletes to start of line
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
        widget.update_visible_size(80.0); // All items visible
        assert_eq!(widget.selected_index(), 0);

        // Click on row 2 (row_height=16, list_origin_y=0)
        // Row 2 starts at y=32 (rows 0-1 are 0-31)
        let outcome = widget.handle_mouse((50.0, 35.0), MouseEventKind::Down, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 2);
    }

    #[test]
    fn mouse_down_outside_list_bounds_above_is_noop() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.update_visible_size(80.0); // All items visible
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 1);

        // Click above the list (list starts at y=100)
        let outcome = widget.handle_mouse((50.0, 50.0), MouseEventKind::Down, 16.0, 100.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 1); // Should not change
    }

    #[test]
    fn mouse_down_outside_list_bounds_below_is_noop() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.update_visible_size(80.0); // All items visible
        assert_eq!(widget.selected_index(), 0);

        // Click below the list (3 items * 16px = 48px height, list starts at y=0)
        // Row 3 and beyond is out of bounds
        let outcome = widget.handle_mouse((50.0, 50.0), MouseEventKind::Down, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 0); // Should not change
    }

    #[test]
    fn mouse_up_on_same_row_as_selected_confirms() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.update_visible_size(80.0); // 5 visible rows (all items visible)
        // Select row 1
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 1);
        assert_eq!(widget.first_visible_item(), 0); // No scrolling needed

        // Mouse up on row 1 (y=20 is in row 1 with height 16, the default row_height)
        // Row 0: y=0..16, Row 1: y=16..32
        let outcome = widget.handle_mouse((50.0, 20.0), MouseEventKind::Up, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Confirmed(1));
    }

    #[test]
    fn mouse_up_on_different_row_selects_but_does_not_confirm() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.update_visible_size(80.0); // All items visible
        assert_eq!(widget.selected_index(), 0);

        // Mouse up on row 2 (different from selected row 0)
        // Row 2 is at y=32..48 with row_height=16
        let outcome = widget.handle_mouse((50.0, 35.0), MouseEventKind::Up, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 2); // Should update selection
    }

    #[test]
    fn mouse_moved_is_noop() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.update_visible_size(80.0); // All items visible
        assert_eq!(widget.selected_index(), 0);

        let outcome = widget.handle_mouse((50.0, 35.0), MouseEventKind::Moved, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 0); // Should not change
    }

    #[test]
    fn mouse_on_empty_items_is_noop() {
        let mut widget = SelectorWidget::new();
        widget.update_visible_size(80.0);
        // No items

        let outcome = widget.handle_mouse((50.0, 20.0), MouseEventKind::Down, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
    }

    #[test]
    fn mouse_with_list_origin_offset() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.update_visible_size(80.0); // All items visible
        assert_eq!(widget.selected_index(), 0);

        // List starts at y=50, row_height=16
        // Row 1 is at y=50+16=66 to y=50+32=82
        // Click at y=70 should be row 1
        let outcome = widget.handle_mouse((50.0, 70.0), MouseEventKind::Down, 16.0, 50.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 1);
    }

    #[test]
    fn click_and_release_on_same_row_confirms() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["a".into(), "b".into(), "c".into()]);
        widget.update_visible_size(80.0); // All items visible

        // Simulate a full click: down then up on row 2
        // Row 2 is at y=32..48 with row_height=16
        widget.handle_mouse((50.0, 35.0), MouseEventKind::Down, 16.0, 0.0);
        assert_eq!(widget.selected_index(), 2);

        let outcome = widget.handle_mouse((50.0, 35.0), MouseEventKind::Up, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Confirmed(2));
    }

    // =========================================================================
    // Smooth scroll accumulation tests
    // =========================================================================

    #[test]
    fn scroll_accumulates_sub_row_deltas() {
        let mut widget = SelectorWidget::new();
        // 20 items, row_height is 16.0 from default FontMetrics
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Scroll 5 pixels (less than one row)
        widget.handle_scroll(5.0);
        assert_eq!(widget.first_visible_item(), 0);
        assert!((widget.scroll_fraction_px() - 5.0).abs() < 0.001);

        // Scroll another 5 pixels (total 10, still less than row)
        widget.handle_scroll(5.0);
        assert_eq!(widget.first_visible_item(), 0);
        assert!((widget.scroll_fraction_px() - 10.0).abs() < 0.001);

        // Scroll 6 more pixels (total 16, exactly one row)
        widget.handle_scroll(6.0);
        assert_eq!(widget.first_visible_item(), 1);
        assert!((widget.scroll_fraction_px() - 0.0).abs() < 0.001);
    }

    #[test]
    fn scroll_preserves_fraction_across_row_boundary() {
        let mut widget = SelectorWidget::new();
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0);

        // Scroll 20 pixels (1 row + 4 pixels)
        widget.handle_scroll(20.0);
        assert_eq!(widget.first_visible_item(), 1);
        assert!((widget.scroll_fraction_px() - 4.0).abs() < 0.001);
    }

    #[test]
    fn visible_item_range_accounts_for_partial_visibility() {
        let mut widget = SelectorWidget::new();
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // At row 0 with no fraction
        let range = widget.visible_item_range();
        // Should include +1 for partially visible bottom row
        assert_eq!(range, 0..6);

        // Scroll to row 5
        widget.handle_scroll(80.0);
        let range = widget.visible_item_range();
        assert_eq!(range, 5..11);
    }

    // =========================================================================
    // Fractional scroll hit-testing tests
    // =========================================================================

    #[test]
    fn mouse_click_with_fractional_scroll_selects_correct_item() {
        let mut widget = SelectorWidget::new();
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows, row_height = 16

        // Scroll 8 pixels (half a row)
        widget.handle_scroll(8.0);
        assert_eq!(widget.first_visible_item(), 0);
        assert!((widget.scroll_fraction_px() - 8.0).abs() < 0.001);

        // Click at y=4 (within the visible portion of item 0, which starts at -8)
        // The visible portion of item 0 is y=0..8 on screen
        // relative_y + frac = 4 + 8 = 12, row = floor(12/16) = 0
        let outcome = widget.handle_mouse((50.0, 4.0), MouseEventKind::Down, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 0);

        // Click at y=10 (within item 1, which starts at y=8 on screen)
        // relative_y + frac = 10 + 8 = 18, row = floor(18/16) = 1
        let outcome = widget.handle_mouse((50.0, 10.0), MouseEventKind::Down, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 1);
    }

    #[test]
    fn mouse_click_near_row_boundary_with_fraction() {
        let mut widget = SelectorWidget::new();
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0);

        // Scroll 12 pixels (3/4 of a row)
        widget.handle_scroll(12.0);
        assert_eq!(widget.first_visible_item(), 0);

        // The visible portion of item 0 is now only y=0..4 on screen
        // Click at y=3 should still hit item 0
        // relative_y + frac = 3 + 12 = 15, row = floor(15/16) = 0
        let outcome = widget.handle_mouse((50.0, 3.0), MouseEventKind::Down, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 0);

        // Click at y=5 should hit item 1
        // relative_y + frac = 5 + 12 = 17, row = floor(17/16) = 1
        let outcome = widget.handle_mouse((50.0, 5.0), MouseEventKind::Down, 16.0, 0.0);
        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(widget.selected_index(), 1);
    }

    // =========================================================================
    // Chunk: selector_hittest_tests - Parameterised hit-testing property tests
    // =========================================================================

    /// Step 1: Parameterised property test verifying that clicking the pixel centre
    /// of any rendered row selects exactly that row.
    ///
    /// This parameterises over:
    /// - Multiple scroll_offset_px values (including non-zero fractional parts)
    /// - A range of item_height values
    /// - Clicking first, middle, and last visible items
    ///
    /// Chunk: docs/chunks/selector_hittest_tests
    #[test]
    fn click_row_centre_selects_that_row() {
        // Parameters: scroll offsets with fractional parts
        let scroll_offsets = [0.0_f64, 8.5, 17.2];
        // Parameters: item heights
        let item_heights = [16.0_f64, 20.0];
        // Visible rows for the test
        let visible_rows = 5_usize;
        // Test clicking first, middle, and last visible rows
        let clicked_rows = [0_usize, visible_rows / 2, visible_rows - 1];
        let list_origin_y = 100.0; // arbitrary non-zero origin

        for &scroll_offset_px in &scroll_offsets {
            for &item_height in &item_heights {
                for &clicked_visible_row in &clicked_rows {
                    // Set up a fresh widget for each test case
                    let mut widget = SelectorWidget::new();
                    widget.set_item_height(item_height as f32);
                    widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
                    widget.update_visible_size((visible_rows as f64 * item_height) as f32);

                    // Apply the scroll offset
                    widget.handle_scroll(scroll_offset_px);

                    let first_visible = widget.first_visible_item();
                    let scroll_frac = widget.scroll_fraction_px() as f64;

                    // Compute the pixel centre of the rendered row
                    // Renderer places row i at: y = list_origin_y - scroll_fraction_px + i * item_height
                    // Centre is at y + item_height / 2
                    let y = list_origin_y - scroll_frac
                        + clicked_visible_row as f64 * item_height
                        + item_height / 2.0;

                    let outcome = widget.handle_mouse(
                        (50.0, y),
                        MouseEventKind::Down,
                        item_height,
                        list_origin_y,
                    );

                    let expected_index = first_visible + clicked_visible_row;

                    assert_eq!(
                        outcome,
                        SelectorOutcome::Pending,
                        "scroll={}, height={}, row={}: expected Pending",
                        scroll_offset_px, item_height, clicked_visible_row
                    );
                    assert_eq!(
                        widget.selected_index(),
                        expected_index,
                        "scroll={}, height={}, clicked_row={}: expected index {}, got {}",
                        scroll_offset_px, item_height, clicked_visible_row,
                        expected_index, widget.selected_index()
                    );
                }
            }
        }
    }

    /// Step 2: Regression test for the coordinate-flip bug.
    ///
    /// This test documents and guards the coordinate system convention:
    /// `handle_mouse` expects already-flipped coordinates (done by `handle_mouse_selector`
    /// in buffer_target.rs via `view_height - raw_y`).
    ///
    /// A click near the top of the overlay (in flipped coordinates, y is small and near
    /// `list_origin_y`) must select a row near the top of the list. Before the
    /// `selector_coord_flip` fix, raw macOS y-coordinates were passed directly, causing
    /// clicks near the top of the screen to produce out-of-bounds indices.
    ///
    /// Chunk: docs/chunks/selector_hittest_tests
    #[test]
    fn coordinate_flip_regression_raw_y_near_top_selects_topmost() {
        let mut widget = SelectorWidget::new();
        let item_height = 16.0_f64;
        widget.set_item_height(item_height as f32);

        // 20 items, 5 visible (80px visible area)
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0);

        // Scroll is at 0, so first_visible_item = 0
        assert_eq!(widget.first_visible_item(), 0);

        // The list starts at y = list_origin_y (e.g., 100.0)
        // Item 0 is rendered from y=100 to y=116
        // Item 0's centre is at y = 100 + 8 = 108
        let list_origin_y = 100.0_f64;
        let item_0_centre = list_origin_y + item_height / 2.0;

        // Click on item 0's centre using flipped coordinates (which handle_mouse expects)
        let outcome = widget.handle_mouse(
            (50.0, item_0_centre),
            MouseEventKind::Down,
            item_height,
            list_origin_y,
        );

        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(
            widget.selected_index(),
            0,
            "Click near top of list (flipped y={}) should select item 0, not {}",
            item_0_centre,
            widget.selected_index()
        );

        // Verify that clicking near the top edge (just inside item 0) also selects item 0
        let near_top = list_origin_y + 1.0;
        widget.handle_mouse((50.0, near_top), MouseEventKind::Down, item_height, list_origin_y);
        assert_eq!(
            widget.selected_index(),
            0,
            "Click just below list_origin_y should select item 0"
        );
    }

    /// Step 3: Regression test for the scroll-rounding bug.
    ///
    /// Before `selector_row_scroller`, each scroll delta was rounded to the nearest
    /// integer row, so a sequence of sub-row deltas (e.g., 0.4 * item_height each)
    /// would produce zero net scroll.
    ///
    /// This test verifies that fractional scroll deltas accumulate correctly:
    /// 10 deltas of `0.4 * item_height` should produce `4.0 * item_height` total
    /// (exactly 4 rows).
    ///
    /// Chunk: docs/chunks/selector_hittest_tests
    #[test]
    fn scroll_rounding_regression_sub_row_deltas_accumulate() {
        let mut widget = SelectorWidget::new();
        // Default item_height from FontMetrics is 16.0
        let item_height = 16.0_f32;
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        assert_eq!(widget.first_visible_item(), 0);
        assert!((widget.scroll_fraction_px() - 0.0).abs() < 0.001);

        // Apply 10 scroll deltas of 0.4 * item_height = 6.4 pixels each
        let delta = 0.4 * item_height as f64;
        for _ in 0..10 {
            widget.handle_scroll(delta);
        }

        // Total scroll should be 10 * 6.4 = 64.0 pixels = 4 rows exactly
        let expected_offset = 64.0_f32;
        let actual_offset = widget.first_visible_item() as f32 * item_height
            + widget.scroll_fraction_px();

        assert!(
            (actual_offset - expected_offset).abs() < 0.001,
            "Expected scroll_offset_px = {}, got first_visible={} + frac={}",
            expected_offset,
            widget.first_visible_item(),
            widget.scroll_fraction_px()
        );

        // Verify derived values
        assert_eq!(
            widget.first_visible_item(),
            4,
            "Expected first_visible_item = 4 (64px / 16px)"
        );
        assert!(
            widget.scroll_fraction_px().abs() < 0.001,
            "Expected scroll_fraction_px = 0.0 (64.0 mod 16.0), got {}",
            widget.scroll_fraction_px()
        );
    }

    // =========================================================================
    // Step 4: Boundary condition tests
    // =========================================================================

    /// Clicking exactly on a row boundary (top pixel of a row) selects that row,
    /// not the previous one.
    ///
    /// Chunk: docs/chunks/selector_hittest_tests
    #[test]
    fn click_on_row_boundary_top_pixel_selects_that_row() {
        let mut widget = SelectorWidget::new();
        let item_height = 16.0_f64;
        widget.set_item_height(item_height as f32);
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Scroll to a fractional position (8.5 pixels)
        widget.handle_scroll(8.5);
        let first_visible = widget.first_visible_item();
        let scroll_frac = widget.scroll_fraction_px() as f64;
        let list_origin_y = 100.0_f64;

        // Test clicking the exact top pixel of row 2 (visible row index 2)
        let row = 2_usize;
        // Row starts at: list_origin_y - scroll_fraction_px + row * item_height
        let row_top_y = list_origin_y - scroll_frac + row as f64 * item_height;

        widget.handle_mouse((50.0, row_top_y), MouseEventKind::Down, item_height, list_origin_y);

        let expected_index = first_visible + row;
        assert_eq!(
            widget.selected_index(),
            expected_index,
            "Click on exact top pixel of row {} should select item {}, got {}",
            row,
            expected_index,
            widget.selected_index()
        );
    }

    /// When scroll_fraction_px == 0 (whole-row alignment), clicking works correctly.
    ///
    /// Chunk: docs/chunks/selector_hittest_tests
    #[test]
    fn click_when_scroll_fraction_is_zero() {
        let mut widget = SelectorWidget::new();
        let item_height = 16.0_f64;
        widget.set_item_height(item_height as f32);
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Scroll to an exact multiple of item_height (32.0 = 2 rows)
        widget.handle_scroll(32.0);
        assert_eq!(widget.first_visible_item(), 2);
        assert!(
            widget.scroll_fraction_px().abs() < 0.001,
            "Expected scroll_fraction_px == 0, got {}",
            widget.scroll_fraction_px()
        );

        let list_origin_y = 100.0_f64;

        // Click the centre of visible row 0 (which is item 2)
        let y = list_origin_y + item_height / 2.0;
        widget.handle_mouse((50.0, y), MouseEventKind::Down, item_height, list_origin_y);

        assert_eq!(
            widget.selected_index(),
            2,
            "With scroll_fraction=0, clicking row 0 centre should select item 2"
        );
    }

    /// Clicking below the last rendered item is a no-op.
    ///
    /// Chunk: docs/chunks/selector_hittest_tests
    #[test]
    fn click_below_last_rendered_item_is_noop() {
        let mut widget = SelectorWidget::new();
        let item_height = 16.0_f64;
        widget.set_item_height(item_height as f32);
        // Only 10 items, 5 visible
        widget.set_items((0..10).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Navigate to item 7 first, which will auto-scroll to keep it visible
        for _ in 0..7 {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }
        assert_eq!(widget.selected_index(), 7);
        // After navigating to item 7, the viewport should have scrolled
        // so item 7 is visible (first_visible should be around 3 or more)

        let list_origin_y = 100.0_f64;

        // Click below the last item in the list (item 9)
        // With 10 items and we're showing items starting from first_visible,
        // we click at a y position that would map to a row index >= 10
        // The visible rows start at list_origin_y, so clicking far below
        // should be out of bounds
        let far_below_y = list_origin_y + 20.0 * item_height; // Way beyond item 9

        let outcome = widget.handle_mouse((50.0, far_below_y), MouseEventKind::Down, item_height, list_origin_y);

        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(
            widget.selected_index(),
            7,
            "Click below last item should be a no-op, selection should remain at 7"
        );
    }

    /// Clicking above list_origin_y is a no-op.
    ///
    /// Chunk: docs/chunks/selector_hittest_tests
    #[test]
    fn click_above_list_origin_is_noop() {
        let mut widget = SelectorWidget::new();
        let item_height = 16.0_f64;
        widget.set_item_height(item_height as f32);
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Pre-select item 3 to verify click doesn't change it
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(widget.selected_index(), 3);

        let list_origin_y = 100.0_f64;

        // Click above the list origin (y=50, which is above list_origin_y=100)
        let outcome = widget.handle_mouse((50.0, 50.0), MouseEventKind::Down, item_height, list_origin_y);

        assert_eq!(outcome, SelectorOutcome::Pending);
        assert_eq!(
            widget.selected_index(),
            3,
            "Click above list_origin_y should be a no-op, selection should remain at 3"
        );
    }

    // =========================================================================
    // Chunk: docs/chunks/selector_scroll_end - Row height mismatch bug fix tests
    // =========================================================================

    /// Step 1 (PLAN.md): Test that demonstrates the row_height mismatch bug.
    ///
    /// When the external item_height (from geometry) differs from the RowScroller's
    /// internal row_height, the selection can end up outside the rendered viewport.
    ///
    /// The bug manifests as: the scroller thinks more items are visible than the
    /// renderer actually draws, so the selection highlight can be outside the
    /// scissor-clipped area.
    ///
    /// This test sets up a SelectorWidget with:
    /// - 50 items (more than fit in viewport)
    /// - An external row_height of 20.0 (differs from default 16.0)
    /// - A viewport that can show 18 items at 20px each (360px)
    ///
    /// Without set_item_height, the RowScroller uses row_height=16:
    /// - visible_rows = floor(360 / 16) = 22 (WRONG - renderer only draws 18)
    ///
    /// With set_item_height(20.0):
    /// - visible_rows = floor(360 / 20) = 18 (CORRECT)
    ///
    /// The draw_idx (selection position relative to first_visible) must be
    /// less than the RENDERER's visible item count (18), not the scroller's.
    ///
    /// Chunk: docs/chunks/selector_scroll_end
    #[test]
    fn row_height_mismatch_causes_incorrect_visible_rows() {
        let mut widget = SelectorWidget::new();
        // RowScroller is initialized with row_height=16.0 (from default FontMetrics)

        // Use an external item_height that differs from the default
        let external_item_height = 20.0_f32;
        let renderer_visible_items = 18_usize; // What the renderer actually draws
        let total_items = 50_usize;

        // Set up items
        widget.set_items((0..total_items).map(|i| format!("item{}", i)).collect());

        // Simulate what happens WITHOUT calling set_item_height:
        // The geometry uses external_item_height (20.0), but RowScroller uses 16.0
        let height_px = renderer_visible_items as f32 * external_item_height;
        widget.update_visible_size(height_px);

        // Navigate to the last item (index 49)
        for _ in 0..49 {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }

        // Verify we reached the last item
        assert_eq!(widget.selected_index(), 49);

        // BUG: Without the fix, visible_rows = 22 (wrong), so ensure_visible
        // places the selection at draw_idx = 21 (position 21 in the viewport).
        // But the renderer only draws 18 items (positions 0-17), so the
        // selection is OUTSIDE the rendered area!
        //
        // Check the draw_idx: selection position relative to first_visible_item
        let first_visible = widget.first_visible_item();
        let draw_idx = widget.selected_index() - first_visible;

        // This assertion documents the BUG: draw_idx >= renderer_visible_items
        // The selection is outside the rendered viewport without the fix.
        // (When the fix is applied, this test will fail and should be updated.)
        if draw_idx >= renderer_visible_items {
            // Bug confirmed: selection is outside rendered area
            // This is expected WITHOUT the fix
        } else {
            // If draw_idx < renderer_visible_items, the bug might not manifest
            // in this specific scenario, but the visible_rows calculation is
            // still wrong (22 instead of 18).
        }

        // The key assertion: visible_rows should match what the renderer draws
        // Without fix: visible_rows = 22 (from 360/16)
        // With fix: visible_rows = 18 (from 360/20)
        let range = widget.visible_item_range();
        let scroller_visible_rows = range.end - range.start;
        // The scroller thinks it can show more items than the renderer draws
        // This is incorrect and causes the selection to be invisible
        assert!(
            scroller_visible_rows > renderer_visible_items,
            "Bug: scroller_visible_rows ({}) should be > renderer_visible_items ({}) without fix",
            scroller_visible_rows,
            renderer_visible_items
        );
    }

    /// Test that after calling set_item_height, visible_rows is correct.
    ///
    /// Chunk: docs/chunks/selector_scroll_end
    #[test]
    fn set_item_height_corrects_visible_rows() {
        let mut widget = SelectorWidget::new();

        // Use an external item_height that differs from the default
        let external_item_height = 20.0_f32;
        let renderer_visible_items = 18_usize;
        let total_items = 50_usize;

        // Set up items
        widget.set_items((0..total_items).map(|i| format!("item{}", i)).collect());

        // FIX: Call set_item_height BEFORE update_visible_size
        widget.set_item_height(external_item_height);

        // Now update_visible_size uses the correct row_height
        let height_px = renderer_visible_items as f32 * external_item_height;
        widget.update_visible_size(height_px);

        // Navigate to the last item (index 49)
        for _ in 0..49 {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }

        // Verify we reached the last item
        assert_eq!(widget.selected_index(), 49);

        // With the fix: RowScroller uses row_height=20.0
        // - visible_rows = floor(360 / 20) = 18 (CORRECT)
        // - draw_idx should be within [0, 17]

        // Check the draw_idx
        let first_visible = widget.first_visible_item();
        let draw_idx = widget.selected_index() - first_visible;

        // With the fix, draw_idx should be < renderer_visible_items
        assert!(
            draw_idx < renderer_visible_items,
            "Selection draw_idx {} should be < renderer_visible_items {} with fix",
            draw_idx,
            renderer_visible_items
        );

        // The last item must be in the visible range
        let range = widget.visible_item_range();
        assert!(
            range.contains(&49),
            "Last item (49) should be in visible_item_range {:?}",
            range
        );
    }

    // =========================================================================
    // Chunk: docs/chunks/selector_scroll_bottom - Bug A / Bug B scroll fix tests
    // =========================================================================

    /// Step 1 (PLAN.md): Test that visible_item_range returns a proper range
    /// when update_visible_size is called after set_items with appropriate geometry.
    ///
    /// This test demonstrates Bug A's root cause: without calling update_visible_size,
    /// visible_item_range() returns 0..1 because visible_rows is initialized to 0.
    ///
    /// Chunk: docs/chunks/selector_scroll_bottom
    #[test]
    fn visible_item_range_correct_after_update_visible_size() {
        let mut widget = SelectorWidget::new();
        // Set up 20 items
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());

        // Before update_visible_size, visible_rows is 0, so range is 0..1
        assert_eq!(
            widget.visible_item_range(),
            0..1,
            "Without update_visible_size, visible_item_range should be 0..1 (Bug A root cause)"
        );

        // Now call update_visible_size with 80px height (row_height=16, so 5 visible rows)
        widget.update_visible_size(80.0);

        // After update_visible_size, visible_item_range should be 0..6 (5 visible + 1 for partial)
        assert_eq!(
            widget.visible_item_range(),
            0..6,
            "After update_visible_size(80.0), visible_item_range should be 0..6"
        );
    }

    /// Step 2 (PLAN.md): Test that navigating to the last item of a list larger than
    /// visible_rows leaves the selection at draw_idx == visible_rows - 1.
    ///
    /// This verifies ensure_visible keeps the selection within the scissor-clipped area.
    ///
    /// Chunk: docs/chunks/selector_scroll_bottom
    #[test]
    fn navigate_to_last_item_keeps_selection_at_bottom_of_viewport() {
        let mut widget = SelectorWidget::new();
        // 20 items, row_height=16, 5 visible rows (80px)
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // 5 visible rows

        // Navigate to the last item (index 19)
        for _ in 0..19 {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }

        assert_eq!(widget.selected_index(), 19);

        // The selection should be at draw_idx = visible_rows - 1 = 4
        // draw_idx = selected_index - first_visible_item()
        let first_visible = widget.first_visible_item();
        let draw_idx = widget.selected_index() - first_visible;

        // With 20 items and 5 visible rows, when selection is at 19:
        // first_visible should be 15 (so items 15-19 are visible)
        // draw_idx = 19 - 15 = 4 = visible_rows - 1
        assert_eq!(
            first_visible, 15,
            "first_visible_item should be 15 when selection is at last item"
        );
        assert_eq!(
            draw_idx, 4,
            "Selection at last item should have draw_idx = visible_rows - 1 = 4"
        );

        // Verify the selection is within visible_item_range
        let range = widget.visible_item_range();
        assert!(
            range.contains(&widget.selected_index()),
            "Selection {} should be within visible_item_range {:?}",
            widget.selected_index(),
            range
        );
    }

    /// Additional test: verify that navigating down one item at a time
    /// keeps the selection within the visible window at every step.
    ///
    /// Chunk: docs/chunks/selector_scroll_bottom
    #[test]
    fn navigate_down_keeps_selection_visible_at_every_step() {
        let mut widget = SelectorWidget::new();
        // 20 items, 5 visible rows
        widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
        widget.update_visible_size(80.0); // row_height=16, 5 visible rows

        // Navigate down one item at a time, checking visibility at each step
        for i in 0..19 {
            let range = widget.visible_item_range();
            assert!(
                range.contains(&widget.selected_index()),
                "At step {}, selection {} should be in visible_item_range {:?}",
                i,
                widget.selected_index(),
                range
            );

            // Also verify draw_idx is within [0, visible_rows - 1]
            let first_visible = widget.first_visible_item();
            let draw_idx = widget.selected_index().saturating_sub(first_visible);
            assert!(
                draw_idx <= 4, // visible_rows - 1 = 4
                "At step {}, draw_idx {} should be <= visible_rows - 1 (4)",
                i,
                draw_idx
            );

            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }

        // Final check at the last item
        let range = widget.visible_item_range();
        assert!(
            range.contains(&widget.selected_index()),
            "At last item, selection {} should be in visible_item_range {:?}",
            widget.selected_index(),
            range
        );
    }

    // =========================================================================
    // Chunk: docs/chunks/selector_scroll_end - Regression tests for scroll-to-bottom fix
    // =========================================================================

    /// Step 4 (PLAN.md): Comprehensive regression test for end-of-list scrolling.
    ///
    /// Creates a SelectorWidget with 2× the panel capacity items, navigates to the
    /// last item via repeated Down key presses, and verifies:
    /// - `selected_index()` == last item index
    /// - `visible_item_range()` contains the last item
    /// - The selection's draw_idx is within `[0, visible_rows - 1]`
    ///
    /// Chunk: docs/chunks/selector_scroll_end
    #[test]
    fn regression_scroll_to_bottom_via_arrow_keys() {
        let mut widget = SelectorWidget::new();

        // External item_height (simulating font_metrics.line_height)
        let item_height = 18.0_f32;
        let visible_rows = 10_usize;
        let total_items = 2 * visible_rows; // 2× panel capacity

        // Set up items
        widget.set_items((0..total_items).map(|i| format!("item{}", i)).collect());

        // CRITICAL: Set item_height before update_visible_size (this is the fix!)
        widget.set_item_height(item_height);
        widget.update_visible_size(visible_rows as f32 * item_height);

        // Navigate to the last item
        for _ in 0..(total_items - 1) {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }

        // Verify selected_index is at the last item
        assert_eq!(
            widget.selected_index(),
            total_items - 1,
            "Selection should be at last item ({})",
            total_items - 1
        );

        // Verify last item is in visible_item_range
        let range = widget.visible_item_range();
        assert!(
            range.contains(&(total_items - 1)),
            "Last item ({}) should be in visible_item_range {:?}",
            total_items - 1,
            range
        );

        // Verify draw_idx is within visible viewport
        let first_visible = widget.first_visible_item();
        let draw_idx = widget.selected_index() - first_visible;
        assert!(
            draw_idx < visible_rows,
            "Selection draw_idx {} should be < visible_rows {}",
            draw_idx,
            visible_rows
        );
    }

    /// Step 5 (PLAN.md): Test mouse scroll to bottom.
    ///
    /// Creates a SelectorWidget with many items, sets correct item_height and
    /// visible size, applies enough scroll delta to reach the maximum scroll
    /// offset, and verifies the last item is within visible_item_range().
    ///
    /// Chunk: docs/chunks/selector_scroll_end
    #[test]
    fn regression_scroll_to_bottom_via_mouse_wheel() {
        let mut widget = SelectorWidget::new();

        // External item_height
        let item_height = 18.0_f32;
        let visible_rows = 10_usize;
        let total_items = 30_usize;

        // Set up items
        widget.set_items((0..total_items).map(|i| format!("item{}", i)).collect());

        // CRITICAL: Set item_height before update_visible_size
        widget.set_item_height(item_height);
        widget.update_visible_size(visible_rows as f32 * item_height);

        // Calculate the scroll delta needed to reach the bottom
        // max_scroll_offset = (total_items - visible_rows) * item_height
        let max_scroll = (total_items - visible_rows) as f64 * item_height as f64;

        // Scroll to the bottom (plus a bit more to test clamping)
        widget.handle_scroll(max_scroll + 100.0);

        // Verify the last item is in visible_item_range
        let range = widget.visible_item_range();
        assert!(
            range.contains(&(total_items - 1)),
            "Last item ({}) should be in visible_item_range {:?} after scroll",
            total_items - 1,
            range
        );

        // Verify first_visible is at the expected position
        let expected_first_visible = total_items - visible_rows;
        assert_eq!(
            widget.first_visible_item(),
            expected_first_visible,
            "first_visible_item should be {} when scrolled to bottom",
            expected_first_visible
        );
    }

    /// Regression test: ensure_visible places the last item at the bottom of the viewport.
    ///
    /// When navigating to the last item, the scroll should position it at
    /// draw_idx = visible_rows - 1, not outside the viewport.
    ///
    /// Chunk: docs/chunks/selector_scroll_end
    #[test]
    fn regression_ensure_visible_last_item_at_bottom() {
        let mut widget = SelectorWidget::new();

        let item_height = 20.0_f32;
        let visible_rows = 5_usize;
        let total_items = 20_usize;

        widget.set_items((0..total_items).map(|i| format!("item{}", i)).collect());
        widget.set_item_height(item_height);
        widget.update_visible_size(visible_rows as f32 * item_height);

        // Jump directly to the last item via repeated Down presses
        for _ in 0..(total_items - 1) {
            widget.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        }

        assert_eq!(widget.selected_index(), total_items - 1);

        // The last item should be at the bottom of the viewport
        // first_visible should be total_items - visible_rows = 15
        // draw_idx = 19 - 15 = 4 = visible_rows - 1 ✓
        let first_visible = widget.first_visible_item();
        let expected_first_visible = total_items - visible_rows;
        assert_eq!(
            first_visible, expected_first_visible,
            "first_visible {} should be {} to show last item at bottom",
            first_visible, expected_first_visible
        );

        let draw_idx = widget.selected_index() - first_visible;
        assert_eq!(
            draw_idx,
            visible_rows - 1,
            "Last item should be at draw_idx {} (bottom of viewport)",
            visible_rows - 1
        );
    }
}
