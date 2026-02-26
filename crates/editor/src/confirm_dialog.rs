// Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog widget
// Chunk: docs/chunks/generic_yes_no_modal - Generalized confirm dialog context
//!
//! A confirmation dialog widget for binary yes/no decisions.
//!
//! This module provides [`ConfirmDialog`], a pure state struct following the
//! project's Humble View Architecture. It manages a prompt message and button
//! selection for binary confirmation dialogs (e.g., "Abandon unsaved changes?").
//!
//! The widget handles keyboard navigation:
//! - **Tab/Left/Right**: Toggle between Cancel and Abandon buttons
//! - **Enter**: Confirm the selected button
//! - **Escape**: Always cancels (shortcut for Cancel button)
//!
//! # Design
//!
//! Following the project's Humble View Architecture, `ConfirmDialog` is pure
//! interaction state with no platform dependencies. Downstream code (renderers,
//! focus targets) consume this state and translate it to pixels or editor mutations.
//!
//! The [`ConfirmDialogContext`] enum captures what triggered the dialog and what
//! action should occur on confirmation. This allows the same dialog infrastructure
//! to support multiple use cases: close-dirty-tab, quit-with-dirty-tabs, etc.
//!
//! # Example
//!
//! ```ignore
//! use crate::confirm_dialog::{ConfirmDialog, ConfirmOutcome, ConfirmButton};
//! use crate::input::{KeyEvent, Key};
//!
//! let mut dialog = ConfirmDialog::new("Abandon unsaved changes?");
//! assert_eq!(dialog.selected, ConfirmButton::Cancel); // Default
//!
//! // User presses Tab to select Abandon
//! let outcome = dialog.handle_key(&KeyEvent::new(Key::Tab, Default::default()));
//! assert_eq!(outcome, ConfirmOutcome::Pending);
//! assert_eq!(dialog.selected, ConfirmButton::Abandon);
//!
//! // User presses Enter to confirm
//! let outcome = dialog.handle_key(&KeyEvent::new(Key::Return, Default::default()));
//! assert_eq!(outcome, ConfirmOutcome::Confirmed);
//! ```

use std::path::PathBuf;

use crate::input::{Key, KeyEvent};
use crate::pane_layout::PaneId;

// =============================================================================
// Context enum for what triggered the confirm dialog
// =============================================================================

/// Context for what triggered the confirm dialog and what action to take on confirmation.
///
/// This enum replaces the tightly-coupled `pending_close: Option<(PaneId, usize)>` field
/// in EditorState, allowing the same dialog infrastructure to support multiple use cases.
/// Each variant contains the data needed for its specific outcome handler.
// Chunk: docs/chunks/generic_yes_no_modal - ConfirmDialogContext enum
#[derive(Debug, Clone)]
pub enum ConfirmDialogContext {
    /// Closing a tab with unsaved changes.
    CloseDirtyTab {
        /// The pane containing the dirty tab.
        pane_id: PaneId,
        /// The index of the tab within the pane.
        tab_idx: usize,
    },
    /// Quitting the application with dirty tabs.
    QuitWithDirtyTabs {
        /// Number of dirty tabs (for display in prompt).
        dirty_count: usize,
    },
    /// Closing a terminal tab with a running process.
    // Chunk: docs/chunks/terminal_close_guard - Terminal process guard context
    CloseActiveTerminal {
        /// The pane containing the terminal tab.
        pane_id: PaneId,
        /// The index of the tab within the pane.
        tab_idx: usize,
    },
    // Chunk: docs/chunks/deletion_rename_handling - File deleted confirmation context
    /// File was deleted from disk while buffer was open.
    ///
    /// The dialog offers "Save" (recreate the file from buffer contents) or
    /// "Abandon" (close the tab). "Save" is the confirm action, "Abandon" is cancel.
    FileDeletedFromDisk {
        /// The pane containing the affected tab.
        pane_id: PaneId,
        /// The index of the tab within the pane.
        tab_idx: usize,
        /// The path that was deleted (for recreating the file).
        deleted_path: PathBuf,
    },
}

/// Which button is currently selected in the confirm dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfirmButton {
    /// The Cancel button (safe default)
    #[default]
    Cancel,
    /// The Abandon button (destructive action)
    Abandon,
}

impl ConfirmButton {
    /// Toggles between Cancel and Abandon.
    pub fn toggle(self) -> Self {
        match self {
            ConfirmButton::Cancel => ConfirmButton::Abandon,
            ConfirmButton::Abandon => ConfirmButton::Cancel,
        }
    }
}

/// Outcome of handling a key event in the confirm dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// User pressed Escape, or Enter with Cancel selected
    Cancelled,
    /// User pressed Enter with Abandon selected
    Confirmed,
    /// Dialog is still open, waiting for user input
    Pending,
}

/// Default label for the cancel button
pub const DEFAULT_CANCEL_LABEL: &str = "Cancel";
/// Default label for the confirm button
pub const DEFAULT_CONFIRM_LABEL: &str = "Abandon";

/// A confirmation dialog widget for binary yes/no decisions.
///
/// This struct holds the pure state for a modal confirmation dialog.
/// Following the Humble View Architecture, it has no platform dependencies
/// and can be unit tested without Metal or macOS.
// Chunk: docs/chunks/generic_yes_no_modal - Parameterized button labels
#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    /// The prompt message (e.g., "Abandon unsaved changes?")
    pub prompt: String,
    /// Currently selected button (default: Cancel)
    pub selected: ConfirmButton,
    /// Label for the cancel (left) button
    pub cancel_label: String,
    /// Label for the confirm (right) button
    pub confirm_label: String,
}

impl ConfirmDialog {
    /// Creates a new confirmation dialog with the given prompt.
    ///
    /// Uses default button labels ("Cancel" / "Abandon").
    /// The Cancel button is selected by default (safe default).
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            selected: ConfirmButton::Cancel,
            cancel_label: DEFAULT_CANCEL_LABEL.to_string(),
            confirm_label: DEFAULT_CONFIRM_LABEL.to_string(),
        }
    }

    /// Creates a new confirmation dialog with custom button labels.
    ///
    /// The Cancel button is selected by default (safe default).
    // Chunk: docs/chunks/generic_yes_no_modal - Custom label constructor
    pub fn with_labels(
        prompt: impl Into<String>,
        cancel_label: impl Into<String>,
        confirm_label: impl Into<String>,
    ) -> Self {
        Self {
            prompt: prompt.into(),
            selected: ConfirmButton::Cancel,
            cancel_label: cancel_label.into(),
            confirm_label: confirm_label.into(),
        }
    }

    /// Handles a keyboard event and returns the appropriate outcome.
    ///
    /// # Behavior
    ///
    /// - **Tab**: Toggles between Cancel and Abandon, returns `Pending`
    /// - **Left arrow**: Selects Cancel, returns `Pending`
    /// - **Right arrow**: Selects Abandon, returns `Pending`
    /// - **Return/Enter**: Returns `Cancelled` if Cancel is selected,
    ///   `Confirmed` if Abandon is selected
    /// - **Escape**: Returns `Cancelled` (shortcut for Cancel)
    /// - **All other keys**: Returns `Pending` (no-op)
    pub fn handle_key(&mut self, event: &KeyEvent) -> ConfirmOutcome {
        match &event.key {
            Key::Tab => {
                self.selected = self.selected.toggle();
                ConfirmOutcome::Pending
            }
            Key::Left => {
                self.selected = ConfirmButton::Cancel;
                ConfirmOutcome::Pending
            }
            Key::Right => {
                self.selected = ConfirmButton::Abandon;
                ConfirmOutcome::Pending
            }
            Key::Return => match self.selected {
                ConfirmButton::Cancel => ConfirmOutcome::Cancelled,
                ConfirmButton::Abandon => ConfirmOutcome::Confirmed,
            },
            Key::Escape => ConfirmOutcome::Cancelled,
            _ => ConfirmOutcome::Pending,
        }
    }
}

// =============================================================================
// Geometry calculation for the confirm dialog overlay
// =============================================================================

/// Computed geometry for the confirm dialog overlay.
///
/// All measurements are in pixels. The dialog is centered both horizontally
/// and vertically (or ~40% from top for better visual balance).
// Chunk: docs/chunks/generic_yes_no_modal - Added hit testing methods
#[derive(Debug, Clone, Copy)]
pub struct ConfirmDialogGeometry {
    /// X coordinate of the panel's left edge
    pub panel_x: f32,
    /// Y coordinate of the panel's top edge
    pub panel_y: f32,
    /// Width of the dialog panel
    pub panel_width: f32,
    /// Height of the dialog panel
    pub panel_height: f32,
    /// X coordinate where the prompt text starts
    pub prompt_x: f32,
    /// Y coordinate where the prompt text baseline is
    pub prompt_y: f32,
    /// X coordinate of the Cancel button's left edge
    pub cancel_button_x: f32,
    /// X coordinate of the Abandon button's left edge
    pub abandon_button_x: f32,
    /// Y coordinate of the button row
    pub buttons_y: f32,
    /// Width of each button
    pub button_width: f32,
    /// Height of each button
    pub button_height: f32,
}

impl ConfirmDialogGeometry {
    /// Returns true if (x, y) is inside the cancel button.
    // Chunk: docs/chunks/generic_yes_no_modal - Hit testing for mouse clicks
    pub fn is_cancel_button(&self, x: f32, y: f32) -> bool {
        x >= self.cancel_button_x
            && x < self.cancel_button_x + self.button_width
            && y >= self.buttons_y
            && y < self.buttons_y + self.button_height
    }

    /// Returns true if (x, y) is inside the confirm button.
    // Chunk: docs/chunks/generic_yes_no_modal - Hit testing for mouse clicks
    pub fn is_confirm_button(&self, x: f32, y: f32) -> bool {
        x >= self.abandon_button_x
            && x < self.abandon_button_x + self.button_width
            && y >= self.buttons_y
            && y < self.buttons_y + self.button_height
    }
}

// Padding and sizing constants
const DIALOG_PADDING: f32 = 16.0;
const BUTTON_PADDING: f32 = 8.0;
const BUTTON_GAP: f32 = 16.0;

/// Calculates geometry for the confirm dialog overlay.
///
/// The dialog is:
/// - Horizontally centered
/// - Vertically positioned at ~40% from the top (for visual balance)
/// - Wide enough for the prompt and two buttons side by side
/// - Two rows tall: prompt row + buttons row (plus padding)
///
/// # Arguments
///
/// - `view_width`: The width of the view in pixels
/// - `view_height`: The height of the view in pixels
/// - `line_height`: The height of a text line in pixels
/// - `glyph_width`: The width of a single glyph in pixels (monospace assumed)
/// - `dialog`: The confirm dialog state (used for prompt and button label widths)
// Chunk: docs/chunks/generic_yes_no_modal - Accept dialog reference for dynamic labels
pub fn calculate_confirm_dialog_geometry(
    view_width: f32,
    view_height: f32,
    line_height: f32,
    glyph_width: f32,
    dialog: &ConfirmDialog,
) -> ConfirmDialogGeometry {
    // Calculate button dimensions using the dialog's actual labels
    let cancel_label_width = dialog.cancel_label.len() as f32 * glyph_width;
    let confirm_label_width = dialog.confirm_label.len() as f32 * glyph_width;
    let button_width = cancel_label_width.max(confirm_label_width) + 2.0 * BUTTON_PADDING;
    let button_height = line_height + BUTTON_PADDING;

    // Calculate total buttons width (two buttons + gap)
    let buttons_total_width = 2.0 * button_width + BUTTON_GAP;

    // Calculate prompt width using the actual prompt
    let prompt_width = dialog.prompt.len() as f32 * glyph_width;

    // Panel width is the larger of buttons row or prompt, plus padding
    let content_width = buttons_total_width.max(prompt_width);
    let panel_width = content_width + 2.0 * DIALOG_PADDING;

    // Panel height: padding + prompt line + gap + button row + padding
    let panel_height = DIALOG_PADDING + line_height + DIALOG_PADDING + button_height + DIALOG_PADDING;

    // Center horizontally
    let panel_x = (view_width - panel_width) / 2.0;

    // Position at ~40% from top for visual balance (feels more natural than exact center)
    let panel_y = view_height * 0.4 - panel_height / 2.0;

    // Prompt position (centered within panel, baseline at first row)
    let prompt_x = panel_x + DIALOG_PADDING;
    let prompt_y = panel_y + DIALOG_PADDING + line_height;

    // Button positions (centered within panel)
    let buttons_start_x = panel_x + (panel_width - buttons_total_width) / 2.0;
    let cancel_button_x = buttons_start_x;
    let abandon_button_x = buttons_start_x + button_width + BUTTON_GAP;
    let buttons_y = panel_y + DIALOG_PADDING + line_height + DIALOG_PADDING;

    ConfirmDialogGeometry {
        panel_x,
        panel_y,
        panel_width,
        panel_height,
        prompt_x,
        prompt_y,
        cancel_button_x,
        abandon_button_x,
        buttons_y,
        button_width,
        button_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Modifiers;

    // =========================================================================
    // ConfirmDialogContext tests
    // Chunk: docs/chunks/generic_yes_no_modal - Tests for context enum
    // =========================================================================

    #[test]
    fn test_context_close_dirty_tab_stores_pane_and_index() {
        let ctx = ConfirmDialogContext::CloseDirtyTab {
            pane_id: 42,
            tab_idx: 3,
        };

        // Verify we can pattern match and extract the values
        match ctx {
            ConfirmDialogContext::CloseDirtyTab { pane_id, tab_idx } => {
                assert_eq!(pane_id, 42);
                assert_eq!(tab_idx, 3);
            }
            _ => panic!("Expected CloseDirtyTab variant"),
        }
    }

    #[test]
    fn test_context_quit_with_dirty_tabs_stores_count() {
        let ctx = ConfirmDialogContext::QuitWithDirtyTabs { dirty_count: 5 };

        // Verify we can pattern match and extract the value
        match ctx {
            ConfirmDialogContext::QuitWithDirtyTabs { dirty_count } => {
                assert_eq!(dirty_count, 5);
            }
            _ => panic!("Expected QuitWithDirtyTabs variant"),
        }
    }

    #[test]
    fn test_context_is_clone() {
        let ctx = ConfirmDialogContext::CloseDirtyTab {
            pane_id: 1,
            tab_idx: 0,
        };
        let cloned = ctx.clone();

        match (ctx, cloned) {
            (
                ConfirmDialogContext::CloseDirtyTab { pane_id: a, tab_idx: b },
                ConfirmDialogContext::CloseDirtyTab { pane_id: c, tab_idx: d },
            ) => {
                assert_eq!(a, c);
                assert_eq!(b, d);
            }
            _ => panic!("Clone should produce same variant"),
        }
    }

    // =========================================================================
    // CloseActiveTerminal context tests
    // Chunk: docs/chunks/terminal_close_guard - Tests for CloseActiveTerminal variant
    // =========================================================================

    #[test]
    fn test_context_close_active_terminal_stores_pane_and_index() {
        let ctx = ConfirmDialogContext::CloseActiveTerminal {
            pane_id: 99,
            tab_idx: 7,
        };

        // Verify we can pattern match and extract the values
        match ctx {
            ConfirmDialogContext::CloseActiveTerminal { pane_id, tab_idx } => {
                assert_eq!(pane_id, 99);
                assert_eq!(tab_idx, 7);
            }
            _ => panic!("Expected CloseActiveTerminal variant"),
        }
    }

    #[test]
    fn test_context_close_active_terminal_is_clone() {
        let ctx = ConfirmDialogContext::CloseActiveTerminal {
            pane_id: 5,
            tab_idx: 2,
        };
        let cloned = ctx.clone();

        match (ctx, cloned) {
            (
                ConfirmDialogContext::CloseActiveTerminal { pane_id: a, tab_idx: b },
                ConfirmDialogContext::CloseActiveTerminal { pane_id: c, tab_idx: d },
            ) => {
                assert_eq!(a, c);
                assert_eq!(b, d);
            }
            _ => panic!("Clone should produce same variant"),
        }
    }

    // =========================================================================
    // FileDeletedFromDisk context tests
    // Chunk: docs/chunks/deletion_rename_handling - Tests for FileDeletedFromDisk variant
    // =========================================================================

    #[test]
    fn test_context_file_deleted_stores_pane_tab_and_path() {
        let ctx = ConfirmDialogContext::FileDeletedFromDisk {
            pane_id: 42,
            tab_idx: 3,
            deleted_path: PathBuf::from("/path/to/deleted.txt"),
        };

        match ctx {
            ConfirmDialogContext::FileDeletedFromDisk { pane_id, tab_idx, deleted_path } => {
                assert_eq!(pane_id, 42);
                assert_eq!(tab_idx, 3);
                assert_eq!(deleted_path, PathBuf::from("/path/to/deleted.txt"));
            }
            _ => panic!("Expected FileDeletedFromDisk variant"),
        }
    }

    #[test]
    fn test_context_file_deleted_is_clone() {
        let ctx = ConfirmDialogContext::FileDeletedFromDisk {
            pane_id: 1,
            tab_idx: 0,
            deleted_path: PathBuf::from("/path"),
        };
        let cloned = ctx.clone();

        match (ctx, cloned) {
            (
                ConfirmDialogContext::FileDeletedFromDisk { pane_id: a, tab_idx: b, deleted_path: c },
                ConfirmDialogContext::FileDeletedFromDisk { pane_id: d, tab_idx: e, deleted_path: f },
            ) => {
                assert_eq!(a, d);
                assert_eq!(b, e);
                assert_eq!(c, f);
            }
            _ => panic!("Clone should produce same variant"),
        }
    }

    // =========================================================================
    // Button label parameterization tests
    // Chunk: docs/chunks/generic_yes_no_modal - Tests for parameterized labels
    // =========================================================================

    #[test]
    fn test_new_uses_default_labels() {
        let dialog = ConfirmDialog::new("Test prompt");
        assert_eq!(dialog.cancel_label, DEFAULT_CANCEL_LABEL);
        assert_eq!(dialog.confirm_label, DEFAULT_CONFIRM_LABEL);
    }

    #[test]
    fn test_with_labels_uses_custom_labels() {
        let dialog = ConfirmDialog::with_labels("Reload file?", "Keep Edits", "Reload");
        assert_eq!(dialog.prompt, "Reload file?");
        assert_eq!(dialog.cancel_label, "Keep Edits");
        assert_eq!(dialog.confirm_label, "Reload");
        assert_eq!(dialog.selected, ConfirmButton::Cancel); // Default selection
    }

    // =========================================================================
    // Step 1: ConfirmDialog widget key handling tests
    // =========================================================================

    #[test]
    fn test_new_dialog_has_cancel_selected_by_default() {
        let dialog = ConfirmDialog::new("Test prompt");
        assert_eq!(dialog.selected, ConfirmButton::Cancel);
        assert_eq!(dialog.prompt, "Test prompt");
    }

    #[test]
    fn test_tab_toggles_selection_to_abandon() {
        let mut dialog = ConfirmDialog::new("Test");
        assert_eq!(dialog.selected, ConfirmButton::Cancel);

        let outcome = dialog.handle_key(&KeyEvent::new(Key::Tab, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Pending);
        assert_eq!(dialog.selected, ConfirmButton::Abandon);
    }

    #[test]
    fn test_tab_toggles_selection_back_to_cancel() {
        let mut dialog = ConfirmDialog::new("Test");
        dialog.selected = ConfirmButton::Abandon;

        let outcome = dialog.handle_key(&KeyEvent::new(Key::Tab, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Pending);
        assert_eq!(dialog.selected, ConfirmButton::Cancel);
    }

    #[test]
    fn test_left_selects_cancel() {
        let mut dialog = ConfirmDialog::new("Test");
        dialog.selected = ConfirmButton::Abandon;

        let outcome = dialog.handle_key(&KeyEvent::new(Key::Left, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Pending);
        assert_eq!(dialog.selected, ConfirmButton::Cancel);
    }

    #[test]
    fn test_left_on_cancel_stays_at_cancel() {
        let mut dialog = ConfirmDialog::new("Test");
        assert_eq!(dialog.selected, ConfirmButton::Cancel);

        let outcome = dialog.handle_key(&KeyEvent::new(Key::Left, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Pending);
        assert_eq!(dialog.selected, ConfirmButton::Cancel);
    }

    #[test]
    fn test_right_selects_abandon() {
        let mut dialog = ConfirmDialog::new("Test");
        assert_eq!(dialog.selected, ConfirmButton::Cancel);

        let outcome = dialog.handle_key(&KeyEvent::new(Key::Right, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Pending);
        assert_eq!(dialog.selected, ConfirmButton::Abandon);
    }

    #[test]
    fn test_right_on_abandon_stays_at_abandon() {
        let mut dialog = ConfirmDialog::new("Test");
        dialog.selected = ConfirmButton::Abandon;

        let outcome = dialog.handle_key(&KeyEvent::new(Key::Right, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Pending);
        assert_eq!(dialog.selected, ConfirmButton::Abandon);
    }

    #[test]
    fn test_enter_on_cancel_returns_cancelled() {
        let mut dialog = ConfirmDialog::new("Test");
        assert_eq!(dialog.selected, ConfirmButton::Cancel);

        let outcome = dialog.handle_key(&KeyEvent::new(Key::Return, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Cancelled);
    }

    #[test]
    fn test_enter_on_abandon_returns_confirmed() {
        let mut dialog = ConfirmDialog::new("Test");
        dialog.selected = ConfirmButton::Abandon;

        let outcome = dialog.handle_key(&KeyEvent::new(Key::Return, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Confirmed);
    }

    #[test]
    fn test_escape_always_returns_cancelled() {
        let mut dialog = ConfirmDialog::new("Test");

        // Escape from Cancel
        let outcome = dialog.handle_key(&KeyEvent::new(Key::Escape, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Cancelled);

        // Escape from Abandon
        dialog.selected = ConfirmButton::Abandon;
        let outcome = dialog.handle_key(&KeyEvent::new(Key::Escape, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Cancelled);
    }

    #[test]
    fn test_unhandled_key_returns_pending() {
        let mut dialog = ConfirmDialog::new("Test");

        // Random character should be no-op
        let outcome = dialog.handle_key(&KeyEvent::char('a'));
        assert_eq!(outcome, ConfirmOutcome::Pending);
        assert_eq!(dialog.selected, ConfirmButton::Cancel); // Should not change

        // Down arrow should be no-op
        let outcome = dialog.handle_key(&KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Pending);

        // Up arrow should be no-op
        let outcome = dialog.handle_key(&KeyEvent::new(Key::Up, Modifiers::default()));
        assert_eq!(outcome, ConfirmOutcome::Pending);
    }

    // =========================================================================
    // Step 2: Geometry calculation tests
    // Chunk: docs/chunks/generic_yes_no_modal - Updated to pass dialog reference
    // =========================================================================

    /// Helper to create a default dialog for geometry tests
    fn default_test_dialog() -> ConfirmDialog {
        ConfirmDialog::new("Abandon unsaved changes?")
    }

    #[test]
    fn test_dialog_geometry_centered_horizontally() {
        let view_width = 800.0;
        let view_height = 600.0;
        let line_height = 16.0;
        let glyph_width = 8.0;
        let dialog = default_test_dialog();

        let geom = calculate_confirm_dialog_geometry(view_width, view_height, line_height, glyph_width, &dialog);

        // Panel should be centered horizontally
        let panel_center_x = geom.panel_x + geom.panel_width / 2.0;
        let view_center_x = view_width / 2.0;
        assert!(
            (panel_center_x - view_center_x).abs() < 0.001,
            "Panel center X ({}) should equal view center X ({})",
            panel_center_x,
            view_center_x
        );
    }

    #[test]
    fn test_dialog_geometry_vertically_positioned() {
        let view_width = 800.0;
        let view_height = 600.0;
        let line_height = 16.0;
        let glyph_width = 8.0;
        let dialog = default_test_dialog();

        let geom = calculate_confirm_dialog_geometry(view_width, view_height, line_height, glyph_width, &dialog);

        // Panel should be positioned at ~40% from top
        let panel_center_y = geom.panel_y + geom.panel_height / 2.0;
        let expected_center_y = view_height * 0.4;
        assert!(
            (panel_center_y - expected_center_y).abs() < 0.001,
            "Panel center Y ({}) should be at 40% of view height ({})",
            panel_center_y,
            expected_center_y
        );
    }

    #[test]
    fn test_dialog_geometry_has_correct_button_positions() {
        let view_width = 800.0;
        let view_height = 600.0;
        let line_height = 16.0;
        let glyph_width = 8.0;
        let dialog = default_test_dialog();

        let geom = calculate_confirm_dialog_geometry(view_width, view_height, line_height, glyph_width, &dialog);

        // Cancel button should be to the left of Abandon button
        assert!(
            geom.cancel_button_x < geom.abandon_button_x,
            "Cancel button X ({}) should be less than Abandon button X ({})",
            geom.cancel_button_x,
            geom.abandon_button_x
        );

        // Both buttons should be within the panel
        assert!(
            geom.cancel_button_x >= geom.panel_x,
            "Cancel button should be within panel (left edge)"
        );
        assert!(
            geom.abandon_button_x + geom.button_width <= geom.panel_x + geom.panel_width,
            "Abandon button should be within panel (right edge)"
        );

        // Buttons should be on the same row
        // (buttons_y is the same for both)
        assert!(
            geom.buttons_y > geom.panel_y,
            "Buttons Y should be below panel top"
        );
        assert!(
            geom.buttons_y + geom.button_height <= geom.panel_y + geom.panel_height,
            "Buttons should fit within panel height"
        );
    }

    #[test]
    fn test_dialog_geometry_with_small_viewport() {
        // Test with a small viewport to ensure geometry doesn't overflow
        let view_width = 200.0;
        let view_height = 150.0;
        let line_height = 16.0;
        let glyph_width = 8.0;
        let dialog = default_test_dialog();

        let geom = calculate_confirm_dialog_geometry(view_width, view_height, line_height, glyph_width, &dialog);

        // Panel should still be centered (even if it overflows)
        let panel_center_x = geom.panel_x + geom.panel_width / 2.0;
        let view_center_x = view_width / 2.0;
        assert!(
            (panel_center_x - view_center_x).abs() < 0.001,
            "Panel should be centered even in small viewport"
        );

        // Button width should be positive
        assert!(geom.button_width > 0.0, "Button width should be positive");
        assert!(geom.button_height > 0.0, "Button height should be positive");
    }

    #[test]
    fn test_dialog_geometry_scales_with_font_metrics() {
        let view_width = 800.0;
        let view_height = 600.0;
        let dialog = default_test_dialog();

        // Test with smaller font
        let geom_small = calculate_confirm_dialog_geometry(view_width, view_height, 12.0, 6.0, &dialog);

        // Test with larger font
        let geom_large = calculate_confirm_dialog_geometry(view_width, view_height, 20.0, 10.0, &dialog);

        // Larger font should produce larger geometry
        assert!(
            geom_large.panel_width > geom_small.panel_width,
            "Larger font should produce wider panel"
        );
        assert!(
            geom_large.panel_height > geom_small.panel_height,
            "Larger font should produce taller panel"
        );
        assert!(
            geom_large.button_width > geom_small.button_width,
            "Larger font should produce wider buttons"
        );
    }

    // =========================================================================
    // Hit testing tests
    // Chunk: docs/chunks/generic_yes_no_modal - Tests for button hit testing
    // =========================================================================

    #[test]
    fn test_is_cancel_button_inside() {
        let dialog = default_test_dialog();
        let geom = calculate_confirm_dialog_geometry(800.0, 600.0, 16.0, 8.0, &dialog);

        // Point in the center of the cancel button
        let center_x = geom.cancel_button_x + geom.button_width / 2.0;
        let center_y = geom.buttons_y + geom.button_height / 2.0;
        assert!(
            geom.is_cancel_button(center_x, center_y),
            "Center of cancel button should be inside"
        );

        // Point at the top-left corner (just inside)
        assert!(
            geom.is_cancel_button(geom.cancel_button_x, geom.buttons_y),
            "Top-left corner should be inside"
        );
    }

    #[test]
    fn test_is_cancel_button_outside() {
        let dialog = default_test_dialog();
        let geom = calculate_confirm_dialog_geometry(800.0, 600.0, 16.0, 8.0, &dialog);

        // Point to the left of the cancel button
        assert!(
            !geom.is_cancel_button(geom.cancel_button_x - 1.0, geom.buttons_y),
            "Point to the left should be outside"
        );

        // Point above the cancel button
        assert!(
            !geom.is_cancel_button(geom.cancel_button_x + geom.button_width / 2.0, geom.buttons_y - 1.0),
            "Point above should be outside"
        );

        // Point below the cancel button
        assert!(
            !geom.is_cancel_button(
                geom.cancel_button_x + geom.button_width / 2.0,
                geom.buttons_y + geom.button_height
            ),
            "Point at bottom edge should be outside"
        );
    }

    #[test]
    fn test_is_confirm_button_inside() {
        let dialog = default_test_dialog();
        let geom = calculate_confirm_dialog_geometry(800.0, 600.0, 16.0, 8.0, &dialog);

        // Point in the center of the confirm button
        let center_x = geom.abandon_button_x + geom.button_width / 2.0;
        let center_y = geom.buttons_y + geom.button_height / 2.0;
        assert!(
            geom.is_confirm_button(center_x, center_y),
            "Center of confirm button should be inside"
        );

        // Point at the top-left corner (just inside)
        assert!(
            geom.is_confirm_button(geom.abandon_button_x, geom.buttons_y),
            "Top-left corner should be inside"
        );
    }

    #[test]
    fn test_is_confirm_button_outside() {
        let dialog = default_test_dialog();
        let geom = calculate_confirm_dialog_geometry(800.0, 600.0, 16.0, 8.0, &dialog);

        // Point to the right of the confirm button
        assert!(
            !geom.is_confirm_button(geom.abandon_button_x + geom.button_width, geom.buttons_y),
            "Point at right edge should be outside"
        );

        // Point in the gap between buttons
        let gap_x = geom.cancel_button_x + geom.button_width + 1.0;
        assert!(
            !geom.is_confirm_button(gap_x, geom.buttons_y),
            "Point in gap should be outside confirm button"
        );
    }

    #[test]
    fn test_button_hit_areas_do_not_overlap() {
        let dialog = default_test_dialog();
        let geom = calculate_confirm_dialog_geometry(800.0, 600.0, 16.0, 8.0, &dialog);

        // Sample points across the button row
        let y = geom.buttons_y + geom.button_height / 2.0;

        // Point in cancel button should not be in confirm button
        let cancel_center_x = geom.cancel_button_x + geom.button_width / 2.0;
        assert!(geom.is_cancel_button(cancel_center_x, y));
        assert!(!geom.is_confirm_button(cancel_center_x, y));

        // Point in confirm button should not be in cancel button
        let confirm_center_x = geom.abandon_button_x + geom.button_width / 2.0;
        assert!(geom.is_confirm_button(confirm_center_x, y));
        assert!(!geom.is_cancel_button(confirm_center_x, y));

        // Point in gap should be in neither
        let gap_x = (geom.cancel_button_x + geom.button_width + geom.abandon_button_x) / 2.0;
        assert!(!geom.is_cancel_button(gap_x, y));
        assert!(!geom.is_confirm_button(gap_x, y));
    }

    #[test]
    fn test_geometry_with_custom_labels() {
        let dialog = ConfirmDialog::with_labels(
            "Delete file permanently?",
            "No",
            "Yes, Delete",
        );
        let geom = calculate_confirm_dialog_geometry(800.0, 600.0, 16.0, 8.0, &dialog);

        // Button width should accommodate the longer label ("Yes, Delete" = 11 chars)
        let expected_min_width = 11.0 * 8.0 + 16.0; // 11 chars * glyph_width + 2*padding
        assert!(
            geom.button_width >= expected_min_width,
            "Button width ({}) should be >= {} for label 'Yes, Delete'",
            geom.button_width,
            expected_min_width
        );
    }

    #[test]
    fn test_geometry_button_width_scales_with_label_length() {
        let short_dialog = ConfirmDialog::with_labels("?", "No", "Yes");
        let long_dialog = ConfirmDialog::with_labels("?", "Cancel", "Permanently Delete");

        let geom_short = calculate_confirm_dialog_geometry(800.0, 600.0, 16.0, 8.0, &short_dialog);
        let geom_long = calculate_confirm_dialog_geometry(800.0, 600.0, 16.0, 8.0, &long_dialog);

        assert!(
            geom_long.button_width > geom_short.button_width,
            "Longer labels should produce wider buttons"
        );
    }
}

// =============================================================================
// GPU Rendering Support
// =============================================================================

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLBuffer, MTLDevice, MTLResourceOptions};

use crate::glyph_atlas::{GlyphAtlas, GlyphInfo};
use crate::glyph_buffer::{GlyphLayout, GlyphVertex, QuadRange};
use crate::shader::VERTEX_SIZE;

// Colors for the confirm dialog (Catppuccin Mocha palette)
/// Dialog panel background color (dark surface)
const PANEL_BACKGROUND_COLOR: [f32; 4] = [0.11, 0.11, 0.15, 0.98]; // surface0 with slight transparency
/// Button background color (surface1)
const BUTTON_BACKGROUND_COLOR: [f32; 4] = [0.15, 0.15, 0.20, 1.0];
/// Selected button background color (accent)
const BUTTON_SELECTED_COLOR: [f32; 4] = [0.54, 0.36, 0.72, 1.0]; // mauve
/// Button text color (text)
const BUTTON_TEXT_COLOR: [f32; 4] = [0.804, 0.839, 0.957, 1.0];
/// Prompt text color (subtext1)
const PROMPT_TEXT_COLOR: [f32; 4] = [0.71, 0.75, 0.86, 1.0];

/// Manages vertex and index buffers for rendering the confirm dialog.
///
/// Similar to `FindStripGlyphBuffer` but specialized for the modal confirm dialog.
/// The dialog renders:
/// 1. Panel background
/// 2. Prompt text
/// 3. Cancel button (background + text)
/// 4. Abandon button (background + text)
///
/// The selected button gets a highlighted background.
pub struct ConfirmDialogGlyphBuffer {
    /// The vertex buffer containing quad vertices
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// The index buffer for drawing triangles
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// Total number of indices
    index_count: usize,
    /// Layout calculator for glyph positioning
    layout: GlyphLayout,

    // Quad ranges for different draw phases (all rendered with same color in this impl)
    /// Panel background quad
    panel_range: QuadRange,
    /// Cancel button background quad
    cancel_bg_range: QuadRange,
    /// Abandon button background quad
    abandon_bg_range: QuadRange,
    /// Prompt text glyphs
    prompt_range: QuadRange,
    /// Cancel button text glyphs
    cancel_text_range: QuadRange,
    /// Abandon button text glyphs
    abandon_text_range: QuadRange,
}

impl ConfirmDialogGlyphBuffer {
    /// Creates a new empty confirm dialog glyph buffer
    pub fn new(layout: GlyphLayout) -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            layout,
            panel_range: QuadRange::default(),
            cancel_bg_range: QuadRange::default(),
            abandon_bg_range: QuadRange::default(),
            prompt_range: QuadRange::default(),
            cancel_text_range: QuadRange::default(),
            abandon_text_range: QuadRange::default(),
        }
    }

    /// Returns the vertex buffer, if any
    pub fn vertex_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.vertex_buffer.as_deref()
    }

    /// Returns the index buffer, if any
    pub fn index_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.index_buffer.as_deref()
    }

    /// Returns the total number of indices
    #[allow(dead_code)]
    pub fn index_count(&self) -> usize {
        self.index_count
    }

    /// Returns the index range for the panel background quad
    pub fn panel_range(&self) -> QuadRange {
        self.panel_range
    }

    /// Returns the index range for the cancel button background
    pub fn cancel_bg_range(&self) -> QuadRange {
        self.cancel_bg_range
    }

    /// Returns the index range for the abandon button background
    pub fn abandon_bg_range(&self) -> QuadRange {
        self.abandon_bg_range
    }

    /// Returns the index range for prompt text glyphs
    pub fn prompt_range(&self) -> QuadRange {
        self.prompt_range
    }

    /// Returns the index range for cancel button text glyphs
    pub fn cancel_text_range(&self) -> QuadRange {
        self.cancel_text_range
    }

    /// Returns the index range for abandon button text glyphs
    pub fn abandon_text_range(&self) -> QuadRange {
        self.abandon_text_range
    }

    /// Updates the buffers with confirm dialog content
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas for text rendering
    /// * `dialog` - The confirm dialog state
    /// * `geometry` - The computed dialog geometry
    pub fn update(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        dialog: &ConfirmDialog,
        geometry: &ConfirmDialogGeometry,
    ) {
        // Estimate capacity: panel bg + 2 button bgs + prompt chars + cancel chars + confirm chars
        // Chunk: docs/chunks/generic_yes_no_modal - Use dialog's actual labels
        let prompt_len = dialog.prompt.chars().count();
        let cancel_len = dialog.cancel_label.len();
        let confirm_len = dialog.confirm_label.len();
        let estimated_quads = 3 + prompt_len + cancel_len + confirm_len;

        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(estimated_quads * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(estimated_quads * 6);
        let mut vertex_offset: u32 = 0;

        // Reset ranges
        self.panel_range = QuadRange::default();
        self.cancel_bg_range = QuadRange::default();
        self.abandon_bg_range = QuadRange::default();
        self.prompt_range = QuadRange::default();
        self.cancel_text_range = QuadRange::default();
        self.abandon_text_range = QuadRange::default();

        let solid_glyph = atlas.solid_glyph();

        // ==================== Phase 1: Panel Background ====================
        let panel_start = indices.len();
        {
            let quad = self.create_rect_quad(
                geometry.panel_x,
                geometry.panel_y,
                geometry.panel_width,
                geometry.panel_height,
                solid_glyph,
                PANEL_BACKGROUND_COLOR,
            );
            vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut indices, vertex_offset);
            vertex_offset += 4;
        }
        self.panel_range = QuadRange::new(panel_start, indices.len() - panel_start);

        // ==================== Phase 2: Cancel Button Background ====================
        let cancel_bg_start = indices.len();
        {
            let color = if dialog.selected == ConfirmButton::Cancel {
                BUTTON_SELECTED_COLOR
            } else {
                BUTTON_BACKGROUND_COLOR
            };
            let quad = self.create_rect_quad(
                geometry.cancel_button_x,
                geometry.buttons_y,
                geometry.button_width,
                geometry.button_height,
                solid_glyph,
                color,
            );
            vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut indices, vertex_offset);
            vertex_offset += 4;
        }
        self.cancel_bg_range = QuadRange::new(cancel_bg_start, indices.len() - cancel_bg_start);

        // ==================== Phase 3: Abandon Button Background ====================
        let abandon_bg_start = indices.len();
        {
            let color = if dialog.selected == ConfirmButton::Abandon {
                BUTTON_SELECTED_COLOR
            } else {
                BUTTON_BACKGROUND_COLOR
            };
            let quad = self.create_rect_quad(
                geometry.abandon_button_x,
                geometry.buttons_y,
                geometry.button_width,
                geometry.button_height,
                solid_glyph,
                color,
            );
            vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut indices, vertex_offset);
            vertex_offset += 4;
        }
        self.abandon_bg_range = QuadRange::new(abandon_bg_start, indices.len() - abandon_bg_start);

        // ==================== Phase 4: Prompt Text ====================
        let prompt_start = indices.len();
        {
            let glyph_width = self.layout.glyph_width;
            let mut x = geometry.prompt_x;
            let y = geometry.prompt_y - self.layout.line_height; // baseline adjustment

            for c in dialog.prompt.chars() {
                if c == ' ' {
                    x += glyph_width;
                    continue;
                }

                if let Some(glyph) = atlas.get_glyph(c) {
                    let quad = self.create_glyph_quad_at(x, y, glyph, PROMPT_TEXT_COLOR);
                    vertices.extend_from_slice(&quad);
                    Self::push_quad_indices(&mut indices, vertex_offset);
                    vertex_offset += 4;
                }
                x += glyph_width;
            }
        }
        self.prompt_range = QuadRange::new(prompt_start, indices.len() - prompt_start);

        // ==================== Phase 5: Cancel Button Text ====================
        // Chunk: docs/chunks/generic_yes_no_modal - Use dialog's actual cancel label
        let cancel_text_start = indices.len();
        {
            let glyph_width = self.layout.glyph_width;
            let text_width = dialog.cancel_label.len() as f32 * glyph_width;
            // Center the text in the button
            let x_start = geometry.cancel_button_x + (geometry.button_width - text_width) / 2.0;
            let mut x = x_start;
            let y = geometry.buttons_y + (geometry.button_height - self.layout.line_height) / 2.0;

            for c in dialog.cancel_label.chars() {
                if c == ' ' {
                    x += glyph_width;
                    continue;
                }

                if let Some(glyph) = atlas.get_glyph(c) {
                    let quad = self.create_glyph_quad_at(x, y, glyph, BUTTON_TEXT_COLOR);
                    vertices.extend_from_slice(&quad);
                    Self::push_quad_indices(&mut indices, vertex_offset);
                    vertex_offset += 4;
                }
                x += glyph_width;
            }
        }
        self.cancel_text_range = QuadRange::new(cancel_text_start, indices.len() - cancel_text_start);

        // ==================== Phase 6: Confirm Button Text ====================
        // Chunk: docs/chunks/generic_yes_no_modal - Use dialog's actual confirm label
        let abandon_text_start = indices.len();
        {
            let glyph_width = self.layout.glyph_width;
            let text_width = dialog.confirm_label.len() as f32 * glyph_width;
            // Center the text in the button
            let x_start = geometry.abandon_button_x + (geometry.button_width - text_width) / 2.0;
            let mut x = x_start;
            let y = geometry.buttons_y + (geometry.button_height - self.layout.line_height) / 2.0;

            for c in dialog.confirm_label.chars() {
                if c == ' ' {
                    x += glyph_width;
                    continue;
                }

                if let Some(glyph) = atlas.get_glyph(c) {
                    let quad = self.create_glyph_quad_at(x, y, glyph, BUTTON_TEXT_COLOR);
                    vertices.extend_from_slice(&quad);
                    Self::push_quad_indices(&mut indices, vertex_offset);
                    #[allow(unused_assignments)]
                    { vertex_offset += 4; }
                }
                x += glyph_width;
            }
        }
        self.abandon_text_range = QuadRange::new(abandon_text_start, indices.len() - abandon_text_start);

        // ==================== Create GPU Buffers ====================
        if vertices.is_empty() {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        // Create the vertex buffer
        let vertex_data_size = vertices.len() * VERTEX_SIZE;
        let vertex_ptr =
            NonNull::new(vertices.as_ptr() as *mut std::ffi::c_void).expect("vertex ptr not null");

        let vertex_buffer = unsafe {
            device
                .newBufferWithBytes_length_options(
                    vertex_ptr,
                    vertex_data_size,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create vertex buffer")
        };

        // Create the index buffer
        let index_data_size = indices.len() * std::mem::size_of::<u32>();
        let index_ptr =
            NonNull::new(indices.as_ptr() as *mut std::ffi::c_void).expect("index ptr not null");

        let index_buffer = unsafe {
            device
                .newBufferWithBytes_length_options(
                    index_ptr,
                    index_data_size,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create index buffer")
        };

        self.vertex_buffer = Some(vertex_buffer);
        self.index_buffer = Some(index_buffer);
        self.index_count = indices.len();
    }

    /// Creates a solid rectangle quad at the given position with the specified color
    fn create_rect_quad(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        solid_glyph: &GlyphInfo,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        let (u0, v0) = solid_glyph.uv_min;
        let (u1, v1) = solid_glyph.uv_max;

        [
            GlyphVertex::new(x, y, u0, v0, color),                 // top-left
            GlyphVertex::new(x + width, y, u1, v0, color),         // top-right
            GlyphVertex::new(x + width, y + height, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + height, u0, v1, color),        // bottom-left
        ]
    }

    /// Creates a glyph quad at an absolute position with the specified color
    fn create_glyph_quad_at(&self, x: f32, y: f32, glyph: &GlyphInfo, color: [f32; 4]) -> [GlyphVertex; 4] {
        let (u0, v0) = glyph.uv_min;
        let (u1, v1) = glyph.uv_max;

        let w = glyph.width;
        let h = glyph.height;

        [
            GlyphVertex::new(x, y, u0, v0, color),         // top-left
            GlyphVertex::new(x + w, y, u1, v0, color),     // top-right
            GlyphVertex::new(x + w, y + h, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + h, u0, v1, color),     // bottom-left
        ]
    }

    /// Pushes indices for a quad (two triangles)
    fn push_quad_indices(indices: &mut Vec<u32>, vertex_offset: u32) {
        // Triangle 1: top-left, top-right, bottom-right
        indices.push(vertex_offset);
        indices.push(vertex_offset + 1);
        indices.push(vertex_offset + 2);
        // Triangle 2: top-left, bottom-right, bottom-left
        indices.push(vertex_offset);
        indices.push(vertex_offset + 2);
        indices.push(vertex_offset + 3);
    }
}
